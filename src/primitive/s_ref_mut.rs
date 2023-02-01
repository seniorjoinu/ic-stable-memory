use crate::encoding::{AsFixedSizeBytes, Buffer};
use crate::SSlice;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

pub struct SRefMut<'o, T: AsFixedSizeBytes> {
    ptr: u64,
    inner: Option<T>,
    _marker: PhantomData<&'o mut T>,
}

impl<'o, T: AsFixedSizeBytes> SRefMut<'o, T> {
    #[inline]
    pub(crate) fn new(ptr: u64) -> Self {
        Self {
            ptr,
            inner: None,
            _marker: PhantomData::default(),
        }
    }

    #[inline]
    fn read(&self) {
        if self.inner.is_none() {
            let it = SSlice::_as_fixed_size_bytes_read(self.ptr, 0);
            unsafe { *(&self.inner as *const Option<T> as *mut Option<T>) = Some(it) };
        }
    }

    #[inline]
    fn repersist(&mut self) {
        if let Some(it) = &self.inner {
            SSlice::_write_bytes(self.ptr, 0, it.as_new_fixed_size_bytes()._deref());
        }
    }
}

impl<'o, T: AsFixedSizeBytes> Deref for SRefMut<'o, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.read();

        self.inner.as_ref().unwrap()
    }
}

impl<'o, T: AsFixedSizeBytes> DerefMut for SRefMut<'o, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.read();

        self.inner.as_mut().unwrap()
    }
}

impl<'o, T: AsFixedSizeBytes> Drop for SRefMut<'o, T> {
    #[inline]
    fn drop(&mut self) {
        self.repersist();
    }
}
