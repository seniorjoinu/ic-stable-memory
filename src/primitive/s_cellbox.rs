use crate::mem::allocator::{NotFree, NotStableMemoryAllocator};
use crate::primitive::s_cell::SCell;
use crate::primitive::s_unsafe_cell::SUnsafeCell;
use crate::utils::encode::AsBytes;
use crate::OutOfMemory;
use candid::types::{Serializer, Type};
use candid::CandidType;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use std::fmt::{Debug, Formatter};

pub struct SCellBox<T>(SCell<SUnsafeCell<T>>);

impl<T: NotFree + NotStableMemoryAllocator> Debug for SCellBox<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<T> Clone for SCellBox<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> CandidType for SCellBox<T> {
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

impl<'de, T> Deserialize<'de> for SCellBox<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(SCellBox(SCell::<SUnsafeCell<T>>::deserialize(
            deserializer,
        )?))
    }
}

impl<T: CandidType + DeserializeOwned> SCellBox<T> {
    pub fn new(value: &T) -> Result<Self, OutOfMemory> {
        let sbox = SUnsafeCell::new(value)?;

        match SCell::new(&sbox) {
            Err(e) => {
                sbox.drop();
                Err(e)
            }
            Ok(scell) => Ok(Self(scell)),
        }
    }

    pub fn get_cloned(&self) -> T {
        let sbox = self.0.get();

        sbox.get_cloned()
    }

    pub fn set(&mut self, value: &T) -> Result<(), OutOfMemory> {
        // TODO: this is actually unsafe :c
        let mut sbox = self.0.get();
        let should_update = unsafe { sbox.set(value)? };

        if should_update {
            self.0.set(&sbox);
        }

        Ok(())
    }

    pub fn as_ptr(&self) -> u64 {
        self.0.as_ptr()
    }

    pub fn drop(self) {
        let sbox = self.0.get();
        sbox.drop();

        self.0.drop();
    }
}

impl<T> AsBytes for SCellBox<T> {
    unsafe fn as_bytes(&self) -> Vec<u8> {
        self.0.as_bytes()
    }

    unsafe fn from_bytes(bytes: &[u8]) -> Self {
        Self(SCell::from_bytes(bytes))
    }
}
