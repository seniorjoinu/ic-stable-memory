use crate::collections::hash_map::SHashMap;
use crate::collections::hash_set::iter::SHashSetIter;
use crate::primitive::StableAllocated;
use crate::utils::encoding::{AsDynSizeBytes, AsFixedSizeBytes, FixedSize};
use std::hash::Hash;

pub mod iter;

pub struct SHashSet<T> {
    map: SHashMap<T, ()>,
}

impl<T: StableAllocated + Hash + Eq> SHashSet<T>
where
    [(); T::SIZE]: Sized,
{
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
    pub fn insert(&mut self, value: T) -> bool {
        self.map.insert(value, ()).is_some()
    }

    #[inline]
    pub fn remove(&mut self, value: &T) -> bool {
        self.map.remove(value).is_some()
    }

    #[inline]
    pub fn contains(&self, value: &T) -> bool {
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

impl<T: StableAllocated + Hash + Eq> Default for SHashSet<T>
where
    [(); T::SIZE]: Sized,
{
    #[inline]
    fn default() -> Self {
        SHashSet::new()
    }
}

impl<T> FixedSize for SHashSet<T> {
    const SIZE: usize = SHashMap::<T, ()>::SIZE;
}

impl<T> AsFixedSizeBytes for SHashSet<T> {
    #[inline]
    fn as_fixed_size_bytes(&self) -> [u8; Self::SIZE] {
        self.map.as_fixed_size_bytes()
    }

    #[inline]
    fn from_fixed_size_bytes(arr: &[u8; Self::SIZE]) -> Self {
        let map = SHashMap::<T, ()>::from_fixed_size_bytes(arr);
        Self { map }
    }
}

impl<T: StableAllocated + Eq + Hash> StableAllocated for SHashSet<T>
where
    [(); T::SIZE]: Sized,
{
    #[inline]
    fn move_to_stable(&mut self) {
        self.map.move_to_stable();
    }

    #[inline]
    fn remove_from_stable(&mut self) {
        self.map.remove_from_stable()
    }

    #[inline]
    unsafe fn stable_drop(self) {
        self.map.stable_drop()
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::hash_set::SHashSet;
    use crate::primitive::StableAllocated;
    use crate::utils::encoding::AsFixedSizeBytes;
    use crate::{init_allocator, stable};

    #[test]
    fn basic_flow_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut set = SHashSet::default();

        assert!(set.is_empty());

        assert!(!set.insert(10));
        assert!(!set.insert(20));
        assert!(set.insert(10));

        assert!(set.contains(&10));
        assert!(!set.contains(&100));

        assert_eq!(set.len(), 2);

        assert!(!set.remove(&100));
        assert!(set.remove(&10));

        unsafe { set.stable_drop() };

        let set = SHashSet::<u64>::new_with_capacity(10);
        unsafe { set.stable_drop() };
    }

    #[test]
    fn iter_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut set = SHashSet::default();

        for i in 0..100 {
            set.insert(i);
        }

        let mut c = 0;
        for i in set.iter() {
            c += 1;
        }

        assert_eq!(c, 100);
    }

    #[test]
    fn serialization_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let set = SHashSet::<u32>::default();

        let len = set.len();
        let cap = set.capacity();

        let buf = set.as_fixed_size_bytes();
        let set1 = SHashSet::<u32>::from_fixed_size_bytes(&buf);

        assert_eq!(len, set1.len());
        assert_eq!(cap, set1.capacity());
    }

    #[test]
    fn helpers_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut set = SHashSet::<u32>::default();

        set.move_to_stable();
        set.remove_from_stable();

        unsafe { set.stable_drop() };
    }
}
