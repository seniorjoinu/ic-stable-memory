use crate::mem::s_slice::{Side, ALLOCATED, BLOCK_META_SIZE, BLOCK_MIN_TOTAL_SIZE, FREE, PTR_SIZE};
use crate::{stable, SSlice, _debug_print_allocator, PAGE_SIZE_BYTES};

#[derive(Debug, Copy, Clone)]
pub(crate) struct FreeBlock {
    pub ptr: u64,
    pub size: usize,
    pub transient: bool,
}

impl FreeBlock {
    pub fn new(ptr: u64, size: usize, transient: bool) -> Self {
        assert!(size >= BLOCK_MIN_TOTAL_SIZE - 2 * BLOCK_META_SIZE);

        Self {
            ptr,
            size,
            transient,
        }
    }

    pub fn new_total_size(ptr: u64, total_size: usize) -> Self {
        Self::new(ptr, total_size - BLOCK_META_SIZE * 2, true)
    }

    pub fn to_allocated(self) -> SSlice {
        SSlice::new(self.ptr, self.size)
    }

    pub fn from_ptr(
        ptr: u64,
        side: Side,
        size_1_opt: Option<usize>,
        check_sizes: bool,
    ) -> Option<Self> {
        match side {
            Side::Start => {
                let size_1 = if let Some(s) = size_1_opt {
                    s
                } else {
                    Self::read_size(ptr)?
                };
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
                let size_1 = if let Some(s) = size_1_opt {
                    s
                } else {
                    Self::read_size(ptr - BLOCK_META_SIZE as u64)?
                };

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

    pub fn persist(&mut self) {
        if self.transient {
            Self::write_size(self.ptr, self.size);

            self.transient = false;
        }
    }

    pub fn get_next_neighbor_ptr(&self) -> u64 {
        self.ptr + (BLOCK_META_SIZE * 2 + self.size) as u64
    }

    pub fn get_prev_neighbor_ptr(&self) -> u64 {
        self.ptr
    }

    pub fn check_neighbor_is_also_free(
        &self,
        side: Side,
        min_ptr: u64,
        max_ptr: u64,
    ) -> Option<usize> {
        match side {
            Side::Start => {
                let prev_neighbor_ptr = self.get_prev_neighbor_ptr();

                if prev_neighbor_ptr >= min_ptr {
                    Self::read_size(prev_neighbor_ptr)
                } else {
                    None
                }
            }
            Side::End => {
                let next_neighbor_ptr = self.get_next_neighbor_ptr();

                if next_neighbor_ptr < max_ptr {
                    Self::read_size(next_neighbor_ptr)
                } else {
                    None
                }
            }
        }
    }

    pub fn set_free_ptrs(&self, prev_ptr: u64, next_ptr: u64) {
        let mut buf = [0u8; 2 * PTR_SIZE];
        buf[0..PTR_SIZE].copy_from_slice(&prev_ptr.to_le_bytes());
        buf[PTR_SIZE..PTR_SIZE * 2].copy_from_slice(&next_ptr.to_le_bytes());

        self._write_bytes(0, &buf)
    }

    pub fn set_prev_free_ptr(&self, prev_ptr: u64) {
        self._write_word(0, prev_ptr);
    }

    pub fn get_prev_free_ptr(&self) -> u64 {
        self._read_word(0)
    }

    pub fn set_next_free_ptr(&self, next_ptr: u64) {
        self._write_word(PTR_SIZE, next_ptr);
    }

    pub fn get_next_free_ptr(&self) -> u64 {
        self._read_word(PTR_SIZE)
    }

    pub fn get_total_size_bytes(&self) -> usize {
        self.size + BLOCK_META_SIZE * 2
    }

    pub fn _write_bytes(&self, offset: usize, data: &[u8]) {
        assert!(offset + data.len() <= self.size);

        stable::write(self.ptr + (BLOCK_META_SIZE + offset) as u64, data);
    }

    pub fn _write_word(&self, offset: usize, word: u64) {
        let num = word.to_le_bytes();
        self._write_bytes(offset, &num);
    }

    pub fn _read_bytes(&self, offset: usize, data: &mut [u8]) {
        assert!(data.len() + offset <= self.size);

        stable::read(self.ptr + (BLOCK_META_SIZE + offset) as u64, data);
    }

    pub fn _read_word(&self, offset: usize) -> u64 {
        let mut buf = [0u8; PTR_SIZE];
        self._read_bytes(offset, &mut buf);

        u64::from_le_bytes(buf)
    }

    fn read_size(ptr: u64) -> Option<usize> {
        let mut meta = [0u8; PTR_SIZE];
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
            None
        } else {
            Some(size)
        }
    }

    fn write_size(ptr: u64, size: usize) {
        let encoded_size = size & FREE;

        let meta = encoded_size.to_le_bytes();

        stable::write(ptr, &meta);
        stable::write(ptr + (PTR_SIZE + size) as u64, &meta);
    }
}
