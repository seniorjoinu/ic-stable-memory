use crate::utils::encoding::AsFixedSizeBytes;
use crate::SSlice;
use std::marker::PhantomData;
use std::ops::Deref;

pub struct SRef<'o, T> {
    ptr: u64,
    inner: Option<T>,
    _marker: PhantomData<&'o T>,
}

impl<'o, T> SRef<'o, T> {
    pub(crate) fn new(ptr: u64) -> Self {
        Self {
            ptr,
            inner: None,
            _marker: PhantomData::default(),
        }
    }
}

impl<'o, T: AsFixedSizeBytes> SRef<'o, T>
where
    [(); T::SIZE]: Sized,
{
    pub fn read(&mut self) -> SRefRead<'o, '_, T> {
        if self.inner.is_none() {
            let it = SSlice::_as_fixed_size_bytes_read(self.ptr, 0);
            self.inner = Some(it);
        }

        SRefRead(self)
    }
}

pub struct SRefRead<'o, 'a, T: AsFixedSizeBytes>(&'a SRef<'o, T>)
where
    [(); T::SIZE]: Sized;

impl<'o, 'a, T: AsFixedSizeBytes> Deref for SRefRead<'o, 'a, T>
where
    [(); T::SIZE]: Sized,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.0.inner.as_ref().unwrap_unchecked() }
    }
}
