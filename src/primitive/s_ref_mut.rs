use crate::encoding::AsFixedSizeBytes;
use crate::primitive::StableType;
use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

pub struct SRefMut<'o, T: StableType + AsFixedSizeBytes> {
    ptr: u64,
    inner: UnsafeCell<Option<T>>,
    _marker: PhantomData<&'o mut T>,
}

impl<'o, T: StableType + AsFixedSizeBytes> SRefMut<'o, T> {
    #[inline]
    pub(crate) fn new(ptr: u64) -> Self {
        Self {
            ptr,
            inner: UnsafeCell::new(None),
            _marker: PhantomData::default(),
        }
    }

    #[inline]
    unsafe fn read(&self) {
        if (*self.inner.get()).is_none() {
            let it = crate::mem::read_fixed_for_move(self.ptr);
            *self.inner.get() = Some(it);
        }
    }

    #[inline]
    unsafe fn repersist(&mut self) {
        if let Some(it) = self.inner.get_mut() {
            crate::mem::write_fixed(self.ptr, it);
        }
    }
}

impl<'o, T: StableType + AsFixedSizeBytes> Deref for SRefMut<'o, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { self.read() };

        unsafe { (*self.inner.get()).as_ref().unwrap() }
    }
}

impl<'o, T: StableType + AsFixedSizeBytes> DerefMut for SRefMut<'o, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.read() };

        self.inner.get_mut().as_mut().unwrap()
    }
}

impl<'o, T: StableType + AsFixedSizeBytes> Drop for SRefMut<'o, T> {
    #[inline]
    fn drop(&mut self) {
        unsafe { self.repersist() };
    }
}
