use crate::collections::vec::SVec;
use crate::utils::encoding::{AsFixedSizeBytes, FixedSize};
use crate::SSlice;

pub struct SVecIter<'a, T> {
    svec: &'a SVec<T>,
    offset: usize,
    max_offset: usize,
}

impl<'a, T: FixedSize> SVecIter<'a, T> {
    pub fn new(svec: &'a SVec<T>) -> Self {
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
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset == self.max_offset {
            return None;
        }

        let mut item_bytes = T::super_size_u8_arr();
        SSlice::_read_bytes(self.svec.ptr, self.offset, &mut item_bytes);

        self.offset += T::SIZE;

        Some(T::from_bytes(item_bytes))
    }
}
