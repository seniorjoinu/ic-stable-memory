use crate::collections::vec::SVec;
use crate::encoding::AsFixedSizeBytes;
use crate::primitive::s_ref::SRef;
use crate::primitive::s_ref_mut::SRefMut;
use crate::primitive::StableType;

pub struct SVecIter<'a, T: StableType + AsFixedSizeBytes> {
    svec: &'a SVec<T>,
    offset: usize,
    max_offset: usize,
}

impl<'a, T: AsFixedSizeBytes + StableType> SVecIter<'a, T> {
    pub(crate) fn new(svec: &'a SVec<T>) -> Self {
        let offset = 0;
        let max_offset = svec.len() * T::SIZE;

        Self {
            svec,
            offset,
            max_offset,
        }
    }
}

impl<'a, T: StableType + AsFixedSizeBytes> Iterator for SVecIter<'a, T> {
    type Item = SRef<'a, T>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset == self.max_offset {
            return None;
        }

        let ptr = self.svec.ptr + self.offset as u64;
        self.offset += T::SIZE;

        Some(SRef::new(ptr))
    }
}
