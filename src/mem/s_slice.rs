use crate::mem::free_block::FreeBlock;
use crate::utils::mem_context::stable;
use speedy::{Readable, Writable};
use std::mem::size_of;
use std::usize;

pub(crate) const FREE: usize = 2usize.pow(usize::BITS - 1) - 1; // first biggest bit set to 0, other set to 1
pub(crate) const ALLOCATED: usize = 2usize.pow(usize::BITS - 1); // first biggest bit set to 1, other set to 0
pub(crate) const PTR_SIZE: usize = size_of::<u64>();
pub(crate) const BLOCK_META_SIZE: usize = PTR_SIZE;
pub(crate) const BLOCK_MIN_TOTAL_SIZE: usize = PTR_SIZE * 4;

#[derive(Debug)]
pub(crate) enum Side {
    Start,
    End,
}

/// A smart-pointer for stable memory.
#[derive(Debug, Copy, Clone, Readable, Writable)]
pub struct SSlice {
    pub ptr: u64,
    pub size: usize,
}

impl SSlice {
    pub(crate) fn new(ptr: u64, size: usize, write_size: bool) -> Self {
        if write_size {
            Self::write_size(ptr, size);
        }

        Self { ptr, size }
    }

    pub(crate) fn from_ptr(ptr: u64, side: Side, check_sizes: bool) -> Option<Self> {
        match side {
            Side::Start => {
                let size_1 = Self::read_size(ptr)?;

                if !check_sizes {
                    return Some(Self::new(ptr, size_1, false));
                }

                let size_2 = Self::read_size(ptr + (BLOCK_META_SIZE + size_1) as u64)?;

                if size_1 == size_2 {
                    Some(Self::new(ptr, size_1, false))
                } else {
                    None
                }
            }
            Side::End => {
                let size_1 = Self::read_size(ptr - BLOCK_META_SIZE as u64)?;

                if !check_sizes {
                    return Some(Self::new(
                        ptr - (BLOCK_META_SIZE * 2 + size_1) as u64,
                        size_1,
                        false,
                    ));
                }

                let size_2 = Self::read_size(ptr - (BLOCK_META_SIZE * 2 + size_1) as u64)?;

                if size_1 == size_2 {
                    Some(Self::new(
                        ptr - (BLOCK_META_SIZE * 2 + size_1) as u64,
                        size_1,
                        false,
                    ))
                } else {
                    None
                }
            }
        }
    }

    pub(crate) fn to_free_block(self) -> FreeBlock {
        FreeBlock {
            ptr: self.ptr,
            size: self.size,
            transient: true,
        }
    }

    pub fn _write_bytes(&self, offset: usize, data: &[u8]) {
        assert!(offset + data.len() <= self.size);

        stable::write(self.get_ptr() + (BLOCK_META_SIZE + offset) as u64, data);
    }

    pub fn _write_word(&self, offset: usize, word: u64) {
        let num = word.to_le_bytes();
        self._write_bytes(offset, &num);
    }

    pub fn _read_bytes(&self, offset: usize, data: &mut [u8]) {
        assert!(data.len() + offset <= self.size);

        stable::read(self.get_ptr() + (BLOCK_META_SIZE + offset) as u64, data);
    }

    pub fn _read_word(&self, offset: usize) -> u64 {
        let mut buf = [0u8; PTR_SIZE];
        self._read_bytes(offset, &mut buf);

        u64::from_le_bytes(buf)
    }

    pub fn get_ptr(&self) -> u64 {
        self.ptr
    }

    pub fn get_size_bytes(&self) -> usize {
        self.size
    }

    pub fn get_total_size_bytes(&self) -> usize {
        self.get_size_bytes() + BLOCK_META_SIZE * 2
    }

    fn read_size(ptr: u64) -> Option<usize> {
        let mut meta = [0u8; BLOCK_META_SIZE as usize];
        stable::read(ptr, &mut meta);

        let encoded_size = usize::from_le_bytes(meta);
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

    fn write_size(ptr: u64, size: usize) {
        let encoded_size = size | ALLOCATED;

        let meta = encoded_size.to_le_bytes();

        stable::write(ptr, &meta);
        stable::write(ptr + (BLOCK_META_SIZE + size) as u64, &meta);
    }
}

/// Only run these tests with `-- --test-threads=1`. It fails otherwise.
#[cfg(test)]
mod tests {
    use crate::mem::s_slice::{Side, BLOCK_META_SIZE};
    use crate::utils::mem_context::stable;
    use crate::SSlice;

    #[test]
    fn read_write_work_fine() {
        stable::clear();
        stable::grow(10).expect("Unable to grow");

        let m1 = SSlice::new(0, 100, true);

        let a = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
        let b = vec![1u8, 3, 3, 7];
        let c = vec![9u8, 8, 7, 6, 5, 4, 3, 2, 1];

        m1._write_bytes(0, &a);
        m1._write_bytes(8, &b);
        m1._write_bytes(90, &c);

        let mut a1 = [0u8; 8];
        let mut b1 = [0u8; 4];
        let mut c1 = [0u8; 9];

        m1._read_bytes(0, &mut a1);
        m1._read_bytes(8, &mut b1);
        m1._read_bytes(90, &mut c1);

        assert_eq!(&a, &a1);
        assert_eq!(&b, &b1);
        assert_eq!(&c, &c1);
    }
}
