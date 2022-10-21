use crate::collections::vec::iter::SVecIter;
use copy_as_bytes::traits::{AsBytes, SuperSized};
use crate::collections::binary_heap::SBinaryHeap;

pub struct SBinaryHeapIter<'a, T> {
    iter: SVecIter<'a, T>,
}

impl<'a, T: SuperSized> SBinaryHeapIter<'a, T> {
    pub fn new(heap: &SBinaryHeap<T>) -> Self {
        Self {
            iter: SVecIter::new(&heap.arr),
        }
    }
}

impl<'a, T: AsBytes> Iterator for SBinaryHeapIter<'a, T>
where
    [(); T::SIZE]: Sized,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}
