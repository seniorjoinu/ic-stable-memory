use crate::stable_linked_list::inner::StableLinkedListInner;
use crate::types::{StableArrayListError, STABLE_ARRAY_LIST_MARKER};
use ic_stable_memory_allocator::mem_block::{MemBlock, MemBlockSide};
use ic_stable_memory_allocator::mem_context::MemContext;
use ic_stable_memory_allocator::stable_memory_allocator::StableMemoryAllocator;
use ic_stable_memory_allocator::types::SMAError;
use std::marker::PhantomData;
use std::mem::size_of;

pub struct StableArrayListInner<T: MemContext + Clone> {
    pub ptr: u64,
    pub marker: PhantomData<T>,
}

// TODO: add delete + delete_at functions

impl<T: MemContext + Clone> StableArrayListInner<T> {
    pub fn new(
        capacity_step: u64,
        allocator: &mut StableMemoryAllocator<T>,
        context: &mut T,
    ) -> Result<Self, StableArrayListError> {
        let mut mem_block = allocator
            .allocate(1 + size_of::<u64>() as u64 * 3, context)
            .map_err(StableArrayListError::SMAError)?;

        let linked_list = StableLinkedListInner::new(allocator, context)
            .map_err(StableArrayListError::StableLinkedListError)?;

        mem_block
            .write_bytes(0, &STABLE_ARRAY_LIST_MARKER, context)
            .unwrap();
        mem_block.write_u64(1, 0, context).unwrap();
        mem_block
            .write_u64(1 + size_of::<u64>() as u64, capacity_step, context)
            .unwrap();
        mem_block
            .write_u64(1 + size_of::<u64>() as u64 * 2, linked_list.ptr, context)
            .unwrap();

        Ok(Self {
            ptr: mem_block.ptr,
            marker: PhantomData,
        })
    }

    pub fn read_at(ptr: u64, context: &T) -> Result<Self, StableArrayListError> {
        let mem_block = MemBlock::read_at(ptr, MemBlockSide::Start, context).ok_or(
            StableArrayListError::SMAError(SMAError::NoMemBlockAtAddress),
        )?;

        let mut marker_buf = [0u8; 1];
        mem_block
            .read_bytes(0, &mut marker_buf, context)
            .map_err(StableArrayListError::SMAError)?;

        if marker_buf != STABLE_ARRAY_LIST_MARKER {
            return Err(StableArrayListError::MarkerMismatch);
        }

        Ok(Self {
            ptr,
            marker: PhantomData,
        })
    }

    pub fn get_len(&self, context: &T) -> u64 {
        let mem_block = MemBlock::read_at(self.ptr, MemBlockSide::Start, context).unwrap();

        mem_block.read_u64(1, context).unwrap()
    }

    fn set_len(&mut self, new_len: u64, context: &mut T) {
        let mut mem_block = MemBlock::read_at(self.ptr, MemBlockSide::Start, context).unwrap();

        mem_block.write_u64(1, new_len, context).unwrap();
    }

    pub fn get_capacity_step(&self, context: &T) -> u64 {
        let mem_block = MemBlock::read_at(self.ptr, MemBlockSide::Start, context).unwrap();

        mem_block
            .read_u64(1 + size_of::<u64>() as u64, context)
            .unwrap()
    }

    fn get_linked_list_ptr(&self, context: &T) -> u64 {
        let mem_block = MemBlock::read_at(self.ptr, MemBlockSide::Start, context).unwrap();

        mem_block
            .read_u64(1 + size_of::<u64>() as u64 * 2, context)
            .unwrap()
    }
}
