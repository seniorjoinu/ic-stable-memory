use crate::encoding::AsFixedSizeBytes;
use crate::primitive::StableType;
use candid::types::{Serializer, Type};
use candid::CandidType;
use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

/// Mutable reference to data stored in stable memory
///
/// See also [SRef](crate::primitive::s_ref::SRef).
///
/// Lazy on reads  - only loads and deserializes the data, when it gets accessed. Lazy on writes -
/// only performs actual underlying data updates when [Drop]-ped. Useful when building your
/// own stable data structure. Immutable and mutable access is provided by dereferencing.
///
/// `T` has to implement [StableType] and [AsFixedSizeBytes].
pub struct SRefMut<'o, T: StableType + AsFixedSizeBytes> {
    ptr: u64,
    inner: UnsafeCell<Option<T>>,
    _marker: PhantomData<&'o mut T>,
}

impl<'o, T: StableType + AsFixedSizeBytes> SRefMut<'o, T> {
    /// Creates mutable reference from raw pointer.
    ///
    /// # Safety
    /// Make sure your raw pointer points to a valid location.
    #[inline]
    pub unsafe fn new(ptr: u64) -> Self {
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
        unsafe {
            self.read();

            (*self.inner.get()).as_ref().unwrap()
        }
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

impl<'o, T: StableType + AsFixedSizeBytes + CandidType> CandidType for SRefMut<'o, T> {
    #[inline]
    fn _ty() -> Type {
        T::_ty()
    }

    #[inline]
    fn idl_serialize<S>(&self, serializer: S) -> Result<(), S::Error>
    where
        S: Serializer,
    {
        unsafe {
            self.read();

            (*self.inner.get())
                .as_ref()
                .unwrap()
                .idl_serialize(serializer)
        }
    }
}
