use candid::encode_one;
use candid::types::{Serializer, Type};
use ic_cdk::export::candid::{CandidType, Deserialize, Error as CandidError};
use std::marker::PhantomData;
use std::mem::size_of;
use crate::{allocate, MemBox};

pub const STABLE_VEC_DEFAULT_CAPACITY: u64 = 16;
pub const PTR_SIZE: u64 = size_of::<u64>() as u64;

#[derive(Debug)]
pub enum StackError {
    CandidError(CandidError),
    OutOfMemory,
}

#[derive(CandidType, Deserialize, Copy, Clone, Debug)]
pub struct MStack();

#[derive(Deserialize, Copy, Clone, Debug)]
pub struct Stack<T> {
    membox: MemBox<MStack>,
    data: PhantomData<T>,
}

impl<T> CandidType for Stack<T> {
    fn _ty() -> Type {
        Type::Empty
    }

    fn idl_serialize<S>(&self, serializer: S) -> Result<(), S::Error>
    where
        S: Serializer,
    {
        Ok(())
    }
}

#[derive(CandidType, Deserialize, Copy, Clone, Debug)]
pub struct StackSector;

#[derive(CandidType, Deserialize, Clone, Debug)]
struct StackInfo {
    len: u64,
    capacity: u64,
    sectors: Vec<MemBox<StackSector>>,
}

impl<'de, T: CandidType + Deserialize<'de>> Stack<T> {
    pub fn new_stack() -> Result<Self, StackError> {
        let info = StackInfo {
            len: 0,
            capacity: STABLE_VEC_DEFAULT_CAPACITY,
            sectors: vec![],
        };

        let info_encoded = encode_one(info).map_err(StackError::CandidError)?;
        let mut membox = allocate::<MStack>(info_encoded.len())
            .map_err(|_| StackError::OutOfMemory)?;

        membox._write_bytes(0, &info_encoded);

        Ok(Self {
            membox,
            data: PhantomData::default()
        })
    }

    pub fn new_stack_with_capacity(capacity: u64) -> Self {
        todo!()
    }

    pub fn push(&mut self, element: T) -> Result<(), StackError> {
        todo!()
    }

    pub fn pop(&mut self) -> Option<T> {
        todo!()
    }

    pub fn get_cloned(&self, idx: u64) -> Option<T> {
        todo!()
    }

    pub fn set(&mut self, idx: u64, element: T) -> Result<(), StackError> {
        todo!()
    }

    pub fn capacity(&self) -> u64 {
        todo!()
    }

    pub fn len(&self) -> u64 {
        todo!()
    }
}
