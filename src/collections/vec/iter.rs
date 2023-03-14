use crate::collections::vec::SVec;
use crate::encoding::AsFixedSizeBytes;
use crate::primitive::s_ref::SRef;
use crate::primitive::StableType;
use crate::SSlice;

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

        let ptr = SSlice::_offset(self.svec.ptr, self.offset as u64);
        self.offset += T::SIZE;

        unsafe { Some(SRef::new(ptr)) }
    }
}

impl <A: StableType + AsFixedSizeBytes> FromIterator<A> for SVec<A>{
    fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self {
        let iter = iter.into_iter();
        let (lower, _) = iter.size_hint();
        let mut new_svec = SVec::new_with_capacity(lower).expect("Failed to allocate memory");
        for i in iter{
            let result = new_svec.push(i);
            match result{
                Ok(_) => continue,
                Err(_) => panic!("Failed to push element")
            }
        }
        new_svec
    }
}