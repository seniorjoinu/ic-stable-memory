use crate::MemBox;
use candid::parser::value::{IDLValue, IDLValueVisitor};
use candid::types::{Serializer, Type};
use candid::utils::decode_one_allow_trailing;
use candid::{decode_one, encode_one};
use ic_cdk::export::candid::{CandidType, Deserialize};
use serde::de::{DeserializeOwned, Error};
use serde::Deserializer;
use std::marker::PhantomData;

impl<T> CandidType for MemBox<T> {
    fn _ty() -> Type {
        Type::Nat64
    }

    fn idl_serialize<S>(&self, serializer: S) -> Result<(), S::Error>
    where
        S: Serializer,
    {
        self.get_ptr().idl_serialize(serializer)
    }
}

impl<'de, T> Deserialize<'de> for MemBox<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let idl_value = deserializer.deserialize_u64(IDLValueVisitor)?;
        match idl_value {
            IDLValue::Nat64(ptr) => Ok(MemBox {
                ptr,
                data: PhantomData::default(),
            }),
            _ => Err(D::Error::custom("Unable to deserialize a Membox")),
        }
    }
}

#[derive(Debug)]
pub enum CandidMemBoxError {
    CandidError(candid::Error),
    MemBoxOverflow(Vec<u8>),
}

impl<'de, T: DeserializeOwned + CandidType> MemBox<T> {
    pub fn get_cloned(&self) -> Result<T, CandidMemBoxError> {
        let mut bytes = vec![0u8; self.get_size_bytes()];
        self._read_bytes(0, &mut bytes);

        decode_one_allow_trailing(&bytes).map_err(CandidMemBoxError::CandidError)
    }

    pub fn set(&mut self, it: T) -> Result<(), CandidMemBoxError> {
        let bytes = encode_one(it).map_err(CandidMemBoxError::CandidError)?;
        if self.get_size_bytes() < bytes.len() {
            return Err(CandidMemBoxError::MemBoxOverflow(bytes));
        }

        self._write_bytes(0, &bytes);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::mem::membox::candid::CandidMemBoxError;
    use crate::utils::mem_context::stable;
    use crate::MemBox;
    use candid::Nat;
    use ic_cdk::export::candid::{CandidType, Deserialize};

    #[derive(CandidType, Deserialize, Debug, PartialEq, Eq, Clone)]
    struct Test {
        pub a: Nat,
        pub b: String,
    }

    #[test]
    fn candid_membox_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();

        let mut tiny_membox = unsafe { MemBox::<Test>::new(0, 20, true) };
        let obj = Test {
            a: Nat::from(12341231231u64),
            b: String::from("The string that sure never fits into 20 bytes"),
        };

        let res = tiny_membox.set(obj.clone()).expect_err("It should fail");
        match res {
            CandidMemBoxError::MemBoxOverflow(encoded_obj) => {
                let mut membox = unsafe { MemBox::<Test>::new(0, encoded_obj.len(), true) };
                membox._write_bytes(0, &encoded_obj);

                let obj1 = membox.get_cloned().unwrap();

                assert_eq!(obj, obj1);
            }
            _ => unreachable!("It should encode just fine"),
        };
    }
}
