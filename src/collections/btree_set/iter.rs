use crate::collections::btree_map::iter::SBTreeMapIter;
use crate::collections::btree_set::SBTreeSet;
use crate::primitive::s_ref::SRef;
use crate::primitive::StableAllocated;

pub struct SBTreeSetIter<'a, T> {
    iter: SBTreeMapIter<'a, T, ()>,
}

impl<'a, T> SBTreeSetIter<'a, T> {
    pub fn new(set: &'a SBTreeSet<T>) -> Self {
        Self {
            iter: SBTreeMapIter::new(&set.map.root, set.map.len),
        }
    }
}

impl<'a, T: StableAllocated + Ord> Iterator for SBTreeSetIter<'a, T> {
    type Item = SRef<'a, T>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|it| it.0)
    }
}

impl<'a, T: StableAllocated + Ord> ExactSizeIterator for SBTreeSetIter<'a, T> {
    fn len(&self) -> usize {
        self.iter.len()
    }
}

impl<'a, T: StableAllocated + Ord> DoubleEndedIterator for SBTreeSetIter<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter.next_back().map(|it| it.0)
    }
}
