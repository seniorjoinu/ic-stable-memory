use crate::encoding::{AsFixedSizeBytes, Buffer};
use crate::mem::allocator::MIN_PTR;
use crate::mem::s_slice::{SSlice, ALLOCATED, FREE};
use crate::mem::StablePtr;
use crate::stable;
use candid::{CandidType, Deserialize};
use std::cmp::Ordering;

// Layout is:
// size + allocated flag (combined): u64
// data: [u8; size]
// size + allocate flag (dublicate): u64

#[derive(Debug, Copy, Clone, CandidType, Deserialize)]
pub(crate) struct FreeBlock {
    ptr: u64,  // always points to the first size+flag word position
    size: u64, // available size
}

impl FreeBlock {
    #[inline]
    pub fn new(ptr: StablePtr, size: u64) -> Self {
        Self { ptr, size }
    }

    #[inline]
    pub fn new_total_size(ptr: StablePtr, total_size: u64) -> Self {
        Self::new(ptr, total_size - (StablePtr::SIZE * 2) as u64)
    }

    #[inline]
    pub fn to_allocated(self) -> SSlice {
        SSlice::new(self.as_ptr(), self.get_size_bytes(), true)
    }

    // when side == Side::End, ptr is the "rear" ptr - the one at the end
    pub fn from_ptr(ptr: StablePtr) -> Option<Self> {
        let size = Self::read_size(ptr)?;

        Some(Self::new(ptr, size))
    }

    pub fn from_rear_ptr(ptr: StablePtr) -> Option<Self> {
        let size = Self::read_size(ptr)?;

        let it_ptr = ptr - (StablePtr::SIZE as u64) - size;
        let it = Self::new(it_ptr, size);

        Some(it)
    }

    #[inline]
    pub(crate) fn persist(&mut self) {
        Self::write_size(self.as_ptr(), self.get_size_bytes());
    }

    #[inline]
    pub fn get_next_neighbor_ptr(&self) -> StablePtr {
        self.as_ptr() + (StablePtr::SIZE * 2) as u64 + self.get_size_bytes()
    }

    #[inline]
    pub fn get_prev_neighbor_rear_ptr(&self) -> StablePtr {
        self.as_ptr() - StablePtr::SIZE as u64
    }

    #[inline]
    pub fn next_neighbor_is_free(&self, max_ptr: StablePtr) -> Option<FreeBlock> {
        let next_neighbor_ptr = self.get_next_neighbor_ptr();

        if next_neighbor_ptr < max_ptr {
            Self::read_size(next_neighbor_ptr).map(|size| Self::new(next_neighbor_ptr, size))
        } else {
            None
        }
    }

    #[inline]
    pub fn prev_neighbor_is_free(&self) -> Option<FreeBlock> {
        let prev_neighbor_rear_ptr = self.get_prev_neighbor_rear_ptr();

        if prev_neighbor_rear_ptr >= MIN_PTR {
            Self::read_size(prev_neighbor_rear_ptr).map(|size| {
                let it_ptr = prev_neighbor_rear_ptr - (StablePtr::SIZE as u64) - size;

                Self::new(it_ptr, size)
            })
        } else {
            None
        }
    }

    #[inline]
    pub fn merged_size(a: &Self, b: &Self) -> u64 {
        a.get_size_bytes() + b.get_size_bytes() + (StablePtr::SIZE * 2) as u64
    }

    /// returns transient free block
    #[inline]
    pub fn merge(a: Self, b: Self) -> Self {
        debug_assert!(a.as_ptr() < b.as_ptr());
        debug_assert_eq!(a.as_ptr() + a.get_total_size_bytes(), b.as_ptr());

        FreeBlock::new(a.as_ptr(), Self::merged_size(&a, &b))
    }

    /// returns transient free blocks
    #[inline]
    pub fn split(self, size_first: u64) -> (Self, Self) {
        debug_assert!(Self::can_split(self.get_size_bytes(), size_first));

        let a = FreeBlock::new(self.as_ptr(), size_first);
        let b = FreeBlock::new(
            a.get_next_neighbor_ptr(),
            self.get_total_size_bytes() - size_first - (StablePtr::SIZE * 4) as u64,
        );

        (a, b)
    }

    #[inline]
    pub fn can_split(self_size: u64, size_first: u64) -> bool {
        (Self::to_total_size(self_size) > size_first + (StablePtr::SIZE * 4) as u64)
            && (Self::to_total_size(self_size) - size_first - (StablePtr::SIZE * 4) as u64
                >= (StablePtr::SIZE * 2) as u64)
    }

    #[inline]
    pub fn to_total_size(size: u64) -> u64 {
        size + (StablePtr::SIZE * 2) as u64
    }

    #[inline]
    pub fn get_total_size_bytes(&self) -> u64 {
        Self::to_total_size(self.size)
    }

    #[inline]
    pub fn get_size_bytes(&self) -> u64 {
        self.size
    }

    #[inline]
    pub fn as_ptr(&self) -> StablePtr {
        self.ptr
    }

    #[inline]
    pub fn as_rear_ptr(&self) -> StablePtr {
        self.as_ptr() + self.get_size_bytes() + (StablePtr::SIZE) as StablePtr
    }

    pub fn debug_validate(&self) {
        let size_1 = Self::read_size(self.as_ptr()).unwrap();
        let size_2 = Self::read_size(self.as_rear_ptr()).unwrap();

        assert_eq!(size_1, size_2);
        assert_eq!(size_1, self.size);
    }

    fn read_size(ptr: StablePtr) -> Option<u64> {
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
            Some(size)
        }
    }

    fn write_size(ptr: StablePtr, size: u64) {
        let encoded_size = size & FREE;

        let meta = encoded_size.to_le_bytes();

        stable::write(ptr, &meta);
        stable::write(ptr + (StablePtr::SIZE as u64) + size, &meta);
    }
}

impl Ord for FreeBlock {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.ptr.cmp(&other.ptr)
    }
}

impl PartialOrd for FreeBlock {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.ptr.partial_cmp(&other.ptr)
    }
}

impl Eq for FreeBlock {}

impl PartialEq for FreeBlock {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.ptr.eq(&other.ptr)
    }
}

#[cfg(test)]
mod tests {
    use crate::mem::allocator::MIN_PTR;
    use crate::mem::free_block::FreeBlock;
    use crate::utils::mem_context::stable;

    #[test]
    fn basic_flow_works_fine() {
        stable::clear();
        stable::grow(1).expect("Unable to grow");

        let mut m1 = FreeBlock::new(MIN_PTR, 100);
        m1.persist();

        assert!(m1.prev_neighbor_is_free().is_none());
        assert!(m1
            .next_neighbor_is_free(m1.get_next_neighbor_ptr())
            .is_none());

        m1 = FreeBlock::from_rear_ptr(m1.get_total_size_bytes() as u64).unwrap();

        assert_eq!(m1.get_total_size_bytes(), 116);

        assert_eq!(m1.get_prev_neighbor_rear_ptr(), 0);
        assert_eq!(m1.get_next_neighbor_ptr(), 124);

        let mut m2 = FreeBlock::new(124, 100);
        m2.persist();

        assert_eq!(m2.get_total_size_bytes(), 116);

        assert_eq!(m2.get_prev_neighbor_rear_ptr(), 116);
        assert_eq!(m2.get_next_neighbor_ptr(), 240);

        assert!(m1.prev_neighbor_is_free().is_none());
        let m1_next = m1
            .next_neighbor_is_free(m2.get_next_neighbor_ptr())
            .unwrap();
        assert_eq!(m1_next.as_ptr(), m2.as_ptr());
        assert_eq!(m1_next.get_size_bytes(), m2.get_size_bytes());

        assert!(m2
            .next_neighbor_is_free(m2.get_next_neighbor_ptr())
            .is_none());
        let m2_prev = m2.prev_neighbor_is_free().unwrap();
        assert_eq!(m2_prev.as_ptr(), m1.as_ptr());
        assert_eq!(m2_prev.get_size_bytes(), m1.get_size_bytes());

        // join
        let mut m3 = FreeBlock::merge(m1, m2);
        m3.persist();

        assert_eq!(m3.get_prev_neighbor_rear_ptr(), 0);
        assert_eq!(m3.get_next_neighbor_ptr(), 240);

        assert_eq!(m3.get_size_bytes(), 216);
        assert_eq!(m3.get_total_size_bytes(), 232);

        // split
        (m1, m2) = m3.split(50);
        m1.persist();
        m2.persist();

        assert_eq!(m1.get_prev_neighbor_rear_ptr(), 0);
        assert_eq!(m1.get_next_neighbor_ptr(), 74);
        assert_eq!(m1.get_size_bytes(), 50);
        assert_eq!(m1.get_total_size_bytes(), 66);

        assert_eq!(m2.as_ptr(), 74);
        assert_eq!(m2.get_prev_neighbor_rear_ptr(), 66);
        assert_eq!(m2.get_next_neighbor_ptr(), 240);
        assert_eq!(m2.get_size_bytes(), 150);
        assert_eq!(m2.get_total_size_bytes(), 166);

        assert!(m1.prev_neighbor_is_free().is_none());
        let m1_next = m1
            .next_neighbor_is_free(m2.get_next_neighbor_ptr())
            .unwrap();
        assert_eq!(m1_next.as_ptr(), m2.as_ptr());
        assert_eq!(m1_next.get_size_bytes(), m2.get_size_bytes());

        assert!(m2
            .next_neighbor_is_free(m2.get_next_neighbor_ptr())
            .is_none());
        let m2_prev = m2.prev_neighbor_is_free().unwrap();
        assert_eq!(m2_prev.as_ptr(), m1.as_ptr());
        assert_eq!(m2_prev.get_size_bytes(), m1.get_size_bytes());
    }
}
