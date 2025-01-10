//! Public traits.

use std::cmp::Ordering;

use eyeball_im::{
    VectorDiff, VectorSubscriber, VectorSubscriberBatchedStream, VectorSubscriberStream,
};
use futures_core::Stream;
use imbl::Vector;

use super::{
    ops::{
        VecVectorDiffFamily, VectorDiffContainerFamily, VectorDiffContainerOps, VectorDiffFamily,
    },
    EmptyLimitStream, Filter, FilterMap, Limit, RLimit, Sort, SortBy, SortByKey,
};

/// Abstraction over stream items that the adapters in this module can deal
/// with.
pub trait VectorDiffContainer:
    VectorDiffContainerOps<Self::Element, Family = <Self as VectorDiffContainer>::Family>
{
    /// The element type of the [`Vector`][imbl::Vector] that diffs are being
    /// handled for.
    type Element: Clone + 'static;

    #[doc(hidden)]
    type Family: VectorDiffContainerFamily<Member<Self::Element> = Self>;
}

impl<T: Clone + 'static> VectorDiffContainer for VectorDiff<T> {
    type Element = T;
    type Family = VectorDiffFamily;
}

impl<T: Clone + 'static> VectorDiffContainer for Vec<VectorDiff<T>> {
    type Element = T;
    type Family = VecVectorDiffFamily;
}

/// Extension trait for [`VectorSubscriber`].
pub trait VectorSubscriberExt<T> {
    /// Create a [`BatchedVectorSubscriber`] from `self`.
    fn batched(self) -> BatchedVectorSubscriber<T>;
}

impl<T> VectorSubscriberExt<T> for VectorSubscriber<T> {
    fn batched(self) -> BatchedVectorSubscriber<T> {
        BatchedVectorSubscriber { inner: self }
    }
}

/// A wrapper around [`VectorSubscriber`] with a different [`VectorObserver`]
/// impl.
#[derive(Debug)]
pub struct BatchedVectorSubscriber<T> {
    inner: VectorSubscriber<T>,
}

/// Abstraction over types that hold both a [`Vector`] and a stream of
/// [`VectorDiff`] updates.
///
/// See [`VectorObserverExt`] for operations available to implementers.
pub trait VectorObserver<T>: Sized {
    #[doc(hidden)]
    type Stream: Stream;

    #[doc(hidden)]
    fn into_parts(self) -> (Vector<T>, Self::Stream);
}

impl<T: Clone + 'static> VectorObserver<T> for VectorSubscriber<T> {
    type Stream = VectorSubscriberStream<T>;

    fn into_parts(self) -> (Vector<T>, Self::Stream) {
        self.into_values_and_stream()
    }
}

impl<T: Clone + 'static> VectorObserver<T> for BatchedVectorSubscriber<T> {
    type Stream = VectorSubscriberBatchedStream<T>;

    fn into_parts(self) -> (Vector<T>, Self::Stream) {
        self.inner.into_values_and_batched_stream()
    }
}

impl<T, S> VectorObserver<T> for (Vector<T>, S)
where
    S: Stream,
    S::Item: VectorDiffContainer,
{
    type Stream = S;

    fn into_parts(self) -> (Vector<T>, Self::Stream) {
        self
    }
}

/// Convenience methods for [`VectorObserver`]s.
///
/// See that trait for which types implement this.
pub trait VectorObserverExt<T>: VectorObserver<T>
where
    T: Clone + 'static,
    <Self::Stream as Stream>::Item: VectorDiffContainer<Element = T>,
{
    /// Filter the vector's values with the given function.
    fn filter<F>(self, f: F) -> (Vector<T>, Filter<Self::Stream, F>)
    where
        F: Fn(&T) -> bool,
    {
        let (items, stream) = self.into_parts();
        Filter::new(items, stream, f)
    }

    /// Filter and map the vector's values with the given function.
    fn filter_map<U, F>(self, f: F) -> (Vector<U>, FilterMap<Self::Stream, F>)
    where
        U: Clone,
        F: Fn(T) -> Option<U>,
    {
        let (items, stream) = self.into_parts();
        FilterMap::new(items, stream, f)
    }

    /// Limit the observed values to `limit`.
    ///
    /// See [`Limit`] for more details.
    fn limit(self, limit: usize) -> (Vector<T>, Limit<Self::Stream, EmptyLimitStream>) {
        let (items, stream) = self.into_parts();
        Limit::new(items, stream, limit)
    }

    /// Limit the observed values to a number of items determined by the given
    /// stream.
    ///
    /// See [`Limit`] for more details.
    fn dynamic_limit<L>(self, limit_stream: L) -> Limit<Self::Stream, L>
    where
        L: Stream<Item = usize>,
    {
        let (items, stream) = self.into_parts();
        Limit::dynamic(items, stream, limit_stream)
    }

    /// Limit the observed values to `initial_limit` items initially, and update
    /// the limit with the value from the given stream.
    ///
    /// See [`Limit`] for more details.
    fn dynamic_limit_with_initial_value<L>(
        self,
        initial_limit: usize,
        limit_stream: L,
    ) -> (Vector<T>, Limit<Self::Stream, L>)
    where
        L: Stream<Item = usize>,
    {
        let (items, stream) = self.into_parts();
        Limit::dynamic_with_initial_limit(items, stream, initial_limit, limit_stream)
    }

    /// Limit from the back the observed values to `limit`.
    ///
    /// See [`RLimit`] for more details.
    fn rlimit(self, limit: usize) -> (Vector<T>, RLimit<Self::Stream, EmptyLimitStream>) {
        let (items, stream) = self.into_parts();
        RLimit::new(items, stream, limit)
    }

    /// Limit from the back the observed values to a number of items determined
    /// by the given stream.
    ///
    /// See [`RLimit`] for more details.
    fn dynamic_rlimit<L>(self, limit_stream: L) -> RLimit<Self::Stream, L>
    where
        L: Stream<Item = usize>,
    {
        let (items, stream) = self.into_parts();
        RLimit::dynamic(items, stream, limit_stream)
    }

    /// Limit from the back the observed values to `initial_limit` items
    /// initially, and update the limit with the value from the given
    /// stream.
    ///
    /// See [`RLimit`] for more details.
    fn dynamic_rlimit_with_initial_value<L>(
        self,
        initial_limit: usize,
        limit_stream: L,
    ) -> (Vector<T>, RLimit<Self::Stream, L>)
    where
        L: Stream<Item = usize>,
    {
        let (items, stream) = self.into_parts();
        RLimit::dynamic_with_initial_limit(items, stream, initial_limit, limit_stream)
    }

    /// Sort the observed values.
    ///
    /// See [`Sort`] for more details.
    fn sort(self) -> (Vector<T>, Sort<Self::Stream>)
    where
        T: Ord,
    {
        let (items, stream) = self.into_parts();
        Sort::new(items, stream)
    }

    /// Sort the observed values with the given comparison function.
    ///
    /// See [`SortBy`] for more details.
    fn sort_by<F>(self, compare: F) -> (Vector<T>, SortBy<Self::Stream, F>)
    where
        F: Fn(&T, &T) -> Ordering,
    {
        let (items, stream) = self.into_parts();
        SortBy::new(items, stream, compare)
    }

    /// Sort the observed values with the given key function.
    ///
    /// See [`SortBy`] for more details.
    fn sort_by_key<F, K>(self, key_fn: F) -> (Vector<T>, SortByKey<Self::Stream, F>)
    where
        F: Fn(&T) -> K,
        K: Ord,
    {
        let (items, stream) = self.into_parts();
        SortByKey::new(items, stream, key_fn)
    }
}

impl<T, O> VectorObserverExt<T> for O
where
    T: Clone + 'static,
    O: VectorObserver<T>,
    <Self::Stream as Stream>::Item: VectorDiffContainer<Element = T>,
{
}
