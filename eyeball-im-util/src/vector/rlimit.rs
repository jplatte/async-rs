use smallvec::SmallVec;
use std::{
    cmp::{min, Ordering},
    iter::repeat,
    mem,
    pin::Pin,
    task::{self, Poll},
};

use super::{
    EmptyLimitStream, VectorDiffContainer, VectorDiffContainerOps,
    VectorDiffContainerStreamElement, VectorDiffContainerStreamRLimitBuf, VectorObserver,
};
use eyeball_im::VectorDiff;
use futures_core::Stream;
use imbl::Vector;
use pin_project_lite::pin_project;

pin_project! {
    /// A [`VectorDiff`] stream adapter that presents a _reversed_ limited view
    /// of the  underlying [`ObservableVector`]s items. The view starts from the
    /// last index of the `ObservableVector`, i.e. it starts from the end. This
    /// is the opposite of [`Limit`][super::Limit], which starts from 0.
    ///
    /// For example, let `S` be a `Stream<Item = VectorDiff>`. The [`Vector`]
    /// represented by `S` can have any length, but one may want to virtually
    /// _limit_ this `Vector` to a certain size. Then this `RLimit` adapter is
    /// appropriate.
    ///
    /// An internal buffered vector is kept so that the adapter knows which
    /// values can be added when the limit is increased, or when values are
    /// removed and new values must be inserted. This fact is important if the
    /// items of the `Vector` have a non-negligible size.
    ///
    /// It's okay to have a limit larger than the length of the observed
    /// `Vector`.
    ///
    /// [`ObservableVector`]: eyeball_im::ObservableVector
    #[project = RLimitProj]
    pub struct RLimit<S, L>
    where
        S: Stream,
        S::Item: VectorDiffContainer,
    {
        // The main stream to poll items from.
        #[pin]
        inner_stream: S,

        // The limit stream to poll new limits from.
        #[pin]
        limit_stream: L,

        // The buffered vector that is updated with the main stream's items.
        // It's used to provide missing items, e.g. when the limit increases.
        buffered_vector: Vector<VectorDiffContainerStreamElement<S>>,

        // The current limit.
        limit: usize,

        // This adapter is not a basic filter: It can produce up to two items
        // per item of the underlying stream.
        //
        // Thus, if the item type is just `VectorDiff<_>` (non-bached, can't
        // just add diffs to a poll_next result), we need a buffer to store the
        // possible extra item in. For example if the vector is [10, 11, 12]
        // with a limit of 2 on top: if an item is popped at the back then 12
        // is removed, but 10 has to be pushed front as it "enters" the "view".
        // That second `PushFront` diff is buffered here.
        ready_values: VectorDiffContainerStreamRLimitBuf<S>,
    }
}

impl<S> RLimit<S, EmptyLimitStream>
where
    S: Stream,
    S::Item: VectorDiffContainer,
{
    /// Create a new [`RLimit`] with the given (unlimited) initial values,
    /// stream of `VectorDiff` updates for those values, and a fixed limit.
    ///
    /// Returns the truncated initial values as well as a stream of updates that
    /// ensure that the resulting vector never exceeds the given limit.
    pub fn new(
        initial_values: Vector<VectorDiffContainerStreamElement<S>>,
        inner_stream: S,
        limit: usize,
    ) -> (Vector<VectorDiffContainerStreamElement<S>>, Self) {
        Self::dynamic_with_initial_limit(initial_values, inner_stream, limit, EmptyLimitStream)
    }
}

impl<S, L> RLimit<S, L>
where
    S: Stream,
    S::Item: VectorDiffContainer,
    L: Stream<Item = usize>,
{
    /// Create a new [`RLimit`] with the given (unlimited) initial values,
    /// stream of `VectorDiff` updates for those values, and a stream of
    /// limits.
    ///
    /// This is equivalent to `dynamic_with_initial_limit` where the
    /// `initial_limit` is 0, except that it doesn't return the limited
    /// vector as it would be empty anyways.
    ///
    /// Note that the returned `RLimit` won't produce anything until the first
    /// limit is produced by the limit stream.
    pub fn dynamic(
        initial_values: Vector<VectorDiffContainerStreamElement<S>>,
        inner_stream: S,
        limit_stream: L,
    ) -> Self {
        Self {
            inner_stream,
            limit_stream,
            buffered_vector: initial_values,
            limit: 0,
            ready_values: Default::default(),
        }
    }

    /// Create a new [`RLimit`] with the given (unlimited) initial values,
    /// stream of `VectorDiff` updates for those values, and an initial
    /// limit as well as a stream of new limits.
    pub fn dynamic_with_initial_limit(
        initial_values: Vector<VectorDiffContainerStreamElement<S>>,
        inner_stream: S,
        initial_limit: usize,
        limit_stream: L,
    ) -> (Vector<VectorDiffContainerStreamElement<S>>, Self) {
        let buffered_vector = initial_values.clone();

        let initial_values = if initial_limit < initial_values.len() {
            initial_values.truncate_from_end(initial_limit)
        } else {
            initial_values
        };

        let stream = Self {
            inner_stream,
            limit_stream,
            buffered_vector,
            limit: initial_limit,
            ready_values: Default::default(),
        };

        (initial_values, stream)
    }
}

impl<S, L> Stream for RLimit<S, L>
where
    S: Stream,
    S::Item: VectorDiffContainer,
    L: Stream<Item = usize>,
{
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().poll_next(cx)
    }
}

impl<S, L> VectorObserver<VectorDiffContainerStreamElement<S>> for RLimit<S, L>
where
    S: Stream,
    S::Item: VectorDiffContainer,
    L: Stream<Item = usize>,
{
    type Stream = Self;

    fn into_parts(self) -> (Vector<VectorDiffContainerStreamElement<S>>, Self::Stream) {
        (self.buffered_vector.clone(), self)
    }
}

impl<S, L> RLimitProj<'_, S, L>
where
    S: Stream,
    S::Item: VectorDiffContainer,
    L: Stream<Item = usize>,
{
    fn poll_next(&mut self, cx: &mut task::Context<'_>) -> Poll<Option<S::Item>> {
        loop {
            // First off, if any values are ready, return them.
            if let Some(value) = S::Item::pop_from_rlimit_buf(self.ready_values) {
                return Poll::Ready(Some(value));
            }

            // Poll `VectorDiff`s from the `inner_stream`.
            match self.inner_stream.as_mut().poll_next(cx) {
                Poll::Ready(Some(diffs)) => {
                    // Consume and apply the diffs if possible.
                    let ready = diffs.push_into_rlimit_buf(self.ready_values, |diff| {
                        let limit = *self.limit;
                        let prev_len = self.buffered_vector.len();

                        // Update the `buffered_vector`. It's a replica of the original observed
                        // `Vector`. We need to maintain it in order to be able to produce valid
                        // `VectorDiff`s when items are missing.
                        diff.clone().apply(self.buffered_vector);

                        // Handle `diff`.
                        handle_diff(diff, limit, prev_len, self.buffered_vector)
                    });

                    if let Some(diff) = ready {
                        return Poll::Ready(Some(diff));
                    }
                }

                // `inner_stream` has been closed…
                Poll::Ready(None) => {
                    // … so let's close this stream too.
                    return Poll::Ready(None);
                }

                // `inner_stream` is pending…
                Poll::Pending => {
                    // … so continue by polling `limit_stream`.
                }
            }

            // Poll a new limit from `limit_stream`.
            match self.limit_stream.as_mut().poll_next(cx) {
                // A new limit has been polled!
                Poll::Ready(Some(next_limit)) => {
                    // Update the limit and emit `VectorDiff`s accordingly.
                    if let Some(diffs) = self.update_limit(next_limit) {
                        return Poll::Ready(S::Item::extend_rlimit_buf(diffs, self.ready_values));
                    }

                    // If `update_limit` returned `None`, poll the streams
                    // again.
                }
                // `limit_stream` has been closed…
                Poll::Ready(None) => {
                    // … that's fine! Since `inner_stream` is pending, and `limit_stream` is closed,
                    // let's consider this stream is pending.
                    return Poll::Pending;
                }
                // `limit_stream` is pending…
                Poll::Pending => {
                    // … which also means `inner_stream` is pending too, so let's consider this
                    // stream is pending entirely.
                    return Poll::Pending;
                }
            }

            // Else loop and poll the streams again.
        }
    }

    /// Update the limit if necessary.
    ///
    /// * If the buffered vector is empty, it returns `None`.
    /// * If the limit increases, `VectorDiff::PushFront`s or a
    ///   `VectorDiff::Append` are produced if any items exist.
    /// * If the limit decreases below the length of the vector,
    ///   `VectorDiff::PopFront`s are produced.
    ///
    /// It's OK to have a `new_limit` larger than the length of the `Vector`.
    /// The `new_limit` won't be capped.
    fn update_limit(
        &mut self,
        new_limit: usize,
    ) -> Option<Vec<VectorDiff<VectorDiffContainerStreamElement<S>>>> {
        // Let's update the limit.
        let old_limit = mem::replace(self.limit, new_limit);

        if self.buffered_vector.is_empty() {
            // If empty, nothing to do.
            return None;
        }

        match old_limit.cmp(&new_limit) {
            // old < new
            Ordering::Less => {
                let mut missing_items = self
                    .buffered_vector
                    .iter()
                    .rev()
                    .skip(old_limit)
                    .take(new_limit - old_limit)
                    .cloned()
                    .peekable();

                if missing_items.peek().is_none() {
                    None
                } else {
                    // Let's add the missing items.
                    //
                    // Optimisations:
                    // - if `old_limit` is 0, we can emit a `VectorDiff::Append` to append all
                    //   missing values,
                    // - otherwise, we emit a bunch of `VectorDiff::PushFront` in reverse order.
                    if old_limit == 0 {
                        Some(vec![VectorDiff::Append { values: missing_items.rev().collect() }])
                    } else {
                        Some(
                            missing_items
                                .map(|missing_item| VectorDiff::PushFront { value: missing_item })
                                .collect(),
                        )
                    }
                }
            }

            // old > new
            Ordering::Greater => {
                if self.buffered_vector.len() <= new_limit {
                    None
                } else {
                    // Let's remove the extra items.
                    //
                    // Optimisations:
                    // - if `new_limit` is 0, we can emit a `VectorDiff::Clear` to remove all values
                    //   at once,
                    // - otherwise, we emit a bunch of `VectorDiff::PopFront`.
                    if new_limit == 0 {
                        Some(vec![VectorDiff::Clear])
                    } else {
                        Some(repeat(VectorDiff::PopFront).take(old_limit - new_limit).collect())
                    }
                }
            }

            // old == new
            Ordering::Equal => {
                // Nothing to do.
                None
            }
        }
    }
}

fn handle_diff<T: Clone>(
    diff: VectorDiff<T>,
    limit: usize,
    previous_length: usize,
    buffered_vector: &Vector<T>,
) -> SmallVec<[VectorDiff<T>; 2]> {
    // If the limit is zero, we have nothing to do.
    if limit == 0 {
        return SmallVec::new();
    }

    let index_of_limit = previous_length.saturating_sub(limit);
    let is_full = previous_length >= limit;
    let mut res = SmallVec::new();

    match diff {
        VectorDiff::Append { values } => {
            let values = values.truncate_from_end(limit);

            res.extend(
                repeat(VectorDiff::PopFront).take(min(
                    values.len(),
                    (previous_length + values.len()).saturating_sub(limit),
                )),
            );
            res.push(VectorDiff::Append { values });
        }

        VectorDiff::Clear => {
            res.push(VectorDiff::Clear);
        }

        VectorDiff::PushFront { value } => {
            if is_full {
                // Ignore the diff.
            } else {
                // There is space for this new item.
                res.push(VectorDiff::PushFront { value });
            }
        }

        VectorDiff::PushBack { value } => {
            if is_full {
                // Create 1 free space.
                res.push(VectorDiff::PopFront);
            }

            // There is space for this new item.
            res.push(VectorDiff::PushBack { value });
        }

        VectorDiff::PopFront => {
            if previous_length > limit {
                // Pop front outside the limit, ignore the diff.
            } else {
                res.push(VectorDiff::PopFront);
            }
        }

        VectorDiff::PopBack => {
            res.push(VectorDiff::PopBack);

            if previous_length > limit {
                if let Some(diff) = buffered_vector.get(index_of_limit.saturating_sub(1)) {
                    // There is a previously-truncated item, push front.
                    res.push(VectorDiff::PushFront { value: diff.clone() });
                }
            }
        }

        VectorDiff::Insert { index, value } => {
            if limit > previous_length || index > index_of_limit {
                if is_full {
                    // Create 1 free space.
                    res.push(VectorDiff::PopFront);
                }

                // There is space for this new item.
                res.push(VectorDiff::Insert {
                    // Subtract 1 because `insert` adds a value compared to `previous_length`.
                    index: (index - index_of_limit).saturating_sub(1),
                    value,
                });
            } else {
                // Insert before `limit`, ignore the diff.
            }
        }

        VectorDiff::Set { index, value } => {
            if index >= index_of_limit {
                res.push(VectorDiff::Set { index: index - index_of_limit, value });
            } else {
                // Update before `limit`, ignore the diff.
            }
        }

        VectorDiff::Remove { index } => {
            if index >= index_of_limit {
                let remove_index = index - index_of_limit;
                res.push(VectorDiff::Remove { index: remove_index });

                if remove_index != index {
                    if let Some(diff) = buffered_vector.get(index_of_limit.saturating_sub(1)) {
                        // There is a previously-truncated item, push front.
                        res.push(VectorDiff::PushFront { value: diff.clone() });
                    }
                }
            } else {
                // Remove before `limit`, ignore the diff.
            }
        }

        VectorDiff::Truncate { length: new_length } => {
            let number_of_removed_values = min(limit, previous_length - new_length);

            res.extend(repeat(VectorDiff::PopBack).take(number_of_removed_values));
            res.extend(
                buffered_vector
                    .iter()
                    .rev()
                    .skip(limit - number_of_removed_values)
                    .take(number_of_removed_values)
                    .cloned()
                    .map(|value| VectorDiff::PushFront { value }),
            );
        }

        VectorDiff::Reset { values: new_values } => {
            let new_values = new_values.truncate_from_end(limit);

            // There is space for these new items.
            res.push(VectorDiff::Reset { values: new_values });
        }
    }

    res
}

trait TruncateFromEnd {
    fn truncate_from_end(self, len: usize) -> Self;
}

impl<T> TruncateFromEnd for Vector<T>
where
    T: Clone,
{
    fn truncate_from_end(self, len: usize) -> Self {
        if len == 0 {
            return Vector::new();
        }

        let index = self.len().saturating_sub(len);

        // Avoid calling `Vector::split_at`.
        if index == 0 {
            return self;
        }

        let (_left, right) = self.split_at(index);

        right
    }
}

#[cfg(test)]
mod tests {
    use super::TruncateFromEnd;
    use imbl::vector;

    #[test]
    fn test_truncate_from_end() {
        // Length is 0.
        assert_eq!(vector![1, 2, 3, 4].truncate_from_end(0), vector![]);

        // Length is smaller than the values.
        assert_eq!(vector![1, 2, 3, 4].truncate_from_end(1), vector![4]);

        // Length is equal to the number of values.
        assert_eq!(vector![1, 2, 3, 4].truncate_from_end(4), vector![1, 2, 3, 4]);

        // Length is larger than the number of values.
        assert_eq!(vector![1, 2, 3, 4].truncate_from_end(6), vector![1, 2, 3, 4]);
    }
}
