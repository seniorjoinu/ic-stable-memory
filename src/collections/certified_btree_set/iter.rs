use crate::collections::btree_map::iter::SBTreeMapIter;
use crate::collections::certified_btree_set::SCertifiedBTreeSet;
use crate::encoding::AsFixedSizeBytes;
use crate::primitive::s_ref::SRef;
use crate::primitive::StableType;
use crate::AsHashableBytes;

pub struct SCertifiedBTreeSetIter<'a, T> {
    iter: SBTreeMapIter<'a, T, ()>,
}

impl<'a, T: StableType + AsFixedSizeBytes + Ord + AsHashableBytes> SCertifiedBTreeSetIter<'a, T> {
    pub fn new(set: &'a SCertifiedBTreeSet<T>) -> Self {
        Self {
            iter: SBTreeMapIter::new(&set.map.inner),
        }
    }
}

impl<'a, T: StableType + AsFixedSizeBytes + Ord> Iterator for SCertifiedBTreeSetIter<'a, T> {
    type Item = SRef<'a, T>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|it| it.0)
    }
}

impl<'a, T: StableType + AsFixedSizeBytes + Ord> DoubleEndedIterator
    for SCertifiedBTreeSetIter<'a, T>
{
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter.next_back().map(|it| it.0)
    }
}
