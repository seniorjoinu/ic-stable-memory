use crate::encoding::AsFixedSizeBytes;
use crate::primitive::StableType;
use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::ops::Deref;

pub struct SRef<'o, T> {
    ptr: u64,
    inner: UnsafeCell<Option<T>>,
    _marker: PhantomData<&'o T>,
}

impl<'o, T> SRef<'o, T> {
    #[inline]
    pub(crate) fn new(ptr: u64) -> Self {
        Self {
            ptr,
            inner: UnsafeCell::new(None),
            _marker: PhantomData::default(),
        }
    }
}

impl<'o, T: StableType + AsFixedSizeBytes> SRef<'o, T> {
    unsafe fn read(&self) {
        if (*self.inner.get()).is_none() {
            let it = crate::mem::read_fixed_for_reference(self.ptr);
            *self.inner.get() = Some(it);
        }
    }
}

impl<'o, T: StableType + AsFixedSizeBytes> Deref for SRef<'o, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { self.read() };

        unsafe { (*self.inner.get()).as_ref().unwrap() }
    }
}
