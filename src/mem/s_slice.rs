use crate::encoding::{AsFixedSizeBytes, Buffer};
use crate::mem::allocator::EMPTY_PTR;
use crate::mem::free_block::FreeBlock;
use crate::mem::{StablePtr, StablePtrBuf};
use crate::utils::mem_context::stable;
use std::usize;

pub(crate) const ALLOCATED: u64 = 2usize.pow(u64::BITS - 1) as u64; // first biggest bit set to 1, other set to 0
pub(crate) const FREE: u64 = ALLOCATED - 1; // first biggest bit set to 0, other set to 1

/// A smart-pointer for stable memory.
#[derive(Debug, Copy, Clone)]
pub struct SSlice {
    ptr: StablePtr,
    size: usize,
}

impl SSlice {
    pub(crate) fn new(ptr: StablePtr, size: usize, write_size: bool) -> Self {
        if write_size {
            Self::write_size(ptr, size);
        }

        Self { ptr, size }
    }

    pub fn from_ptr(ptr: StablePtr) -> Option<Self> {
        if ptr == 0 || ptr == EMPTY_PTR {
            return None;
        }

        let size = Self::read_size(ptr)?;

        Some(Self::new(ptr, size, false))
    }

    pub fn from_rear_ptr(ptr: StablePtr) -> Option<Self> {
        if ptr == 0 || ptr == EMPTY_PTR {
            return None;
        }

        let size = Self::read_size(ptr)?;

        Some(Self::new(
            ptr - (StablePtr::SIZE + size) as u64,
            size,
            false,
        ))
    }

    #[inline]
    pub(crate) fn to_free_block(self) -> FreeBlock {
        FreeBlock::new(self.ptr, self.size)
    }

    #[inline]
    pub fn as_ptr(&self) -> StablePtr {
        self.ptr
    }

    #[inline]
    pub fn get_size_bytes(&self) -> usize {
        self.size
    }

    #[inline]
    pub fn get_total_size_bytes(&self) -> usize {
        self.get_size_bytes() + StablePtr::SIZE * 2
    }

    #[inline]
    pub fn _make_ptr_by_offset(self_ptr: u64, offset: usize) -> StablePtr {
        debug_assert_ne!(self_ptr, EMPTY_PTR);

        self_ptr + (StablePtr::SIZE + offset) as u64
    }

    #[inline]
    pub fn make_ptr_by_offset(&self, offset: usize) -> StablePtr {
        Self::_make_ptr_by_offset(self.as_ptr(), offset)
    }

    fn read_size(ptr: StablePtr) -> Option<usize> {
        let mut meta = StablePtrBuf::new(StablePtr::SIZE);
        stable::read(ptr, &mut meta);

        let encoded_size = u64::from_le_bytes(meta);
        let mut size = encoded_size;

        let allocated = if encoded_size & ALLOCATED == ALLOCATED {
            size &= FREE;
            true
        } else {
            false
        };

        if allocated {
            Some(size as usize)
        } else {
            None
        }
    }

    fn write_size(ptr: StablePtr, size: usize) {
        let encoded_size = size as u64 | ALLOCATED;

        let meta = encoded_size.to_le_bytes();

        stable::write(ptr, &meta);
        stable::write(ptr + (StablePtr::SIZE + size) as u64, &meta);
    }
}

#[cfg(test)]
mod tests {
    use crate::encoding::AsFixedSizeBytes;
    use crate::mem::allocator::MIN_PTR;
    use crate::mem::s_slice::SSlice;
    use crate::utils::mem_context::stable;
    use crate::StablePtr;

    #[test]
    fn read_write_work_fine() {
        stable::clear();
        stable::grow(10).expect("Unable to grow");

        let m1 = SSlice::new(MIN_PTR, 100, true);
        let m1 = SSlice::from_rear_ptr(
            MIN_PTR + m1.get_total_size_bytes() as u64 - StablePtr::SIZE as u64,
        )
        .unwrap();

        let a = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
        let b = vec![1u8, 3, 3, 7];
        let c = vec![9u8, 8, 7, 6, 5, 4, 3, 2, 1];

        unsafe { crate::mem::write_bytes(m1.make_ptr_by_offset(0), &a) };
        unsafe { crate::mem::write_bytes(m1.make_ptr_by_offset(8), &b) };
        unsafe { crate::mem::write_bytes(m1.make_ptr_by_offset(90), &c) };

        let mut a1 = [0u8; 8];
        let mut b1 = [0u8; 4];
        let mut c1 = [0u8; 9];

        unsafe { crate::mem::read_bytes(m1.make_ptr_by_offset(0), &mut a1) };
        unsafe { crate::mem::read_bytes(m1.make_ptr_by_offset(8), &mut b1) };
        unsafe { crate::mem::read_bytes(m1.make_ptr_by_offset(90), &mut c1) };

        assert_eq!(&a, &a1);
        assert_eq!(&b, &b1);
        assert_eq!(&c, &c1);
    }
}
