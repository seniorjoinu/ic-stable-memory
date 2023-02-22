use crate::collections::hash_map::iter::SHashMapIter;
use crate::collections::hash_set::SHashSet;
use crate::encoding::AsFixedSizeBytes;
use crate::primitive::s_ref::SRef;
use crate::primitive::StableType;
use std::hash::Hash;

pub struct SHashSetIter<'a, T: StableType + AsFixedSizeBytes + Hash + Eq> {
    iter: SHashMapIter<'a, T, ()>,
}

impl<'a, T: StableType + AsFixedSizeBytes + Hash + Eq> SHashSetIter<'a, T> {
    pub fn new(set: &'a SHashSet<T>) -> Self {
        Self {
            iter: SHashMapIter::new(&set.map),
        }
    }
}

impl<'a, T: StableType + AsFixedSizeBytes + Eq + Hash> Iterator for SHashSetIter<'a, T> {
    type Item = SRef<'a, T>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|it| it.0)
    }
}
