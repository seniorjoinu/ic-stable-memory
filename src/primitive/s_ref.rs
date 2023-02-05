use crate::encoding::AsFixedSizeBytes;
use crate::primitive::StableType;
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

impl<'o, T: StableType + AsFixedSizeBytes> SRef<'o, T> {
    unsafe fn read(&self) {
        if self.inner.is_none() {
            let it = crate::mem::read_fixed_for_reference(self.ptr);
            *(&self.inner as *const Option<T> as *mut Option<T>) = Some(it);
        }
    }
}

impl<'o, T: StableType + AsFixedSizeBytes> Deref for SRef<'o, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { self.read() };

        self.inner.as_ref().unwrap()
    }
}
