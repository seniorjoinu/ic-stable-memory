use crate::utils::encode::{decode_one_allow_trailing, AsBytes};
use crate::{allocate, deallocate, reallocate, OutOfMemory, RawSCell};
use candid::types::{Serializer, Type};
use candid::{encode_one, CandidType};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use std::cell::UnsafeCell;

pub struct SUnsafeCell<T>(RawSCell<T>);

impl<T> CandidType for SUnsafeCell<T> {
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

impl<'de, T> Deserialize<'de> for SUnsafeCell<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(SUnsafeCell(RawSCell::<T>::deserialize(deserializer)?))
    }
}

impl<'de, T: DeserializeOwned + CandidType> SUnsafeCell<T> {
    pub fn new(it: &T) -> Result<Self, OutOfMemory> {
        let bytes = encode_one(it).expect("Unable to encode");
        let raw = allocate(bytes.len())?;

        raw._write_bytes(0, &bytes);

        Ok(Self(raw))
    }

    pub fn get_cloned(&self) -> T {
        let mut bytes = vec![0u8; self.0.get_size_bytes()];
        self.0._read_bytes(0, &mut bytes);

        decode_one_allow_trailing(&bytes).expect("Unable to decode")
    }

    /// # Safety
    /// Make sure you update all references pointing to this sbox after setting a new value to it.
    /// Set can cause a reallocation that will change the location of the data.
    /// Use the return bool value to determine if the location is changed (true = you need to update).
    pub unsafe fn set(&mut self, it: &T) -> Result<bool, OutOfMemory> {
        let bytes = encode_one(it).expect("Unable to encode");
        let mut res = false;

        if self.0.get_size_bytes() < bytes.len() {
            self.0 = reallocate(RawSCell::from_bytes(&self.0.as_bytes()), bytes.len())?;
            res = true;
        }

        self.0._write_bytes(0, &bytes);

        Ok(res)
    }

    pub fn drop(self) {
        deallocate(self.0)
    }
}

impl<T> AsBytes for SUnsafeCell<T> {
    unsafe fn as_bytes(&self) -> Vec<u8> {
        self.0.as_bytes()
    }

    unsafe fn from_bytes(bytes: &[u8]) -> Self {
        Self(RawSCell::from_bytes(bytes))
    }
}

#[cfg(test)]
mod tests {
    use crate::init_allocator;
    use crate::primitive::s_unsafe_cell::SUnsafeCell;
    use crate::utils::mem_context::stable;
    use candid::Nat;
    use ic_cdk::export::candid::{CandidType, Deserialize};

    #[derive(CandidType, Deserialize, Debug, PartialEq, Eq)]
    struct Test {
        pub a: Nat,
        pub b: String,
    }

    #[test]
    fn candid_membox_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let obj = Test {
            a: Nat::from(12341231231u64),
            b: String::from("The string"),
        };

        let membox = SUnsafeCell::new(&obj).expect("Should allocate just fine");
        let obj1 = membox.get_cloned();

        assert_eq!(obj, obj1);
    }
}
