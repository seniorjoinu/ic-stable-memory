use crate::encoding::AsFixedSizeBytes;
use crate::SSlice;
use std::marker::PhantomData;
use std::ops::Deref;

pub struct SRef<'o, T> {
    ptr: u64,
    inner: Option<T>,
    _marker: PhantomData<&'o T>,
}

impl<'o, T> SRef<'o, T> {
    #[inline]
    pub(crate) fn new(ptr: u64) -> Self {
        Self {
            ptr,
            inner: None,
            _marker: PhantomData::default(),
        }
    }
}

impl<'o, T: AsFixedSizeBytes> SRef<'o, T> {
    fn read(&self) {
        if self.inner.is_none() {
            let it = SSlice::_as_fixed_size_bytes_read(self.ptr, 0);
            unsafe { *(&self.inner as *const Option<T> as *mut Option<T>) = Some(it) };
        }
    }
}

impl<'o, T: AsFixedSizeBytes> Deref for SRef<'o, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.read();

        self.inner.as_ref().unwrap()
    }
}
