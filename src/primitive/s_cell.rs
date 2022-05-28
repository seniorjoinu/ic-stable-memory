use crate::mem::allocator::{NotFree, NotStableMemoryAllocator};
use crate::utils::encode::AsBytes;
use crate::{allocate, deallocate, OutOfMemory, SSlice};
use candid::types::{Serializer, Type};
use candid::CandidType;
use serde::{Deserialize, Deserializer};
use std::fmt::{Debug, Formatter};
use std::mem::size_of;

pub struct SCell<T>(SSlice<T>);

impl<T: NotFree + NotStableMemoryAllocator> Debug for SCell<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<T> Clone for SCell<T> {
    fn clone(&self) -> Self {
        unsafe { Self(self.0.clone()) }
    }
}

impl<T> CandidType for SCell<T> {
    fn _ty() -> Type {
        Type::Nat64
    }

    fn idl_serialize<S>(&self, serializer: S) -> Result<(), S::Error>
    where
        S: Serializer,
    {
        self.0.idl_serialize(serializer)
    }
}

impl<'de, T> Deserialize<'de> for SCell<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(SCell(SSlice::<T>::deserialize(deserializer)?))
    }
}

impl<T: Sized + AsBytes> SCell<T> {
    pub fn new(value: &T) -> Result<Self, OutOfMemory> {
        let value_bytes = unsafe { value.as_bytes() };
        let raw = allocate(value_bytes.len())?;

        raw._write_bytes(0, &value_bytes);

        Ok(Self(raw))
    }

    pub fn get(&self) -> T {
        let mut value_bytes = vec![0u8; size_of::<T>()];
        self.0._read_bytes(0, &mut value_bytes);

        unsafe { T::from_bytes(&value_bytes) }
    }

    pub fn set(&self, value: &T) {
        let value_bytes = unsafe { value.as_bytes() };
        self.0._write_bytes(0, &value_bytes);
    }

    pub fn as_ptr(&self) -> u64 {
        self.0.ptr
    }

    pub fn drop(self) {
        deallocate(self.0)
    }
}

impl<T> AsBytes for SCell<T> {
    unsafe fn as_bytes(&self) -> Vec<u8> {
        self.0.as_bytes()
    }

    unsafe fn from_bytes(bytes: &[u8]) -> Self {
        Self(SSlice::from_bytes(bytes))
    }
}
