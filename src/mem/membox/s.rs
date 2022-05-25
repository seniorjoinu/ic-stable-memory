use crate::utils::encode::decode_one_allow_trailing;
use crate::{allocate, deallocate, reallocate, OutOfMemory, RawSBox};
use candid::{encode_one, CandidType};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};

#[derive(Debug)]
pub enum SBoxError {
    CandidError(candid::Error),
    OutOfMemory,
}

impl SBoxError {
    pub fn unwrap_oom(self) -> OutOfMemory {
        match self {
            SBoxError::OutOfMemory => OutOfMemory,
            _ => unreachable!(),
        }
    }
}

#[derive(CandidType)]
pub struct SBox<T>(RawSBox<T>);

impl<T> Clone for SBox<T> {
    fn clone(&self) -> Self {
        Self(self.0)
    }
}

impl<T> Copy for SBox<T> {}

impl<'de, T: CandidType + Deserialize<'de>> Deserialize<'de> for SBox<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(SBox(RawSBox::<T>::deserialize(deserializer)?))
    }
}

impl<'de, T: DeserializeOwned + CandidType> SBox<T> {
    pub fn new(it: &T) -> Result<Self, SBoxError> {
        let bytes = encode_one(it).map_err(SBoxError::CandidError)?;
        let mut raw = allocate(bytes.len()).map_err(|_| SBoxError::OutOfMemory)?;

        raw._write_bytes(0, &bytes);

        Ok(Self::from_raw(raw))
    }

    pub fn get_cloned(&self) -> Result<T, SBoxError> {
        let mut bytes = vec![0u8; self.0.get_size_bytes()];
        self.0._read_bytes(0, &mut bytes);

        decode_one_allow_trailing(&bytes).map_err(SBoxError::CandidError)
    }

    /// # Safety
    /// Make sure you update all references pointing to this sbox after setting a new value to it.
    /// Set can cause a reallocation that will change the location of the data.
    /// Use the return bool value to determine if the location is changed (true = you need to update).
    pub unsafe fn set(&mut self, it: &T) -> Result<bool, SBoxError> {
        let bytes = encode_one(it).map_err(SBoxError::CandidError)?;
        let mut res = false;

        if self.0.get_size_bytes() < bytes.len() {
            self.0 = reallocate(self.as_raw(), bytes.len()).map_err(|_| SBoxError::OutOfMemory)?;
            res = true;
        }

        self.0._write_bytes(0, &bytes);

        Ok(res)
    }

    pub fn destroy(self) {
        deallocate(self.0)
    }

    pub fn as_raw(&self) -> RawSBox<T> {
        self.0
    }

    pub fn from_raw(membox: RawSBox<T>) -> Self {
        Self(membox)
    }
}

#[cfg(test)]
mod tests {
    use crate::init_allocator;
    use crate::mem::membox::s::SBox;
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

        let membox = SBox::new(&obj).expect("Should allocate just fine");
        let obj1 = membox.get_cloned().expect("Should deserialize just fine");

        assert_eq!(obj, obj1);
    }
}
