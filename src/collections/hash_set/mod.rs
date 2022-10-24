use crate::collections::hash_map::SHashMap;
use crate::collections::hash_set::iter::SHashSetIter;
use crate::primitive::StableAllocated;
use copy_as_bytes::traits::{AsBytes, SuperSized};
use speedy::{Context, LittleEndian, Readable, Reader, Writable, Writer};
use std::hash::Hash;

pub mod iter;

pub struct SHashSet<T> {
    map: SHashMap<T, ()>,
}

impl<T> SHashSet<T> {
    pub fn new() -> Self {
        Self {
            map: SHashMap::new(),
        }
    }

    pub fn new_with_capacity(capacity: usize) -> Self {
        Self {
            map: SHashMap::new_with_capacity(capacity),
        }
    }
}

impl<T: StableAllocated + Hash + Eq> SHashSet<T>
where
    [u8; T::SIZE]: Sized,
{
    pub fn insert(&mut self, value: T) -> bool {
        self.map.insert(value, ()).is_some()
    }

    pub fn remove(&mut self, value: &T) -> bool {
        self.map.remove(value).is_some()
    }

    pub fn contains(&self, value: &T) -> bool {
        self.map.contains_key(value)
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub unsafe fn stable_drop_collection(&mut self) {
        self.map.stable_drop_collection()
    }

    pub fn iter(&self) -> SHashSetIter<T> {
        SHashSetIter::new(self)
    }
}

impl<T> Default for SHashSet<T> {
    fn default() -> Self {
        SHashSet::new()
    }
}

impl<'a, T> Readable<'a, LittleEndian> for SHashSet<T> {
    fn read_from<R: Reader<'a, LittleEndian>>(
        reader: &mut R,
    ) -> Result<Self, <speedy::LittleEndian as Context>::Error> {
        let map = SHashMap::read_from(reader)?;

        Ok(Self { map })
    }
}

impl<T> Writable<LittleEndian> for SHashSet<T> {
    fn write_to<W: ?Sized + Writer<LittleEndian>>(
        &self,
        writer: &mut W,
    ) -> Result<(), <speedy::LittleEndian as Context>::Error> {
        self.map.write_to(writer)
    }
}

impl<T> SuperSized for SHashSet<T> {
    const SIZE: usize = SHashMap::<T, ()>::SIZE;
}

impl<T> AsBytes for SHashSet<T> {
    fn to_bytes(self) -> [u8; Self::SIZE] {
        self.map.to_bytes()
    }

    fn from_bytes(arr: [u8; Self::SIZE]) -> Self {
        let map = SHashMap::<T, ()>::from_bytes(arr);
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
    use crate::{init_allocator, stable};
    use copy_as_bytes::traits::AsBytes;
    use speedy::{Readable, Writable};

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
        let buf = set.write_to_vec().unwrap();
        let set1 = SHashSet::<u32>::read_from_buffer_copying_data(&buf).unwrap();

        assert_eq!(set.map.len(), set1.map.len());
        assert_eq!(set.map.capacity, set1.map.capacity);
        assert!(set.map.table.is_none() && set1.map.table.is_none());

        let len = set.map.len;
        let cap = set.map.capacity;

        let buf = set.to_bytes();
        let set1 = SHashSet::<u32>::from_bytes(buf);

        assert_eq!(len, set1.map.len);
        assert_eq!(cap, set1.map.capacity);
        assert!(set1.map.table.is_none());
    }

    #[test]
    fn helpers_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut set = SHashSet::<u32>::default();

        set.move_to_stable();
        set.remove_from_stable();

        unsafe { set.stable_drop_collection() };
    }
}
