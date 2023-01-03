use crate::collections::hash_map::iter::SHashMapIter;
use crate::collections::hash_set::SHashSet;
use crate::primitive::StableAllocated;
use std::hash::Hash;

pub struct SHashSetIter<'a, T> {
    iter: SHashMapIter<'a, T, ()>,
}

impl<'a, T> SHashSetIter<'a, T> {
    pub fn new(set: &'a SHashSet<T>) -> Self {
        Self {
            iter: SHashMapIter::new(&set.map),
        }
    }
}

impl<'a, T: StableAllocated + Eq + Hash> Iterator for SHashSetIter<'a, T>
where
    [(); T::SIZE]: Sized,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|it| it.0)
    }
}
