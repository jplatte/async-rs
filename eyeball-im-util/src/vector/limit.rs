use arrayvec::ArrayVec;
use std::{
    cmp::{min, Ordering},
    mem,
    pin::Pin,
    task::{self, ready, Poll},
};

use super::{
    VectorDiffContainer, VectorDiffContainerOps, VectorDiffContainerStreamElement,
    VectorDiffContainerStreamLimitBuf, VectorObserver,
};
use eyeball_im::VectorDiff;
use futures_core::Stream;
use imbl::Vector;
use pin_project_lite::pin_project;

pin_project! {
    /// A [`VectorDiff`] stream adapter that presents a limited view of the
    /// underlying [`ObservableVector`]'s items. The view starts from index 0.
    ///
    /// For example, let `S` be a `Stream<Item = VectorDiff>`. The [`Vector`]
    /// represented by `S` can have any length, but one may want to virtually
    /// _limit_ this `Vector` to a certain size. Then this `Limit` adapter is
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
    #[project = LimitProj]
    pub struct Limit<S, L>
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
        // with a limit of 2 on top: if an item is popped at the front then 10
        // is removed, but 12 has to be pushed back as it "enters" the "view".
        // That second `PushBack` diff is buffered here.
        ready_values: VectorDiffContainerStreamLimitBuf<S>,
    }
}

impl<S> Limit<S, EmptyLimitStream>
where
    S: Stream,
    S::Item: VectorDiffContainer,
{
    /// Create a new [`Limit`] with the given (unlimited) initial values,
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

impl<S, L> Limit<S, L>
where
    S: Stream,
    S::Item: VectorDiffContainer,
    L: Stream<Item = usize>,
{
    /// Create a new [`Limit`] with the given (unlimited) initial values, stream
    /// of `VectorDiff` updates for those values, and a stream of limits.
    ///
    /// This is equivalent to `dynamic_with_initial_limit` where the
    /// `initial_limit` is 0, except that it doesn't return the limited
    /// vector as it would be empty anyways.
    ///
    /// Note that the returned `Limit` won't produce anything until the first
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

    /// Create a new [`Limit`] with the given (unlimited) initial values, stream
    /// of `VectorDiff` updates for those values, and an initial limit as well
    /// as a stream of new limits.
    pub fn dynamic_with_initial_limit(
        mut initial_values: Vector<VectorDiffContainerStreamElement<S>>,
        inner_stream: S,
        initial_limit: usize,
        limit_stream: L,
    ) -> (Vector<VectorDiffContainerStreamElement<S>>, Self) {
        let buffered_vector = initial_values.clone();
        if initial_limit < initial_values.len() {
            initial_values.truncate(initial_limit);
        }

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

impl<S, L> Stream for Limit<S, L>
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

impl<S, L> VectorObserver<VectorDiffContainerStreamElement<S>> for Limit<S, L>
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

impl<S, L> LimitProj<'_, S, L>
where
    S: Stream,
    S::Item: VectorDiffContainer,
    L: Stream<Item = usize>,
{
    fn poll_next(&mut self, cx: &mut task::Context<'_>) -> Poll<Option<S::Item>> {
        loop {
            // First off, if any values are ready, return them.
            if let Some(value) = S::Item::pop_from_limit_buf(self.ready_values) {
                return Poll::Ready(Some(value));
            }

            // Poll a new limit from `limit_stream` before polling `inner_stream`.
            while let Poll::Ready(Some(next_limit)) = self.limit_stream.as_mut().poll_next(cx) {
                // We have new `VectorDiff`s after the limit has been updated.
                // Return them.
                if let Some(diffs) = self.update_limit(next_limit) {
                    return Poll::Ready(Some(diffs));
                }

                // If `update_limit` returned `None`, poll the limit stream
                // again.
            }

            // Poll `VectorDiff`s from the `inner_stream`.
            let Some(diffs) = ready!(self.inner_stream.as_mut().poll_next(cx)) else {
                return Poll::Ready(None);
            };

            // Consume and apply the diffs if possible.
            let ready = diffs.push_into_limit_buf(self.ready_values, |diff| {
                let limit = *self.limit;
                let prev_len = self.buffered_vector.len();

                // Update the `buffered_vector`. It's a replica of the original observed
                // `Vector`. We need to maintain it in order to be able to produce valid
                // `VectorDiff`s when items are missing.
                diff.clone().apply(self.buffered_vector);

                // Handle the `diff`.
                handle_diff(diff, limit, prev_len, self.buffered_vector)
            });

            if let Some(diff) = ready {
                return Poll::Ready(Some(diff));
            }

            // Else loop and poll the streams again.
        }
    }

    /// Update the limit if necessary.
    ///
    /// * If the buffered vector is empty, it returns `None`.
    /// * If the limit increases, a `VectorDiff::Append` is produced if any
    ///   items exist.
    /// * If the limit decreases below the length of the vector, a
    ///   `VectorDiff::Truncate` is produced.
    ///
    /// It's OK to have a `new_limit` larger than the length of the `Vector`.
    /// The `new_limit` won't be capped.
    fn update_limit(&mut self, new_limit: usize) -> Option<S::Item> {
        // Let's update the limit.
        let old_limit = mem::replace(self.limit, new_limit);

        if self.buffered_vector.is_empty() {
            // If empty, nothing to do.
            return None;
        }

        match old_limit.cmp(&new_limit) {
            // old < new
            Ordering::Less => {
                let missing_items = self
                    .buffered_vector
                    .iter()
                    .skip(old_limit)
                    .take(new_limit - old_limit)
                    .cloned()
                    .collect::<Vector<_>>();

                if missing_items.is_empty() {
                    None
                } else {
                    // Let's add the missing items.
                    Some(S::Item::from_item(VectorDiff::Append { values: missing_items }))
                }
            }

            // old > new
            Ordering::Greater => {
                if self.buffered_vector.len() <= new_limit {
                    None
                } else {
                    // Let's remove the extra items.
                    Some(S::Item::from_item(VectorDiff::Truncate { length: new_limit }))
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

/// An empty stream with an item type of `usize`.
#[derive(Debug)]
#[non_exhaustive]
pub struct EmptyLimitStream;

impl Stream for EmptyLimitStream {
    type Item = usize;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut task::Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(None)
    }
}

fn handle_diff<T: Clone>(
    diff: VectorDiff<T>,
    limit: usize,
    prev_len: usize,
    buffered_vector: &Vector<T>,
) -> ArrayVec<VectorDiff<T>, 2> {
    // If the limit is zero, we have nothing to do.
    if limit == 0 {
        return ArrayVec::new();
    }

    let is_full = prev_len >= limit;
    let mut res = ArrayVec::new();

    match diff {
        VectorDiff::Append { mut values } => {
            if is_full {
                // Ignore the diff.
            } else {
                // Truncate the `values` to fit inside the free space.
                values.truncate(min(limit - prev_len, values.len()));
                res.push(VectorDiff::Append { values });
            }
        }
        VectorDiff::Clear => {
            res.push(VectorDiff::Clear);
        }
        VectorDiff::PushFront { value } => {
            if is_full {
                // Create 1 free space.
                res.push(VectorDiff::PopBack);
            }

            // There is space for this new item.
            res.push(VectorDiff::PushFront { value });
        }
        VectorDiff::PushBack { value } => {
            if is_full {
                // Ignore the diff.
            } else {
                // There is space for this new item.
                res.push(VectorDiff::PushBack { value });
            }
        }
        VectorDiff::PopFront => {
            res.push(VectorDiff::PopFront);

            if let Some(diff) = buffered_vector.get(limit - 1) {
                // There is a previously-truncated item, push back.
                res.push(VectorDiff::PushBack { value: diff.clone() });
            }
        }
        VectorDiff::PopBack => {
            if prev_len > limit {
                // Pop back outside the limit, ignore the diff.
            } else {
                res.push(VectorDiff::PopBack);
            }
        }
        VectorDiff::Insert { index, value } => {
            if index >= limit {
                // Insert after `limit`, ignore the diff.
            } else {
                if is_full {
                    // Create 1 free space.
                    res.push(VectorDiff::PopBack);
                }

                // There is space for this new item.
                res.push(VectorDiff::Insert { index, value });
            }
        }
        VectorDiff::Set { index, value } => {
            if index >= limit {
                // Update after `limit`, ignore the diff.
            } else {
                res.push(VectorDiff::Set { index, value });
            }
        }
        VectorDiff::Remove { index } => {
            if index >= limit {
                // Remove after `limit`, ignore the diff.
            } else {
                res.push(VectorDiff::Remove { index });

                if let Some(diff) = buffered_vector.get(limit - 1) {
                    // There is a previously-truncated item, push back.
                    res.push(VectorDiff::PushBack { value: diff.clone() });
                }
            }
        }
        VectorDiff::Truncate { length: new_length } => {
            if new_length >= limit {
                // Truncate items after `limit`, ignore the diff.
            } else {
                res.push(VectorDiff::Truncate { length: new_length });
            }
        }
        VectorDiff::Reset { values: mut new_values } => {
            if new_values.len() > limit {
                // There are too many values, truncate.
                new_values.truncate(limit);
            }

            // There is space for these new items.
            res.push(VectorDiff::Reset { values: new_values });
        }
    }

    res
}
