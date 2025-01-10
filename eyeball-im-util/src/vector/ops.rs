use arrayvec::ArrayVec;
use eyeball_im::VectorDiff;
use smallvec::SmallVec;

pub trait VectorDiffContainerOps<T>: Sized {
    type Family: VectorDiffContainerFamily;
    type LimitBuf: Default;
    type RLimitBuf: Default;
    type SortBuf: Default;

    fn from_item(vector_diff: VectorDiff<T>) -> Self;

    fn filter_map<U>(
        self,
        f: impl FnMut(VectorDiff<T>) -> Option<VectorDiff<U>>,
    ) -> Option<VectorDiffContainerFamilyMember<Self::Family, U>>;

    fn push_into_limit_buf(
        self,
        buffer: &mut Self::LimitBuf,
        map_diffs: impl FnMut(VectorDiff<T>) -> ArrayVec<VectorDiff<T>, 2>,
    ) -> Option<Self>;

    fn pop_from_limit_buf(buffer: &mut Self::LimitBuf) -> Option<Self>;

    fn push_into_rlimit_buf(
        self,
        buffer: &mut Self::RLimitBuf,
        map_diffs: impl FnMut(VectorDiff<T>) -> SmallVec<[VectorDiff<T>; 2]>,
    ) -> Option<Self>;

    fn extend_rlimit_buf(diffs: Vec<VectorDiff<T>>, buffer: &mut Self::RLimitBuf) -> Option<Self>;

    fn pop_from_rlimit_buf(buffer: &mut Self::RLimitBuf) -> Option<Self>;

    fn push_into_sort_buf(
        self,
        buffer: &mut Self::SortBuf,
        map_diffs: impl FnMut(VectorDiff<T>) -> SmallVec<[VectorDiff<T>; 2]>,
    ) -> Option<Self>;

    fn pop_from_sort_buf(buffer: &mut Self::SortBuf) -> Option<Self>;
}

#[allow(unreachable_pub)]
pub type VectorDiffContainerFamilyMember<F, U> = <F as VectorDiffContainerFamily>::Member<U>;

impl<T> VectorDiffContainerOps<T> for VectorDiff<T> {
    type Family = VectorDiffFamily;
    type LimitBuf = Option<VectorDiff<T>>;
    type RLimitBuf = SmallVec<[VectorDiff<T>; 2]>;
    type SortBuf = SmallVec<[VectorDiff<T>; 2]>;

    fn from_item(vector_diff: VectorDiff<T>) -> Self {
        vector_diff
    }

    fn filter_map<U>(
        self,
        mut f: impl FnMut(VectorDiff<T>) -> Option<VectorDiff<U>>,
    ) -> Option<VectorDiffContainerFamilyMember<Self::Family, U>> {
        f(self)
    }

    fn push_into_limit_buf(
        self,
        buffer: &mut Self::LimitBuf,
        mut map_diffs: impl FnMut(VectorDiff<T>) -> ArrayVec<VectorDiff<T>, 2>,
    ) -> Option<Self> {
        assert!(buffer.is_none(), "buffer must be None when calling push_into_limit_buf");

        let mut diffs = map_diffs(self);

        let last = diffs.pop();
        if let Some(first) = diffs.pop() {
            *buffer = last;
            Some(first)
        } else {
            last
        }
    }

    fn pop_from_limit_buf(buffer: &mut Self::LimitBuf) -> Option<Self> {
        buffer.take()
    }

    fn push_into_rlimit_buf(
        self,
        buffer: &mut Self::RLimitBuf,
        mut map_diffs: impl FnMut(VectorDiff<T>) -> SmallVec<[VectorDiff<T>; 2]>,
    ) -> Option<Self> {
        buffer.insert_many(0, map_diffs(self).into_iter().rev());

        buffer.pop()
    }

    fn extend_rlimit_buf(diffs: Vec<VectorDiff<T>>, buffer: &mut Self::RLimitBuf) -> Option<Self> {
        // We cannot pop front on a `SmallVec`. We store all `diffs` in reverse order to
        // pop from it.
        buffer.insert_many(0, diffs.into_iter().rev());

        buffer.pop()
    }

    fn pop_from_rlimit_buf(buffer: &mut Self::RLimitBuf) -> Option<Self> {
        buffer.pop()
    }

    fn push_into_sort_buf(
        self,
        buffer: &mut Self::SortBuf,
        mut map_diffs: impl FnMut(VectorDiff<T>) -> SmallVec<[VectorDiff<T>; 2]>,
    ) -> Option<Self> {
        assert!(buffer.is_empty(), "buffer must be empty when calling `push_into_sort_buf`");

        let mut diffs = map_diffs(self);

        match diffs.len() {
            0 => None,
            1 => diffs.pop(),
            _ => {
                // We want the first element. We can't “pop front” on a `SmallVec`.
                // The idea is to reverse the `diffs` and to pop from it.
                diffs.reverse();
                *buffer = diffs;

                buffer.pop()
            }
        }
    }

    fn pop_from_sort_buf(buffer: &mut Self::SortBuf) -> Option<Self> {
        buffer.pop()
    }
}

impl<T> VectorDiffContainerOps<T> for Vec<VectorDiff<T>> {
    type Family = VecVectorDiffFamily;
    type LimitBuf = ();
    type RLimitBuf = ();
    type SortBuf = ();

    fn from_item(vector_diff: VectorDiff<T>) -> Self {
        vec![vector_diff]
    }

    fn filter_map<U>(
        self,
        f: impl FnMut(VectorDiff<T>) -> Option<VectorDiff<U>>,
    ) -> Option<VectorDiffContainerFamilyMember<Self::Family, U>> {
        let res: Vec<_> = self.into_iter().filter_map(f).collect();

        if res.is_empty() {
            None
        } else {
            Some(res)
        }
    }

    fn push_into_limit_buf(
        self,
        _buffer: &mut Self::LimitBuf,
        map_diffs: impl FnMut(VectorDiff<T>) -> ArrayVec<VectorDiff<T>, 2>,
    ) -> Option<Self> {
        let res: Vec<_> = self.into_iter().flat_map(map_diffs).collect();

        if res.is_empty() {
            None
        } else {
            Some(res)
        }
    }

    fn pop_from_limit_buf(_: &mut Self::LimitBuf) -> Option<Self> {
        None
    }

    fn push_into_rlimit_buf(
        self,
        _buffer: &mut Self::RLimitBuf,
        map_diffs: impl FnMut(VectorDiff<T>) -> SmallVec<[VectorDiff<T>; 2]>,
    ) -> Option<Self> {
        let res: Vec<_> = self.into_iter().flat_map(map_diffs).collect();

        if res.is_empty() {
            None
        } else {
            Some(res)
        }
    }

    fn extend_rlimit_buf(diffs: Vec<VectorDiff<T>>, _buffer: &mut Self::RLimitBuf) -> Option<Self> {
        if diffs.is_empty() {
            None
        } else {
            Some(diffs)
        }
    }

    fn pop_from_rlimit_buf(_buffer: &mut Self::RLimitBuf) -> Option<Self> {
        None
    }

    fn push_into_sort_buf(
        self,
        _buffer: &mut (),
        map_diffs: impl FnMut(VectorDiff<T>) -> SmallVec<[VectorDiff<T>; 2]>,
    ) -> Option<Self> {
        let res: Vec<_> = self.into_iter().flat_map(map_diffs).collect();

        if res.is_empty() {
            None
        } else {
            Some(res)
        }
    }

    fn pop_from_sort_buf(_: &mut Self::LimitBuf) -> Option<Self> {
        None
    }
}

#[allow(unreachable_pub)]
pub trait VectorDiffContainerFamily {
    type Member<T>: VectorDiffContainerOps<T, Family = Self>;
}

#[derive(Debug)]
pub enum VectorDiffFamily {}

impl VectorDiffContainerFamily for VectorDiffFamily {
    type Member<T> = VectorDiff<T>;
}

#[derive(Debug)]
pub enum VecVectorDiffFamily {}

impl VectorDiffContainerFamily for VecVectorDiffFamily {
    type Member<T> = Vec<VectorDiff<T>>;
}
