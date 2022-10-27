use crate::mem::s_slice::{Side, ALLOCATED, BLOCK_META_SIZE, BLOCK_MIN_TOTAL_SIZE, FREE, PTR_SIZE};
use crate::{stable, SSlice};
use copy_as_bytes::traits::{AsBytes, SuperSized};

#[derive(Debug, Copy, Clone)]
pub(crate) struct FreeBlock {
    pub ptr: u64,
    pub size: usize,
    pub transient: bool,
}

impl FreeBlock {
    pub fn new(ptr: u64, size: usize, transient: bool) -> Self {
        assert!(size >= PTR_SIZE * 2);

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
        SSlice::new(self.ptr, self.size, true)
    }

    pub fn from_ptr(ptr: u64, side: Side, size_1_opt: Option<usize>) -> Option<Self> {
        match side {
            Side::Start => {
                let size_1 = if let Some(s) = size_1_opt {
                    s
                } else {
                    Self::read_size(ptr)?
                };

                Some(Self::new(ptr, size_1, false))
            }
            Side::End => {
                let size_1 = if let Some(s) = size_1_opt {
                    s
                } else {
                    Self::read_size(ptr - BLOCK_META_SIZE as u64)?
                };

                let it_ptr = ptr - (BLOCK_META_SIZE * 2 + size_1) as u64;
                let it = Self::new(it_ptr, size_1, false);

                Some(it)
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

    pub(crate) fn persist(&mut self) {
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

    pub fn set_free_ptrs(ptr: u64, prev_ptr: u64, next_ptr: u64) {
        let mut buf = [0u8; 2 * PTR_SIZE];
        buf[0..PTR_SIZE].copy_from_slice(&prev_ptr.to_le_bytes());
        buf[PTR_SIZE..PTR_SIZE * 2].copy_from_slice(&next_ptr.to_le_bytes());

        stable::write(ptr + PTR_SIZE as u64, &buf);
    }

    pub fn set_prev_free_ptr(ptr: u64, prev_ptr: u64) {
        stable::write(ptr + PTR_SIZE as u64, &prev_ptr.to_bytes());
    }

    pub fn get_prev_free_ptr(ptr: u64) -> u64 {
        let mut buf = u64::super_size_u8_arr();
        stable::read(ptr + PTR_SIZE as u64, &mut buf);

        u64::from_bytes(buf)
    }

    pub fn set_next_free_ptr(ptr: u64, next_ptr: u64) {
        stable::write(ptr + (PTR_SIZE * 2) as u64, &next_ptr.to_bytes());
    }

    pub fn get_next_free_ptr(ptr: u64) -> u64 {
        let mut buf = u64::super_size_u8_arr();
        stable::read(ptr + (PTR_SIZE * 2) as u64, &mut buf);

        u64::from_bytes(buf)
    }

    pub fn get_total_size_bytes(&self) -> usize {
        self.size + BLOCK_META_SIZE * 2
    }

    fn read_size(ptr: u64) -> Option<usize> {
        let mut meta = [0u8; PTR_SIZE];
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
            None
        } else {
            Some(size as usize)
        }
    }

    fn write_size(ptr: u64, size: usize) {
        let encoded_size = size as u64 & FREE;

        let meta = encoded_size.to_le_bytes();

        stable::write(ptr, &meta);
        stable::write(ptr + (PTR_SIZE + size) as u64, &meta);
    }
}

#[cfg(test)]
mod tests {
    use crate::mem::free_block::FreeBlock;
    use crate::mem::free_block::Side;
    use crate::utils::mem_context::stable;

    #[test]
    fn read_write_work_fine() {
        stable::clear();
        stable::grow(1).expect("Unable to grow");

        let mut m1 = FreeBlock::new(0, 100, true);
        m1.persist();

        let m1 = FreeBlock::from_ptr(m1.get_total_size_bytes() as u64, Side::End, None).unwrap();
    }
}
