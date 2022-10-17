use crate::collections::btree_map::{BTreeNode, SBTreeMap};
use crate::primitive::StackAllocated;
use speedy::{Context, LittleEndian, Readable, Reader, Writable, Writer};
use std::mem::size_of;

pub struct SBTreeSet<T, A> {
    map: SBTreeMap<T, (), A, [u8; 0]>,
}

impl<A, T> SBTreeSet<T, A> {
    pub fn new() -> Self {
        Self {
            map: SBTreeMap::new(),
        }
    }

    pub fn new_with_degree(degree: usize) -> Self {
        Self {
            map: SBTreeMap::new_with_degree(degree),
        }
    }

    pub fn len(&self) -> u64 {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

impl<A: AsMut<[u8]> + AsRef<[u8]>, T: Ord + StackAllocated<T, A>> SBTreeSet<T, A>
where
    [(); size_of::<BTreeNode<T, (), A, [u8; 0]>>()]: Sized,
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

    pub unsafe fn drop(self) {
        self.map.drop()
    }
}

impl<A, T> Default for SBTreeSet<T, A> {
    fn default() -> Self {
        SBTreeSet::new()
    }
}

impl<'a, T, A> Readable<'a, LittleEndian> for SBTreeSet<T, A> {
    fn read_from<R: Reader<'a, LittleEndian>>(
        reader: &mut R,
    ) -> Result<Self, <speedy::LittleEndian as Context>::Error> {
        let map = SBTreeMap::read_from(reader)?;

        Ok(Self { map })
    }
}

impl<T, A> Writable<LittleEndian> for SBTreeSet<T, A>
where
    [(); size_of::<BTreeNode<T, (), A, [u8; 0]>>()]: Sized,
{
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
    use crate::{init_allocator, stable};
    use std::mem::size_of;

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

        unsafe { set.drop() };

        let set = SBTreeSet::<u64, [u8; size_of::<u64>()]>::new_with_degree(3);
        unsafe { set.drop() };
    }
}
