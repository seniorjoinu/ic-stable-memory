use crate::utils::encoding::AsFixedSizeBytes;
use crate::SSlice;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

pub struct SRefMut<'o, T> {
    ptr: u64,
    inner: Option<T>,
    _marker: PhantomData<&'o mut T>,
}

impl<'o, T> SRefMut<'o, T> {
    pub(crate) fn new(ptr: u64) -> Self {
        Self {
            ptr,
            inner: None,
            _marker: PhantomData::default(),
        }
    }
}

impl<'o, T: AsFixedSizeBytes> SRefMut<'o, T>
where
    [(); T::SIZE]: Sized,
{
    pub fn read(&mut self) -> SRefMutRead<'o, '_, T> {
        if self.inner.is_none() {
            let it = SSlice::_as_fixed_size_bytes_read(self.ptr, 0);
            self.inner = Some(it);
        }

        SRefMutRead(self)
    }

    fn repersist(&mut self) {
        if let Some(it) = &self.inner {
            SSlice::_write_bytes(self.ptr, 0, &it.as_fixed_size_bytes());
        }
    }
}

pub struct SRefMutRead<'o, 'a, T: AsFixedSizeBytes>(&'a mut SRefMut<'o, T>)
where
    [(); T::SIZE]: Sized;

impl<'o, 'a, T: AsFixedSizeBytes> Deref for SRefMutRead<'o, 'a, T>
where
    [(); T::SIZE]: Sized,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.0.inner.as_ref().unwrap_unchecked() }
    }
}

impl<'o, 'a, T: AsFixedSizeBytes> DerefMut for SRefMutRead<'o, 'a, T>
where
    [(); T::SIZE]: Sized,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.0.inner.as_mut().unwrap_unchecked() }
    }
}

impl<'o, 'a, T: AsFixedSizeBytes> Drop for SRefMutRead<'o, 'a, T>
where
    [(); T::SIZE]: Sized,
{
    fn drop(&mut self) {
        self.0.repersist()
    }
}
