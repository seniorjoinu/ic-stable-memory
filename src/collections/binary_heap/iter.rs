use crate::collections::binary_heap::SBinaryHeap;
use crate::collections::vec::iter::SVecIter;
use crate::encoding::AsFixedSizeBytes;
use crate::primitive::s_ref::SRef;
use crate::primitive::StableType;

pub struct SBinaryHeapIter<'a, T: StableType + AsFixedSizeBytes> {
    iter: SVecIter<'a, T>,
}

impl<'a, T: StableType + AsFixedSizeBytes> SBinaryHeapIter<'a, T> {
    pub fn new(heap: &'a SBinaryHeap<T>) -> Self {
        Self {
            iter: SVecIter::new(&heap.inner),
        }
    }
}

impl<'a, T: StableType + AsFixedSizeBytes> Iterator for SBinaryHeapIter<'a, T> {
    type Item = SRef<'a, T>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}
