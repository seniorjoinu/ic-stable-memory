//! A primitive smart-pointer that points to an allocated memory block.
//!
//! This data structure's main purpose is to tell the dimensions of an allocated memory block and allow taking
//! a pointer somewhere inside it. One can create such a memory block by calling [allocate](crate::allocate)
//! function. This is a managed resource, so please be careful and always [deallocate](crate::deallocate)
//! memory blocks, when you don't longer need them.

use crate::encoding::{AsFixedSizeBytes, Buffer};
use crate::mem::allocator::EMPTY_PTR;
use crate::mem::free_block::FreeBlock;
use crate::mem::{StablePtr, StablePtrBuf};
use crate::utils::mem_context::stable;

pub(crate) const ALLOCATED: u64 = 2u64.pow(u64::BITS - 1); // first biggest bit set to 1, other set to 0
pub(crate) const FREE: u64 = ALLOCATED - 1; // first biggest bit set to 0, other set to 1

/// An allocated block of stable memory.
///
/// Represented by a pointer to the first byte of the memory block and a [u64] size of this block in
/// bytes. It implements [Copy], but using it after deallocation is undefined behavior.
///
/// In stable memory each memory block has the following layout:
/// - bytes `0..8` - `size` + `allocated bit flag` (the flag uses the first bit of little endian encoded size)
/// - bytes `8..(size + 8)` - the data
/// - bytes `(size + 8)..(size + 16)` - another `size` + `allocated bit flag`
/// So, a memory block is simply `size` bytes of data wrapped with some metadata from both sides.
/// [FreeBlock](mem::free_block::FreeBlock) is stored exactly in a same way.
#[derive(Debug, Copy, Clone)]
pub struct SSlice {
    ptr: StablePtr,
    size: u64,
}

impl SSlice {
    pub(crate) fn new(ptr: StablePtr, size: u64, write_size: bool) -> Self {
        if write_size {
            Self::write_size(ptr, size);
        }

        Self { ptr, size }
    }

    /// Recreate an [SSlice] from a pointer to the front of the memory block.
    /// 
    /// See also [SSlice::from_rear_ptr].
    /// 
    /// This call will check whether a pointer is valid (points to an *allocated* memory block) and
    /// if it's not, it will return [None].
    pub fn from_ptr(ptr: StablePtr) -> Option<Self> {
        if ptr == 0 || ptr == EMPTY_PTR {
            return None;
        }

        let size = Self::read_size(ptr)?;

        Some(Self::new(ptr, size, false))
    }

    /// Recreate an [SSlice] from a pointer to the back of the memory block.
    /// 
    /// See also [SSlice::from_ptr].
    pub fn from_rear_ptr(ptr: StablePtr) -> Option<Self> {
        if ptr == 0 || ptr == EMPTY_PTR {
            return None;
        }

        let size = Self::read_size(ptr)?;

        Some(Self::new(
            ptr - (StablePtr::SIZE as u64) - size,
            size,
            false,
        ))
    }
    
    /// Returns a pointer to the memory block.
    /// 
    /// *Don't use this function to point to the data inside this memory block!* Use [SSlice::offset]
    /// instead.
    #[inline]
    pub fn as_ptr(&self) -> StablePtr {
        self.ptr
    }

    /// Returns the size of the data in this memory block in bytes.
    #[inline]
    pub fn get_size_bytes(&self) -> u64 {
        self.size
    }

    /// Returns the size of the whole memory block in bytes (including metadata).
    #[inline]
    pub fn get_total_size_bytes(&self) -> u64 {
        self.get_size_bytes() + StablePtr::SIZE as u64 * 2
    }

    /// Static analog of [SSlice::offset].
    /// 
    /// Does not perform boundary check.
    #[inline]
    pub fn _offset(self_ptr: u64, offset: u64) -> StablePtr {
        debug_assert_ne!(self_ptr, EMPTY_PTR);

        self_ptr + (StablePtr::SIZE as u64) + offset
    }

    /// Returns a pointer to the data inside [SSlice].
    /// 
    /// One should use this function to write data in a memory block by using [mem::write_fixed] or
    /// [mem::write_bytes].
    /// 
    /// # Panics
    /// Panics if boundary check fails (if the offset is outside the memory block).
    ///
    /// # Example
    /// ```rust
    /// # use ic_stable_memory::{allocate, mem};
    /// let slice = allocate(100);
    /// let ptr = slice.offset(20);
    /// 
    /// // will write `10` as little endian bytes into the memory block
    /// // starting from 20th byte
    /// unsafe { mem::write_fixed(ptr, 10u64); }
    /// ``` 
    #[inline]
    pub fn offset(&self, offset: u64) -> StablePtr {
        let ptr = Self::_offset(self.as_ptr(), offset);
        assert!(ptr <= self.as_ptr() + StablePtr::SIZE as u64 + self.get_size_bytes());
        
        ptr
    }
    
    #[inline]
    pub(crate) fn to_free_block(self) -> FreeBlock {
        FreeBlock::new(self.ptr, self.size)
    }

    fn read_size(ptr: StablePtr) -> Option<u64> {
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
            Some(size)
        } else {
            None
        }
    }

    fn write_size(ptr: StablePtr, size: u64) {
        let encoded_size = size | ALLOCATED;

        let meta = encoded_size.to_le_bytes();

        stable::write(ptr, &meta);
        stable::write(ptr + (StablePtr::SIZE as u64) + size, &meta);
    }
}

#[cfg(test)]
mod tests {
    use crate::encoding::AsFixedSizeBytes;
    use crate::mem::allocator::MIN_PTR;
    use crate::mem::s_slice::SSlice;
    use crate::mem::StablePtr;
    use crate::utils::mem_context::stable;

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

        unsafe { crate::mem::write_bytes(m1.offset(0), &a) };
        unsafe { crate::mem::write_bytes(m1.offset(8), &b) };
        unsafe { crate::mem::write_bytes(m1.offset(90), &c) };

        let mut a1 = [0u8; 8];
        let mut b1 = [0u8; 4];
        let mut c1 = [0u8; 9];

        unsafe { crate::mem::read_bytes(m1.offset(0), &mut a1) };
        unsafe { crate::mem::read_bytes(m1.offset(8), &mut b1) };
        unsafe { crate::mem::read_bytes(m1.offset(90), &mut c1) };

        assert_eq!(&a, &a1);
        assert_eq!(&b, &b1);
        assert_eq!(&c, &c1);
    }
}
