use crate::encoding::{AsFixedSizeBytes, Buffer};
use crate::mem::s_slice::{Side, ALLOCATED, BLOCK_META_SIZE, FREE};
use crate::mem::StablePtrBuf;
use crate::{stable, SSlice, StablePtr};

#[derive(Debug, Copy, Clone)]
pub(crate) struct FreeBlock {
    pub ptr: u64,
    pub size: usize,
    pub transient: bool,
}

impl FreeBlock {
    #[inline]
    pub fn new(ptr: StablePtr, size: usize, transient: bool) -> Self {
        Self {
            ptr,
            size,
            transient,
        }
    }

    #[inline]
    pub fn new_total_size(ptr: StablePtr, total_size: usize) -> Self {
        Self::new(ptr, total_size - BLOCK_META_SIZE * 2, true)
    }

    #[inline]
    pub fn to_allocated(self) -> SSlice {
        SSlice::new(self.ptr, self.size, true)
    }

    pub fn from_ptr(ptr: StablePtr, side: Side, size_1_opt: Option<usize>) -> Option<Self> {
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

    #[inline]
    pub(crate) fn validate(&self) -> Option<()> {
        let size_2 = Self::read_size(self.ptr + (BLOCK_META_SIZE + self.size) as u64)?;

        if self.size == size_2 {
            Some(())
        } else {
            None
        }
    }

    #[inline]
    pub(crate) fn persist(&mut self) {
        if self.transient {
            Self::write_size(self.ptr, self.size);

            self.transient = false;
        }
    }

    #[inline]
    pub fn get_next_neighbor_ptr(&self) -> StablePtr {
        self.ptr + (BLOCK_META_SIZE * 2 + self.size) as u64
    }

    #[inline]
    pub fn get_prev_neighbor_ptr(&self) -> StablePtr {
        self.ptr
    }

    pub fn check_neighbor_is_also_free(
        &self,
        side: Side,
        min_ptr: StablePtr,
        max_ptr: StablePtr,
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

    pub fn set_free_ptrs(ptr: StablePtr, prev_ptr: StablePtr, next_ptr: StablePtr) {
        let mut buf = [0u8; 2 * StablePtr::SIZE];
        prev_ptr.as_fixed_size_bytes(&mut buf[0..StablePtr::SIZE]);
        next_ptr.as_fixed_size_bytes(&mut buf[StablePtr::SIZE..StablePtr::SIZE * 2]);

        stable::write(ptr + StablePtr::SIZE as u64, &buf);
    }

    pub fn set_prev_free_ptr(ptr: StablePtr, prev_ptr: StablePtr) {
        stable::write(
            ptr + StablePtr::SIZE as u64,
            prev_ptr.as_new_fixed_size_bytes()._deref(),
        );
    }

    pub fn get_prev_free_ptr(ptr: StablePtr) -> StablePtr {
        let mut buf = StablePtrBuf::new(StablePtr::SIZE);
        stable::read(ptr + StablePtr::SIZE as u64, &mut buf);

        u64::from_fixed_size_bytes(&buf)
    }

    pub fn set_next_free_ptr(ptr: StablePtr, next_ptr: StablePtr) {
        stable::write(
            ptr + (StablePtr::SIZE * 2) as u64,
            &next_ptr.as_new_fixed_size_bytes(),
        );
    }

    pub fn get_next_free_ptr(ptr: StablePtr) -> u64 {
        let mut buf = StablePtrBuf::new(u64::SIZE);
        stable::read(ptr + (StablePtr::SIZE * 2) as u64, &mut buf);

        u64::from_fixed_size_bytes(&buf)
    }

    pub fn get_total_size_bytes(&self) -> usize {
        self.size + BLOCK_META_SIZE * 2
    }

    fn read_size(ptr: StablePtr) -> Option<usize> {
        let mut meta = [0u8; u64::SIZE];
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

    fn write_size(ptr: StablePtr, size: usize) {
        let encoded_size = size as u64 & FREE;

        let meta = encoded_size.to_le_bytes();

        stable::write(ptr, &meta);
        stable::write(ptr + (StablePtr::SIZE + size) as u64, &meta);
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
