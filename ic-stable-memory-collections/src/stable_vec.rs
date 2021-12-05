/*use crate::types::StableVecError;
use ic_stable_memory_allocator::mem_block::{MemBlock, MemBlockSide};
use ic_stable_memory_allocator::mem_context::MemContext;
use ic_stable_memory_allocator::stable_memory_allocator::StableMemoryAllocator;
use ic_stable_memory_allocator::types::SMAError;
use std::marker::PhantomData;
use std::mem::size_of;

pub const STABLE_VEC_MARKER: [u8; 1] = [128u8];
pub const STABLE_VEC_GROW_SIZE_BYTES: usize = 256;
pub const STABLE_VEC_OVERHEAD_SIZE_BYTES: usize = 1 + size_of::<u64>() * 2;

pub struct StableVecInner<T: MemContext + Clone> {
    pub ptr: u64,
    pub len_ptrs: u64,
    pub occupied_size_bytes: u64,
    pub marker: PhantomData<T>,
}

impl<T: MemContext + Clone> StableVecInner<T> {
    pub fn get(&self, idx: u64, offset: u64, buf: &mut [u8], context: &T) -> bool {
        if idx >= self.len_ptrs {
            return false;
        }

        let mut item_ptr_buf = [0u8; size_of::<u64>()];
        let item_ptr_read_fail = StableMemoryAllocator::read_at(
            self.ptr,
            idx * size_of::<u64>() as u64,
            &mut item_ptr_buf,
            context,
        )
        .ok()
        .is_none();

        if item_ptr_read_fail {
            return false;
        }

        let item_ptr = u64::from_le_bytes(item_ptr_buf);

        StableMemoryAllocator::read_at(item_ptr, offset, buf, context).ok().is_some()
    }

    pub fn init(
        allocator: &mut StableMemoryAllocator<T>,
        context: &mut T,
    ) -> Result<StableVecInner<T>, StableVecError> {
        let ptr = allocator
            .allocate(STABLE_VEC_GROW_SIZE_BYTES as u64, context)
            .map_err(StableVecError::SMAError)?;

        let mem_block = MemBlock::read_at(ptr, MemBlockSide::Start, context)
            .ok_or(StableVecError::SMAError(SMAError::NoMemBlockAtAddress))?;

        let occupied_size_bytes = mem_block.size;

        let len_ptrs = 0u64;

        if !mem_block.write_bytes(0, &STABLE_VEC_MARKER, context) {
            return Err(StableVecError::SMAError(SMAError::OutOfBounds));
        }

        if !mem_block.write_bytes(1, &len_ptrs.to_le_bytes(), context) {
            return Err(StableVecError::SMAError(SMAError::OutOfBounds));
        }

        Ok(StableVecInner {
            ptr,
            len_ptrs,
            occupied_size_bytes,
            marker: PhantomData,
        })
    }

    pub fn reinit(ptr: u64, context: &T) -> Result<StableVecInner<T>, StableVecError> {
        let mem_block = MemBlock::read_at(ptr, MemBlockSide::Start, context)
            .ok_or(StableVecError::SMAError(SMAError::NoMemBlockAtAddress))?;

        let mut marker = [0u8; 1];
        if !mem_block.read_bytes(0, &mut marker, context) {
            return Err(StableVecError::SMAError(SMAError::OutOfBounds));
        }

        if marker != STABLE_VEC_MARKER {
            return Err(StableVecError::MarkerMismatch);
        }

        let mut len_ptrs_buf = [0u8; size_of::<u64>()];
        if !mem_block.read_bytes(1, &mut len_ptrs_buf, context) {
            return Err(StableVecError::SMAError(SMAError::OutOfBounds));
        }
        let len_ptrs = u64::from_le_bytes(len_ptrs_buf);

        let occupied_size_bytes = mem_block.size;

        Ok(StableVecInner {
            ptr,
            len_ptrs,
            occupied_size_bytes,
            marker: PhantomData,
        })
    }
}
*/
