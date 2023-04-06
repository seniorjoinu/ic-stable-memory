use crate::encoding::AsFixedSizeBytes;
use crate::primitive::StableType;
use candid::types::{Serializer, Type, TypeId};
use candid::CandidType;
use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::ops::Deref;

/// Immutable reference to data stored in stable memory
///
/// See also [SRefMut](crate::primitive::s_ref_mut::SRefMut).
///
/// Lazy - only loads and deserializes the data, when it gets accessed. Useful when building your
/// own stable data structure. Immutable access is provided by dereferencing.
///
/// `T` has to implement [StableType] and [AsFixedSizeBytes].
pub struct SRef<'o, T> {
    ptr: u64,
    inner: UnsafeCell<Option<T>>,
    _marker: PhantomData<&'o T>,
}

impl<'o, T> SRef<'o, T> {
    /// Creates reference from raw pointer.
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
        unsafe {
            self.read();

            (*self.inner.get()).as_ref().unwrap()
        }
    }
}

impl<'o, T: StableType + AsFixedSizeBytes + CandidType> CandidType for SRef<'o, T> {
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
