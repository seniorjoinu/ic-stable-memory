use crate::mem::free_block::FreeBlock;
use crate::utils::encoding::AsFixedSizeBytes;
use crate::utils::mem_context::stable;
use std::mem::size_of;
use std::usize;

pub(crate) const FREE: u64 = 2usize.pow(u32::BITS - 1) as u64 - 1; // first biggest bit set to 0, other set to 1
pub(crate) const ALLOCATED: u64 = 2usize.pow(u32::BITS - 1) as u64; // first biggest bit set to 1, other set to 0
pub(crate) const PTR_SIZE: usize = size_of::<u64>();
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
    ptr: u64,
    size: usize,
}

impl SSlice {
    pub(crate) fn new(ptr: u64, size: usize, write_size: bool) -> Self {
        if write_size {
            Self::write_size(ptr, size);
        }

        Self { ptr, size }
    }

    pub(crate) fn from_ptr(ptr: u64, side: Side) -> Option<Self> {
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
    pub fn write_bytes(&self, offset: usize, data: &[u8]) {
        Self::_write_bytes(self.ptr, offset, data);
    }

    #[inline]
    pub fn read_bytes(&self, offset: usize, data: &mut [u8]) {
        Self::_read_bytes(self.ptr, offset, data)
    }

    #[inline]
    pub fn get_ptr(&self) -> u64 {
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
    pub fn _write_bytes(ptr: u64, offset: usize, data: &[u8]) {
        stable::write(ptr + (BLOCK_META_SIZE + offset) as u64, data);
    }

    #[inline]
    pub fn _read_bytes(ptr: u64, offset: usize, data: &mut [u8]) {
        stable::read(ptr + (BLOCK_META_SIZE + offset) as u64, data);
    }

    fn read_size(ptr: u64) -> Option<usize> {
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

    fn write_size(ptr: u64, size: usize) {
        let encoded_size = size as u64 | ALLOCATED;

        let meta = encoded_size.to_le_bytes();

        stable::write(ptr, &meta);
        stable::write(ptr + (BLOCK_META_SIZE + size) as u64, &meta);
    }
}

impl SSlice {
    pub fn _as_fixed_size_bytes_read<T: AsFixedSizeBytes<[u8; T::SIZE]>>(
        ptr: u64,
        offset: usize,
    ) -> T
    where
        [(); T::SIZE]: Sized,
    {
        let mut buf = T::super_size_u8_arr();
        SSlice::_read_bytes(ptr, offset, &mut buf);

        T::from_bytes(buf)
    }

    #[inline]
    pub fn _as_fixed_size_bytes_write<T: AsFixedSizeBytes<[u8; T::SIZE]>>(
        ptr: u64,
        offset: usize,
        it: T,
    ) where
        [(); T::SIZE]: Sized,
    {
        SSlice::_write_bytes(ptr, offset, &it.to_bytes())
    }

    #[inline]
    pub fn as_fixed_size_bytes_read<T: AsFixedSizeBytes<[u8; T::SIZE]>>(&self, offset: usize) -> T
    where
        [(); T::SIZE]: Sized,
    {
        Self::_as_fixed_size_bytes_read(self.ptr, offset)
    }

    #[inline]
    pub fn as_fixed_size_bytes_write<T: AsFixedSizeBytes<[u8; T::SIZE]>>(
        &self,
        offset: usize,
        it: T,
    ) where
        [(); T::SIZE]: Sized,
    {
        Self::_as_fixed_size_bytes_write(self.ptr, offset, it)
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

        m1.write_bytes(0, &a);
        m1.write_bytes(8, &b);
        m1.write_bytes(90, &c);

        let mut a1 = [0u8; 8];
        let mut b1 = [0u8; 4];
        let mut c1 = [0u8; 9];

        m1.read_bytes(0, &mut a1);
        m1.read_bytes(8, &mut b1);
        m1.read_bytes(90, &mut c1);

        assert_eq!(&a, &a1);
        assert_eq!(&b, &b1);
        assert_eq!(&c, &c1);
    }
}
