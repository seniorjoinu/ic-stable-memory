use crate::collections::btree_map::iter::SBTreeMapIter;
use crate::collections::btree_map::BTreeNode;
use crate::collections::btree_set::SBTreeSet;
use crate::primitive::StableAllocated;

pub struct SBTreeSetIter<'a, T> {
    iter: SBTreeMapIter<'a, T, ()>,
}

impl<'a, T> SBTreeSetIter<'a, T> {
    pub fn new(set: &'a SBTreeSet<T>) -> Self {
        Self {
            iter: SBTreeMapIter::new(&set.map),
        }
    }
}

impl<'a, T: StableAllocated> Iterator for SBTreeSetIter<'a, T>
where
    [(); BTreeNode::<T, ()>::SIZE]: Sized,
    [(); T::SIZE]: Sized,
    BTreeNode<T, ()>: StableAllocated,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|it| it.0)
    }
}
