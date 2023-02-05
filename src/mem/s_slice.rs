use crate::mem::allocator::EMPTY_PTR;
use crate::mem::free_block::FreeBlock;
use crate::mem::StablePtr;
use crate::utils::mem_context::stable;
use std::usize;
use crate::encoding::AsFixedSizeBytes;

pub(crate) const FREE: u64 = 2usize.pow(u32::BITS - 1) as u64 - 1; // first biggest bit set to 0, other set to 1
pub(crate) const ALLOCATED: u64 = 2usize.pow(u32::BITS - 1) as u64; // first biggest bit set to 1, other set to 0
pub(crate) const PTR_SIZE: usize = <StablePtr as AsFixedSizeBytes>::SIZE;
pub(crate) const BLOCK_META_SIZE: usize = PTR_SIZE;
pub(crate) const BLOCK_MIN_TOTAL_SIZE: usize = PTR_SIZE * 4;

#[derive(Debug)]
pub(crate) enum Side {
    Start,
    End,
}

/// A smart-pointer for stable memory.
#[derive(Debug, Copy, Clone)]
pub struct SSlice {
    // ptr is shifted by BLOCK_META_SIZE for faster computations
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

    pub(crate) fn from_ptr(ptr: StablePtr, side: Side) -> Option<Self> {
        match side {
            Side::Start => {
                let size_1 = Self::read_size(ptr)?;

                Some(Self::new(ptr, size_1, false))
            }
            Side::End => {
                let size_1 = Self::read_size(ptr - BLOCK_META_SIZE as u64)?;

                Some(Self::new(
                    ptr - (BLOCK_META_SIZE * 2 + size_1) as u64,
                    size_1,
                    false,
                ))
            }
        }
    }

    pub(crate) fn validate(&self) -> Option<()> {
        let size_2 = Self::read_size(self.ptr + (BLOCK_META_SIZE + self.size) as u64)?;

        if self.size == size_2 {
            Some(())
        } else {
            None
        }
    }

    #[inline]
    pub(crate) fn to_free_block(self) -> FreeBlock {
        FreeBlock {
            ptr: self.ptr,
            size: self.size,
            transient: true,
        }
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
        self.get_size_bytes() + BLOCK_META_SIZE * 2
    }

    #[inline]
    pub fn _make_ptr_by_offset(self_ptr: u64, offset: usize) -> StablePtr {
        debug_assert_ne!(self_ptr, EMPTY_PTR);

        self_ptr + (BLOCK_META_SIZE + offset) as u64
    }

    #[inline]
    pub fn make_ptr_by_offset(&self, offset: usize) -> StablePtr {
        Self::_make_ptr_by_offset(self.as_ptr(), offset)
    }

    fn read_size(ptr: StablePtr) -> Option<usize> {
        let mut meta = [0u8; BLOCK_META_SIZE as usize];
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
        stable::write(ptr + (BLOCK_META_SIZE + size) as u64, &meta);
    }
}

#[cfg(test)]
mod tests {
    use crate::mem::s_slice::Side;
    use crate::utils::mem_context::stable;
    use crate::SSlice;

    #[test]
    fn read_write_work_fine() {
        stable::clear();
        stable::grow(10).expect("Unable to grow");

        let m1 = SSlice::new(0, 100, true);
        let m1 = SSlice::from_ptr(m1.get_total_size_bytes() as u64, Side::End).unwrap();

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
