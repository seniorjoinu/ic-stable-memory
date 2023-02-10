use crate::collections::hash_map::SHashMap;
use crate::collections::hash_set::iter::SHashSetIter;
use crate::encoding::AsFixedSizeBytes;
use crate::primitive::StableType;
use crate::OutOfMemory;
use std::borrow::Borrow;
use std::hash::Hash;

pub mod iter;

pub struct SHashSet<T: StableType + AsFixedSizeBytes + Hash + Eq> {
    map: SHashMap<T, ()>,
}

impl<T: StableType + AsFixedSizeBytes + Hash + Eq> SHashSet<T> {
    #[inline]
    pub fn new() -> Self {
        Self {
            map: SHashMap::new(),
        }
    }

    #[inline]
    pub fn new_with_capacity(capacity: usize) -> Self {
        Self {
            map: SHashMap::new_with_capacity(capacity),
        }
    }

    #[inline]
    pub fn insert(&mut self, value: T) -> Result<bool, OutOfMemory> {
        self.map.insert(value, ()).map(|it| it.is_some())
    }

    #[inline]
    pub fn remove<Q>(&mut self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.map.remove(value).is_some()
    }

    #[inline]
    pub fn contains<Q>(&self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.map.contains_key(value)
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.map.capacity()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        self.map.is_full()
    }

    #[inline]
    pub fn iter(&self) -> SHashSetIter<T> {
        SHashSetIter::new(self)
    }

    #[inline]
    pub fn clear(&mut self) {
        self.map.clear();
    }
}

impl<T: StableType + AsFixedSizeBytes + Hash + Eq> Default for SHashSet<T> {
    #[inline]
    fn default() -> Self {
        SHashSet::new()
    }
}

impl<T: StableType + AsFixedSizeBytes + Hash + Eq> AsFixedSizeBytes for SHashSet<T> {
    const SIZE: usize = SHashMap::<T, ()>::SIZE;
    type Buf = <SHashMap<T, ()> as AsFixedSizeBytes>::Buf;

    #[inline]
    fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
        self.map.as_fixed_size_bytes(buf)
    }

    #[inline]
    fn from_fixed_size_bytes(arr: &[u8]) -> Self {
        let map = SHashMap::<T, ()>::from_fixed_size_bytes(arr);
        Self { map }
    }
}

impl<T: StableType + AsFixedSizeBytes + Hash + Eq> StableType for SHashSet<T> {
    #[inline]
    unsafe fn assume_owned_by_stable_memory(&mut self) {
        self.map.assume_owned_by_stable_memory();
    }

    #[inline]
    unsafe fn assume_not_owned_by_stable_memory(&mut self) {
        self.map.assume_not_owned_by_stable_memory();
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
    use crate::collections::hash_set::SHashSet;
    use crate::encoding::{AsFixedSizeBytes, Buffer};
    use crate::{_debug_validate_allocator, get_allocated_size, stable, stable_memory_init};

    #[test]
    fn basic_flow_works_fine() {
        stable::clear();
        stable_memory_init();

        {
            let mut set = SHashSet::default();

            assert!(set.is_empty());

            assert!(!set.insert(10).unwrap());
            assert!(!set.insert(20).unwrap());
            assert!(set.insert(10).unwrap());

            assert!(set.contains(&10));
            assert!(!set.contains(&100));

            assert_eq!(set.len(), 2);

            assert!(!set.remove(&100));
            assert!(set.remove(&10));

            SHashSet::<u64>::new_with_capacity(10);
        }

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
    }

    #[test]
    fn iter_works_fine() {
        stable::clear();
        stable_memory_init();

        {
            let mut set = SHashSet::default();

            for i in 0..100 {
                set.insert(i);
            }

            let mut c = 0;
            for _ in set.iter() {
                c += 1;
            }

            assert_eq!(c, 100);
        }

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
    }

    #[test]
    fn serialization_works_fine() {
        stable::clear();
        stable_memory_init();

        {
            let set = SHashSet::<u32>::default();

            let len = set.len();
            let cap = set.capacity();

            let buf = set.as_new_fixed_size_bytes();
            let set1 = SHashSet::<u32>::from_fixed_size_bytes(buf._deref());

            assert_eq!(len, set1.len());
            assert_eq!(cap, set1.capacity());
        }
        
        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
    }
}
