use crate::primitive::s_slice::Side;
use crate::utils::encode::decode_one_allow_trailing;
use crate::{allocate, deallocate, reallocate, OutOfMemory, SSlice};
use candid::types::{Serializer, Type};
use candid::{encode_one, CandidType};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use std::cmp::Ordering;
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};

pub struct SUnsafeCell<T>(SSlice<T>);

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
            self.0 = reallocate(self.0.clone(), bytes.len())?;
            res = true;
        }

        self.0._write_bytes(0, &bytes);

        Ok(res)
    }

    pub fn _allocated_size(&self) -> usize {
        self.0.get_size_bytes()
    }

    pub unsafe fn from_ptr(ptr: u64) -> Self {
        assert_ne!(ptr, 0);
        Self(SSlice::from_ptr(ptr, Side::Start).unwrap())
    }

    pub unsafe fn as_ptr(&self) -> u64 {
        self.0.ptr
    }

    pub fn drop(self) {
        deallocate(self.0)
    }
}

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
        Ok(SUnsafeCell(SSlice::<T>::deserialize(deserializer)?))
    }
}

impl<T: Eq + CandidType + DeserializeOwned> PartialEq<Self> for SUnsafeCell<T> {
    fn eq(&self, other: &Self) -> bool {
        self.get_cloned().eq(&other.get_cloned())
    }
}

impl<T: Eq + CandidType + DeserializeOwned> Eq for SUnsafeCell<T> {}

impl<T: Ord + CandidType + DeserializeOwned> PartialOrd<Self> for SUnsafeCell<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.get_cloned().partial_cmp(&other.get_cloned())
    }
}

impl<T: Ord + CandidType + DeserializeOwned> Ord for SUnsafeCell<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.get_cloned().cmp(&other.get_cloned())
    }

    fn max(self, other: Self) -> Self
    where
        Self: Sized,
    {
        let self_val = self.get_cloned();
        let other_val = other.get_cloned();

        if other_val > self_val {
            other
        } else {
            self
        }
    }

    fn min(self, other: Self) -> Self
    where
        Self: Sized,
    {
        let self_val = self.get_cloned();
        let other_val = other.get_cloned();

        if other_val < self_val {
            other
        } else {
            self
        }
    }

    fn clamp(self, min: Self, max: Self) -> Self
    where
        Self: Sized,
    {
        let self_val = self.get_cloned();
        let min_val = min.get_cloned();
        if min_val > self_val {
            return min;
        }

        let max_val = max.get_cloned();
        if max_val < self_val {
            return max;
        }

        self
    }
}

impl<T: Hash + CandidType + DeserializeOwned> Hash for SUnsafeCell<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.get_cloned().hash(state)
    }
}

impl<T: Debug + CandidType + DeserializeOwned> Debug for SUnsafeCell<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.get_cloned().fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use crate::primitive::s_unsafe_cell::SUnsafeCell;
    use crate::utils::mem_context::stable;
    use crate::{init_allocator, stable_memory_init};
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
