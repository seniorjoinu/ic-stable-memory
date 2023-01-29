use crate::collections::vec::SVec;
use crate::primitive::s_ref::SRef;
use crate::primitive::s_ref_mut::SRefMut;
use crate::SSlice;
use crate::utils::encoding::{AsFixedSizeBytes, FixedSize};

pub struct SVecIter<'a, T> {
    svec: &'a SVec<T>,
    offset: usize,
    max_offset: usize,
}

impl<'a, T: FixedSize> SVecIter<'a, T> {
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

impl<'a, T: AsFixedSizeBytes> Iterator for SVecIter<'a, T>
where
    [(); T::SIZE]: Sized,
{
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

pub struct SVecIterMut<'a, T> {
    svec: &'a mut SVec<T>,
    offset: usize,
    max_offset: usize,
}

impl<'a, T: FixedSize> SVecIterMut<'a, T> {
    pub(crate) fn new(svec: &'a mut SVec<T>) -> Self {
        let offset = 0;
        let max_offset = svec.len() * T::SIZE;

        Self {
            svec,
            offset,
            max_offset,
        }
    }
}

impl<'a, T: AsFixedSizeBytes> Iterator for SVecIterMut<'a, T>
    where
        [(); T::SIZE]: Sized,
{
    type Item = SRefMut<'a, T>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset == self.max_offset {
            return None;
        }

        let ptr = self.svec.ptr + self.offset as u64;
        self.offset += T::SIZE;

        Some(SRefMut::new(ptr))
    }
}

pub struct SVecIterCopy<'a, T> {
    svec: &'a SVec<T>,
    offset: usize,
    max_offset: usize,
}

impl<'a, T: FixedSize> SVecIterCopy<'a, T> {
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

impl<'a, T: AsFixedSizeBytes> Iterator for SVecIterCopy<'a, T>
    where
        [(); T::SIZE]: Sized,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset == self.max_offset {
            return None;
        }

        let it = Some(SSlice::_as_fixed_size_bytes_read(self.svec.ptr, self.offset));

        self.offset += T::SIZE;

        it
    }
}