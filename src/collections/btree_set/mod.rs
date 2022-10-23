use crate::collections::btree_map::{BTreeNode, SBTreeMap};
use crate::collections::btree_set::iter::SBTreeSetIter;
use crate::collections::vec::SVec;
use crate::primitive::StableAllocated;
use copy_as_bytes::traits::{AsBytes, SuperSized};
use speedy::{Context, LittleEndian, Readable, Reader, Writable, Writer};

pub mod iter;

pub struct SBTreeSet<T> {
    map: SBTreeMap<T, ()>,
}

impl<T> SBTreeSet<T> {
    pub fn len(&self) -> u64 {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

impl<T: Ord + StableAllocated> SBTreeSet<T>
where
    [(); BTreeNode::<T, ()>::SIZE]: Sized, // ???? why only putting K is enough
    [(); T::SIZE]: Sized,
    [(); SVec::<BTreeNode<T, ()>>::SIZE]: Sized,
    BTreeNode<T, ()>: StableAllocated,
{
    pub fn new() -> Self {
        Self {
            map: SBTreeMap::new(),
        }
    }

    pub fn insert(&mut self, value: T) -> bool {
        self.map.insert(value, ()).is_some()
    }

    pub fn remove(&mut self, value: &T) -> bool {
        self.map.remove(value).is_some()
    }

    pub fn contains(&self, value: &T) -> bool {
        self.map.contains_key(value)
    }

    pub fn iter(&self) -> SBTreeSetIter<T> {
        SBTreeSetIter::new(self)
    }
}

impl<T: Ord + StableAllocated> Default for SBTreeSet<T>
where
    [(); BTreeNode::<T, ()>::SIZE]: Sized, // ???? why only putting K is enough
    [(); T::SIZE]: Sized,
    [(); SVec::<BTreeNode<T, ()>>::SIZE]: Sized,
    BTreeNode<T, ()>: StableAllocated,
{
    fn default() -> Self {
        SBTreeSet::new()
    }
}

impl<T> SuperSized for SBTreeSet<T> {
    const SIZE: usize = SBTreeMap::<T, ()>::SIZE;
}

impl<T: StableAllocated> AsBytes for SBTreeSet<T>
where
    [(); BTreeNode::<T, ()>::SIZE]: Sized,
    [(); T::SIZE]: Sized,
    [(); SVec::<BTreeNode<T, ()>>::SIZE]: Sized,
    BTreeNode<T, ()>: StableAllocated,
    [(); Self::SIZE]: Sized,
    [(); SBTreeMap::<T, ()>::SIZE]: Sized,
{
    #[inline]
    fn from_bytes(arr: [u8; Self::SIZE]) -> Self {
        let mut buf = [0u8; SBTreeMap::<T, ()>::SIZE];
        buf.copy_from_slice(&arr);

        let map = SBTreeMap::<T, ()>::from_bytes(buf);
        Self { map }
    }

    #[inline]
    fn to_bytes(self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        let map_buf = self.map.to_bytes();

        buf.copy_from_slice(&map_buf);

        buf
    }
}

impl<T: StableAllocated + Ord> StableAllocated for SBTreeSet<T>
where
    [(); BTreeNode::<T, ()>::SIZE]: Sized, // ???? why only putting K is enough
    [(); T::SIZE]: Sized,
    [(); SVec::<BTreeNode<T, ()>>::SIZE]: Sized,
    BTreeNode<T, ()>: StableAllocated,
    [(); Self::SIZE]: Sized,
    [(); SBTreeMap::<T, ()>::SIZE]: Sized,
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
        self.map.stable_drop();
    }
}

impl<'a, T> Readable<'a, LittleEndian> for SBTreeSet<T> {
    fn read_from<R: Reader<'a, LittleEndian>>(
        reader: &mut R,
    ) -> Result<Self, <speedy::LittleEndian as Context>::Error> {
        let map = SBTreeMap::read_from(reader)?;

        Ok(Self { map })
    }
}

impl<T> Writable<LittleEndian> for SBTreeSet<T> {
    fn write_to<W: ?Sized + Writer<LittleEndian>>(
        &self,
        writer: &mut W,
    ) -> Result<(), <speedy::LittleEndian as Context>::Error> {
        self.map.write_to(writer)
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::btree_set::SBTreeSet;
    use crate::primitive::StableAllocated;
    use crate::{init_allocator, stable};
    use copy_as_bytes::traits::AsBytes;
    use speedy::{Readable, Writable};

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

        unsafe { set.stable_drop() };

        let set = SBTreeSet::<u64>::new();
        unsafe { set.stable_drop() };
    }

    #[test]
    fn serialization_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let set = SBTreeSet::<u32>::new();
        let buf = set.write_to_vec().unwrap();
        SBTreeSet::<u32>::read_from_buffer_copying_data(&buf).unwrap();

        let buf = set.to_bytes();
        SBTreeSet::<u32>::from_bytes(buf);
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

        for (idx, i) in set.iter().enumerate() {
            assert_eq!(idx as u32, i);
        }
    }
}
