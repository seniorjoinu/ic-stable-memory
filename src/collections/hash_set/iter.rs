use crate::collections::hash_map::iter::SHashMapIter;
use crate::collections::hash_set::SHashSet;
use copy_as_bytes::traits::{AsBytes, SuperSized};

pub struct SHashSetIter<'a, T> {
    iter: SHashMapIter<'a, T, ()>,
}

impl<'a, T: SuperSized> SHashSetIter<'a, T> {
    pub fn new(set: &SHashSet<T>) -> Self {
        Self {
            iter: SHashMapIter::new(&set.map),
        }
    }
}

impl<'a, T: AsBytes> Iterator for SHashSetIter<'a, T>
where
    [(); T::SIZE]: Sized,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|it| it.0)
    }
}
