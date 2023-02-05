use crate::collections::btree_map::SBTreeMap;
use crate::collections::btree_set::iter::SBTreeSetIter;
use crate::encoding::AsFixedSizeBytes;
use crate::primitive::StableType;
use std::borrow::Borrow;

pub mod iter;

pub struct SBTreeSet<T: StableType + AsFixedSizeBytes + Ord> {
    map: SBTreeMap<T, ()>,
}

impl<T: Ord + StableType + AsFixedSizeBytes> SBTreeSet<T> {
    #[inline]
    pub fn new() -> Self {
        Self {
            map: SBTreeMap::new(),
        }
    }

    #[inline]
    pub fn len(&self) -> u64 {
        self.map.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    #[inline]
    pub fn insert(&mut self, value: T) -> bool {
        self.map.insert(value, ()).is_some()
    }

    #[inline]
    pub fn remove<Q>(&mut self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: Ord,
    {
        self.map.remove(value).is_some()
    }

    #[inline]
    pub fn contains<Q>(&self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: Ord,
    {
        self.map.contains_key(value)
    }

    #[inline]
    pub fn iter(&self) -> SBTreeSetIter<T> {
        SBTreeSetIter::new(self)
    }
}

impl<T: StableType + AsFixedSizeBytes + Ord> SBTreeSet<T> {
    #[inline]
    pub fn clear(&mut self) {
        self.map.clear();
    }
}

impl<T: Ord + StableType + AsFixedSizeBytes> Default for SBTreeSet<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: StableType + AsFixedSizeBytes + Ord> AsFixedSizeBytes for SBTreeSet<T> {
    const SIZE: usize = SBTreeMap::<T, ()>::SIZE;
    type Buf = <SBTreeMap<T, ()> as AsFixedSizeBytes>::Buf;

    #[inline]
    fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
        self.map.as_fixed_size_bytes(buf);
    }

    #[inline]
    fn from_fixed_size_bytes(arr: &[u8]) -> Self {
        let map = SBTreeMap::<T, ()>::from_fixed_size_bytes(&arr);
        Self { map }
    }
}

impl<T: StableType + AsFixedSizeBytes + Ord> StableType for SBTreeSet<T> {
    #[inline]
    unsafe fn stable_memory_disown(&mut self) {
        self.map.stable_memory_disown();
    }

    #[inline]
    unsafe fn stable_memory_own(&mut self) {
        self.map.stable_memory_own()
    }

    #[inline]
    fn is_owned_by_stable_memory(&self) -> bool {
        self.map.is_owned_by_stable_memory()
    }

    #[inline]
    unsafe fn stable_drop(&mut self) {}
}

#[cfg(test)]
mod tests {
    use crate::collections::btree_set::SBTreeSet;
    use crate::encoding::{AsFixedSizeBytes, Buffer};
    use crate::primitive::StableType;
    use crate::{init_allocator, stable};

    #[test]
    fn it_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut set = SBTreeSet::default();
        set.insert(10);
        set.insert(20);

        assert!(set.contains(&10));
        assert_eq!(set.len(), 2);
        assert!(!set.is_empty());

        assert!(set.remove(&10));
        assert!(!set.remove(&10));

        let set = SBTreeSet::<u64>::new();
    }

    #[test]
    fn serialization_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let set = SBTreeSet::<u32>::new();

        let buf = set.as_new_fixed_size_bytes();
        SBTreeSet::<u32>::from_fixed_size_bytes(buf._deref());
    }

    #[test]
    fn iter_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut set = SBTreeSet::<u32>::default();
        for i in 0..100 {
            set.insert(i);
        }

        for (idx, mut i) in set.iter().enumerate() {
            assert_eq!(idx as u32, *i);
        }
    }

    #[test]
    fn helpers_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut set = SBTreeSet::<u32>::default();
    }
}
