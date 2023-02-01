use crate::collections::binary_heap::SBinaryHeap;
use crate::collections::vec::iter::SVecIter;
use crate::encoding::{AsFixedSizeBytes, Buffer};
use crate::primitive::s_ref::SRef;

pub struct SBinaryHeapIter<'a, T> {
    iter: SVecIter<'a, T>,
}

impl<'a, T: AsFixedSizeBytes> SBinaryHeapIter<'a, T> {
    pub fn new(heap: &'a SBinaryHeap<T>) -> Self {
        Self {
            iter: SVecIter::new(&heap.inner),
        }
    }
}

impl<'a, T: AsFixedSizeBytes> Iterator for SBinaryHeapIter<'a, T> {
    type Item = SRef<'a, T>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}
