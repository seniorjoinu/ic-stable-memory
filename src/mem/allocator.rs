use crate::encoding::{AsFixedSizeBytes, Buffer};
use crate::mem::free_block::FreeBlock;
use crate::mem::s_slice::{Side, BLOCK_META_SIZE, BLOCK_MIN_TOTAL_SIZE};
use crate::mem::StablePtr;
use crate::utils::math::fast_log2;
use crate::utils::mem_context::{stable, OutOfMemory, PAGE_SIZE_BYTES};
use crate::SSlice;
use std::fmt::Debug;
use std::usize;

pub(crate) const EMPTY_PTR: u64 = u64::MAX;
pub(crate) const SEG_CLASS_PTRS_COUNT: u32 = usize::BITS - 4;
pub(crate) const CUSTOM_DATA_PTRS_COUNT: usize = 4;

#[derive(Debug)]
pub(crate) struct StableMemoryAllocator {
    // serialized
    seg_class_heads: [Option<FreeBlock>; SEG_CLASS_PTRS_COUNT as usize],
    seg_class_tails: [Option<FreeBlock>; SEG_CLASS_PTRS_COUNT as usize],
    free_size: u64,
    allocated_size: u64,
    custom_data_ptrs: [u64; CUSTOM_DATA_PTRS_COUNT],
}

pub const MIN_PTR: u64 = u64::SIZE as u64;
pub fn MAX_PTR() -> u64 {
    stable::size_pages() * PAGE_SIZE_BYTES as u64
}

impl StableMemoryAllocator {
    #[inline]
    fn new() -> Self {
        Self {
            seg_class_heads: [None; SEG_CLASS_PTRS_COUNT as usize],
            seg_class_tails: [None; SEG_CLASS_PTRS_COUNT as usize],
            free_size: 0,
            allocated_size: 0,
            custom_data_ptrs: [EMPTY_PTR; CUSTOM_DATA_PTRS_COUNT],
        }
    }

    /// # Safety
    /// Invoke only once during `init()` canister function execution
    /// Execution more than once will lead to undefined behavior.
    ///
    /// Don't call on canisters, that have more than 4GBs of stable memory! The canister is gonna crash,
    /// and all data will be lost.
    pub(crate) unsafe fn init() -> Self {
        let mut allocator = Self::new();

        // max_ptr is mutliple of 8, because of page size
        let max_ptr = stable::size_pages() * PAGE_SIZE_BYTES as u64;

        assert!(
            max_ptr <= u32::MAX as u64,
            "To much stable memory already allocated, can't build free blocks"
        );

        if max_ptr > MIN_PTR {
            // TODO: allow creation of free-blocks and slices bigger than 2**32
            let free_block = FreeBlock::new_total_size(MIN_PTR, (max_ptr - MIN_PTR) as usize);
            allocator.push_free_block(free_block, false);
        }

        allocator
    }

    pub(crate) fn store(&mut self) {
        let slice = self.allocate(Self::SIZE);
        let mut buf = <Self as AsFixedSizeBytes>::Buf::new(Self::SIZE);
        self.as_fixed_size_bytes(buf._deref_mut());

        println!("before {} {}", slice.get_total_size_bytes(), slice.as_ptr());

        unsafe { crate::mem::write_bytes(slice.make_ptr_by_offset(0), buf._deref()) };
        unsafe { crate::mem::write_and_own_fixed(0, &mut slice.as_ptr()) };
    }

    pub(crate) unsafe fn retrieve() -> Self {
        let slice_ptr = unsafe { crate::mem::read_and_disown_fixed(0) };
        let slice = SSlice::from_ptr(slice_ptr, Side::Start).unwrap();

        println!("after {} {}", slice.get_total_size_bytes(), slice_ptr);

        let mut buf = <Self as AsFixedSizeBytes>::Buf::new(Self::SIZE);
        unsafe { crate::mem::read_bytes(slice.make_ptr_by_offset(0), buf._deref_mut()) };

        let mut it = Self::from_fixed_size_bytes(buf._deref());

        it.deallocate(slice);

        it
    }

    // TODO: allocator simply drains memory wtf

    pub(crate) fn allocate(&mut self, size: usize) -> SSlice {
        let size = Self::pad_size(size);

        let free_membox = match self.pop_free_block(size) {
            Ok(m) => m,
            Err(_) => panic!("Not enough stable memory to allocate {} more bytes. Grown: {} bytes; Allocated: {} bytes; Free: {} bytes", size, stable::size_pages() * PAGE_SIZE_BYTES as u64, self.get_allocated_size(), self.get_free_size())
        };

        free_membox.to_allocated()
    }

    pub(crate) fn deallocate(&mut self, slice: SSlice) {
        let free_block = slice.to_free_block();
        let total_allocated = self.get_allocated_size();

        self.set_allocated_size(total_allocated - free_block.get_total_size_bytes() as u64);

        self.push_free_block(free_block, true);
    }

    pub(crate) fn reallocate(&mut self, slice: SSlice, new_size: usize) -> Result<SSlice, SSlice> {
        match self.try_reallocate_inplace(slice, new_size) {
            Ok(s) => Ok(s),
            Err(slice) => {
                let mut data = vec![0u8; slice.get_size_bytes()];
                unsafe { crate::mem::read_bytes(slice.make_ptr_by_offset(0), &mut data) };

                self.deallocate(slice);
                let new_slice = self.allocate(new_size);
                unsafe { crate::mem::write_bytes(new_slice.make_ptr_by_offset(0), &data) };

                Err(new_slice)
            }
        }
    }

    pub(crate) fn try_reallocate_inplace(
        &mut self,
        slice: SSlice,
        new_size: usize,
    ) -> Result<SSlice, SSlice> {
        let free_block = FreeBlock::new(slice.as_ptr(), slice.get_size_bytes(), true);

        let next_neighbor_free_size_1_opt =
            free_block.check_neighbor_is_also_free(Side::End, MIN_PTR, MAX_PTR());

        if let Some(next_neighbor_free_size_1) = next_neighbor_free_size_1_opt {
            if let Some(next_neighbor) = FreeBlock::from_ptr(
                free_block.get_next_neighbor_ptr(),
                Side::Start,
                Some(next_neighbor_free_size_1),
            ) {
                if next_neighbor.validate().is_some() {
                    let seg_class_id = get_seg_class_id(next_neighbor.size);
                    let target_size = free_block.size + next_neighbor.size + BLOCK_META_SIZE * 2;

                    if target_size >= new_size && target_size < new_size + BLOCK_MIN_TOTAL_SIZE {
                        self.eject_from_freelist(seg_class_id, &next_neighbor);

                        let total_allocated = self.get_allocated_size();
                        self.set_allocated_size(
                            total_allocated + free_block.get_total_size_bytes() as u64,
                        );

                        let new_block = FreeBlock::new(free_block.ptr, target_size, true);

                        return Ok(new_block.to_allocated());
                    }

                    if target_size >= new_size + BLOCK_MIN_TOTAL_SIZE {
                        self.eject_from_freelist(seg_class_id, &next_neighbor);

                        let block_1 = FreeBlock::new(free_block.ptr, new_size, true);
                        let block_2 = FreeBlock::new_total_size(
                            block_1.get_next_neighbor_ptr(),
                            target_size - new_size,
                        );

                        self.push_free_block(block_2, false);

                        let total_allocated = self.get_allocated_size();
                        self.set_allocated_size(
                            total_allocated + block_1.get_total_size_bytes() as u64,
                        );

                        return Ok(block_1.to_allocated());
                    }

                    return Err(slice);
                }

                return Err(slice);
            }

            return Err(slice);
        }

        Err(slice)
    }

    fn push_free_block(&mut self, mut free_block: FreeBlock, try_merge: bool) {
        if try_merge {
            free_block = self.maybe_merge_with_free_neighbors(free_block);
        }

        free_block.persist();

        let total_free = self.get_free_size();
        self.set_free_size(total_free + free_block.get_total_size_bytes() as u64);

        let seg_class_id = get_seg_class_id(free_block.size);

        if self.seg_class_heads[seg_class_id].is_none() {
            self.set_seg_class_head(seg_class_id, Some(free_block));
            self.set_seg_class_tail(seg_class_id, Some(free_block));

            FreeBlock::set_free_ptrs(free_block.ptr, EMPTY_PTR, EMPTY_PTR);
        } else {
            let tail = self.seg_class_tails[seg_class_id].unwrap();

            self.set_seg_class_tail(seg_class_id, Some(free_block));

            FreeBlock::set_next_free_ptr(tail.ptr, free_block.ptr);
            FreeBlock::set_free_ptrs(free_block.ptr, tail.ptr, EMPTY_PTR);
        }
    }

    fn pop_free_block(&mut self, size: usize) -> Result<FreeBlock, OutOfMemory> {
        let mut seg_class_id = get_seg_class_id(size);
        let mut free_block_opt = self.get_seg_class_head(seg_class_id);

        while seg_class_id < SEG_CLASS_PTRS_COUNT as usize {
            if let Some(free_block) = free_block_opt {
                if free_block.size >= size && free_block.size < size + BLOCK_MIN_TOTAL_SIZE {
                    self.eject_from_freelist(seg_class_id, &free_block);

                    let total_allocated = self.get_allocated_size();
                    self.set_allocated_size(
                        total_allocated + free_block.get_total_size_bytes() as u64,
                    );

                    return Ok(free_block);
                }

                if free_block.size >= size + BLOCK_MIN_TOTAL_SIZE {
                    self.eject_from_freelist(seg_class_id, &free_block);

                    let block_1 = FreeBlock::new(free_block.ptr, size, true);
                    let block_2 = FreeBlock::new_total_size(
                        block_1.get_next_neighbor_ptr(),
                        free_block.size - size,
                    );

                    self.push_free_block(block_2, false);

                    let total_allocated = self.get_allocated_size();
                    self.set_allocated_size(
                        total_allocated + block_1.get_total_size_bytes() as u64,
                    );

                    return Ok(block_1);
                }

                let next_ptr = FreeBlock::get_next_free_ptr(free_block.ptr);
                if next_ptr != EMPTY_PTR {
                    free_block_opt = FreeBlock::from_ptr(next_ptr, Side::Start, None);
                } else {
                    seg_class_id += 1;

                    if seg_class_id < SEG_CLASS_PTRS_COUNT as usize {
                        free_block_opt = self.get_seg_class_head(seg_class_id);
                    } else {
                        free_block_opt = None;
                    }
                }
            } else {
                seg_class_id += 1;

                if seg_class_id < SEG_CLASS_PTRS_COUNT as usize {
                    free_block_opt = self.get_seg_class_head(seg_class_id);
                } else {
                    free_block_opt = None;
                }
            }
        }

        let mut pages_to_grow = ((size + BLOCK_META_SIZE * 2) / PAGE_SIZE_BYTES) as u64;
        if (size + BLOCK_META_SIZE * 2) % PAGE_SIZE_BYTES != 0 {
            pages_to_grow += 1;
        }

        // TODO: remove in favor of free-buffer
        match stable::grow(pages_to_grow) {
            Ok(prev_pages) => {
                let ptr = prev_pages * PAGE_SIZE_BYTES as u64;
                let free_block =
                    FreeBlock::new_total_size(ptr, pages_to_grow as usize * PAGE_SIZE_BYTES);

                if free_block.size >= size && free_block.size < size + BLOCK_MIN_TOTAL_SIZE {
                    let new_size =
                        self.get_allocated_size() + free_block.get_total_size_bytes() as u64;
                    self.set_allocated_size(new_size);

                    return Ok(free_block);
                }

                if free_block.size >= size + BLOCK_MIN_TOTAL_SIZE {
                    let block_1 = FreeBlock::new(free_block.ptr, size, true);
                    let block_2 = FreeBlock::new_total_size(
                        block_1.get_next_neighbor_ptr(),
                        free_block.size - size,
                    );

                    self.push_free_block(block_2, false);

                    let total_allocated = self.get_allocated_size();
                    self.set_allocated_size(
                        total_allocated + block_1.get_total_size_bytes() as u64,
                    );

                    return Ok(block_1);
                }

                unreachable!();
            }
            _ => Err(OutOfMemory),
        }
    }

    fn eject_from_freelist(&mut self, seg_class_id: usize, free_block: &FreeBlock) {
        // if block is the head of it's segregation class
        if self.seg_class_heads[seg_class_id].unwrap().ptr == free_block.ptr {
            // if it is also the tail
            if self.seg_class_tails[seg_class_id].unwrap().ptr == free_block.ptr {
                self.set_seg_class_head(seg_class_id, None);
                self.set_seg_class_tail(seg_class_id, None);
            } else {
                // there should be next
                let next_free_block_ptr = FreeBlock::get_next_free_ptr(free_block.ptr);
                let new_head = FreeBlock::from_ptr(next_free_block_ptr, Side::Start, None);

                // next is the head now
                self.set_seg_class_head(seg_class_id, new_head);
                FreeBlock::set_prev_free_ptr(next_free_block_ptr, EMPTY_PTR);
            }

            // if block is the tail of it's class, but not the head
        } else if self.seg_class_tails[seg_class_id].unwrap().ptr == free_block.ptr {
            // there should be prev
            let prev_ptr = FreeBlock::get_prev_free_ptr(free_block.ptr);
            let new_tail = FreeBlock::from_ptr(prev_ptr, Side::Start, None);

            self.set_seg_class_tail(seg_class_id, new_tail);
            FreeBlock::set_next_free_ptr(prev_ptr, EMPTY_PTR);

            // if the block is somewhere in between
        } else {
            // it should have both: prev and next
            let prev_ptr = FreeBlock::get_prev_free_ptr(free_block.ptr);
            let next_ptr = FreeBlock::get_next_free_ptr(free_block.ptr);

            // just link together next and prev
            FreeBlock::set_next_free_ptr(prev_ptr, next_ptr);
            FreeBlock::set_prev_free_ptr(next_ptr, prev_ptr);
        }

        let total_free = self.get_free_size();
        self.set_free_size(total_free - free_block.get_total_size_bytes() as u64);
    }

    fn maybe_merge_with_free_neighbors(&mut self, mut free_block: FreeBlock) -> FreeBlock {
        let prev_neighbor_ptr = free_block.get_prev_neighbor_ptr();
        let next_neighbor_ptr = free_block.get_next_neighbor_ptr();

        let prev_neighbor_free_size_1_opt =
            free_block.check_neighbor_is_also_free(Side::Start, MIN_PTR, MAX_PTR());

        let next_neighbor_free_size_1_opt =
            free_block.check_neighbor_is_also_free(Side::End, MIN_PTR, MAX_PTR());

        free_block = if let Some(prev_neighbor_free_size_1) = prev_neighbor_free_size_1_opt {
            let size = Some(prev_neighbor_free_size_1);

            if let Some(prev_neighbor) = FreeBlock::from_ptr(prev_neighbor_ptr, Side::End, size) {
                if prev_neighbor.validate().is_some() {
                    let seg_class_id = get_seg_class_id(prev_neighbor.size);
                    self.eject_from_freelist(seg_class_id, &prev_neighbor);
                    let size = prev_neighbor.size + free_block.size + BLOCK_META_SIZE * 2;

                    FreeBlock::new(prev_neighbor.ptr, size, true)
                } else {
                    free_block
                }
            } else {
                free_block
            }
        } else {
            free_block
        };

        free_block = if let Some(next_neighbor_free_size_1) = next_neighbor_free_size_1_opt {
            let size = Some(next_neighbor_free_size_1);

            if let Some(next_neighbor) = FreeBlock::from_ptr(next_neighbor_ptr, Side::Start, size) {
                if next_neighbor.validate().is_some() {
                    let seg_class_id = get_seg_class_id(next_neighbor.size);
                    self.eject_from_freelist(seg_class_id, &next_neighbor);

                    let size = next_neighbor.size + free_block.size + BLOCK_META_SIZE * 2;
                    FreeBlock::new(free_block.ptr, size, true)
                } else {
                    free_block
                }
            } else {
                free_block
            }
        } else {
            free_block
        };

        free_block
    }

    fn get_seg_class_head(&self, id: usize) -> Option<FreeBlock> {
        self.seg_class_heads[id]
    }

    fn set_seg_class_head(&mut self, id: usize, new_head: Option<FreeBlock>) {
        self.seg_class_heads[id] = new_head;
    }

    fn set_seg_class_tail(&mut self, id: usize, new_tail: Option<FreeBlock>) {
        self.seg_class_tails[id] = new_tail;
    }

    pub(crate) fn get_allocated_size(&self) -> u64 {
        self.allocated_size
    }

    fn set_allocated_size(&mut self, size: u64) {
        self.allocated_size = size;
    }

    pub(crate) fn get_free_size(&self) -> u64 {
        self.free_size
    }

    fn set_free_size(&mut self, size: u64) {
        self.free_size = size;
    }

    pub fn set_custom_data_ptr(&mut self, idx: usize, ptr: u64) {
        self.custom_data_ptrs[idx] = ptr;
    }

    pub fn get_custom_data_ptr(&self, idx: usize) -> u64 {
        self.custom_data_ptrs[idx]
    }

    /// If the size is less than 32 bytes - return 32, since this is the size that is needed in order
    /// to store two pointers (which is required for this allocator).
    /// Otherwise, simply round up to the closest multiple of 8
    fn pad_size(size: usize) -> usize {
        if size < BLOCK_MIN_TOTAL_SIZE {
            return BLOCK_MIN_TOTAL_SIZE;
        }

        (size + 7) & !7
    }
}

impl AsFixedSizeBytes for StableMemoryAllocator {
    const SIZE: usize =
        u64::SIZE * (SEG_CLASS_PTRS_COUNT as usize * 2 + 2 + CUSTOM_DATA_PTRS_COUNT);
    type Buf = [u8; Self::SIZE];

    fn from_fixed_size_bytes(buf: &[u8]) -> Self {
        let mut from = 0;
        let mut to = 0;

        let mut heads = [None; SEG_CLASS_PTRS_COUNT as usize];

        for i in 0..SEG_CLASS_PTRS_COUNT as usize {
            from = to;
            to += u64::SIZE;

            let ptr = u64::from_fixed_size_bytes(&buf[from..to]);
            if ptr == EMPTY_PTR {
                continue;
            }

            // unwrapping in order to check for errors
            let block = FreeBlock::from_ptr(ptr, Side::Start, None).unwrap();

            heads[i] = Some(block);
        }

        let mut tails = [None; SEG_CLASS_PTRS_COUNT as usize];

        for i in 0..SEG_CLASS_PTRS_COUNT as usize {
            from = to;
            to += u64::SIZE;

            let ptr = u64::from_fixed_size_bytes(&buf[from..to]);
            if ptr == EMPTY_PTR {
                continue;
            }

            // unwrapping in order to check for errors
            let block = FreeBlock::from_ptr(ptr, Side::Start, None).unwrap();

            assert!(heads[i].is_some(), "Invalid free blocks");

            tails[i] = Some(block);
        }

        from = to;
        to += u64::SIZE;

        let free_size = u64::from_fixed_size_bytes(&buf[from..to]);

        from = to;
        to += u64::SIZE;

        let allocated_size = u64::from_fixed_size_bytes(&buf[from..to]);

        let mut custom_ptrs = [0u64; CUSTOM_DATA_PTRS_COUNT];

        for i in 0..CUSTOM_DATA_PTRS_COUNT {
            from = to;
            to += u64::SIZE;

            custom_ptrs[i] = u64::from_fixed_size_bytes(&buf[from..to]);
        }

        Self {
            seg_class_heads: heads,
            seg_class_tails: tails,
            free_size,
            allocated_size,
            custom_data_ptrs: custom_ptrs,
        }
    }

    fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
        let mut from = 0;
        let mut to = 0;

        // heads
        for i in 0..SEG_CLASS_PTRS_COUNT as usize {
            let ptr = if let Some(free_block) = self.seg_class_heads[i] {
                free_block.ptr
            } else {
                EMPTY_PTR
            };

            from = to;
            to += u64::SIZE;

            ptr.as_fixed_size_bytes(&mut buf[from..to]);
        }

        // tails
        for i in 0..SEG_CLASS_PTRS_COUNT as usize {
            let ptr = if let Some(free_block) = self.seg_class_tails[i] {
                free_block.ptr
            } else {
                EMPTY_PTR
            };

            from = to;
            to += u64::SIZE;

            ptr.as_fixed_size_bytes(&mut buf[from..to]);
        }

        from = to;
        to += u64::SIZE;

        self.free_size.as_fixed_size_bytes(&mut buf[from..to]);

        from = to;
        to += u64::SIZE;

        self.allocated_size.as_fixed_size_bytes(&mut buf[from..to]);

        for i in 0..CUSTOM_DATA_PTRS_COUNT {
            let ptr = self.custom_data_ptrs[i];

            from = to;
            to += u64::SIZE;

            ptr.as_fixed_size_bytes(&mut buf[from..to]);
        }
    }
}

fn get_seg_class_id(size: usize) -> usize {
    let mut log = fast_log2(size);

    if 2usize.pow(log) < size {
        log += 1;
    }

    if log > 3 {
        (log - 4) as usize
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use crate::mem::allocator::SEG_CLASS_PTRS_COUNT;
    use crate::utils::mem_context::stable;
    use crate::utils::Anyway;
    use crate::{deallocate, isoprint, StableMemoryAllocator};
    use rand::seq::SliceRandom;
    use rand::thread_rng;

    #[test]
    fn initialization_growing_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();

        unsafe {
            let mut sma = StableMemoryAllocator::init();
            let free_memboxes: Vec<_> = (0..SEG_CLASS_PTRS_COUNT as usize)
                .filter_map(|it| sma.get_seg_class_head(it))
                .collect();

            assert_eq!(free_memboxes.len(), 1);
            let free_block_1 = free_memboxes[0];

            let mut buf_1 = [0u8; 100];
            stable::read(0, &mut buf_1);

            sma.store();

            let mut buf_2 = [0u8; 100];
            stable::read(0, &mut buf_2);

            println!("{:?}, {:?}", buf_1, buf_2);

            let sma = StableMemoryAllocator::retrieve();
            let free_blocks: Vec<_> = (0..SEG_CLASS_PTRS_COUNT as usize)
                .filter_map(|it| sma.get_seg_class_head(it))
                .collect();

            assert_eq!(free_blocks.len(), 1);
            let free_block_2 = free_blocks[0];

            assert_eq!(free_block_1.size, free_block_2.size);
        }
    }

    /// [0, 0, 0, 0, 0, 0, 0, 0, 232, 255, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    /// [8, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255]

    #[test]
    fn initialization_not_growing_works_fine() {
        stable::clear();

        unsafe {
            let mut sma = StableMemoryAllocator::init();
            let free_memboxes: Vec<_> = (0..SEG_CLASS_PTRS_COUNT as usize)
                .filter_map(|it| sma.get_seg_class_head(it))
                .collect();

            assert_eq!(free_memboxes.len(), 0);

            sma.store();

            let sma = StableMemoryAllocator::retrieve();
            let free_blocks: Vec<_> = (0..SEG_CLASS_PTRS_COUNT as usize)
                .filter_map(|it| sma.get_seg_class_head(it))
                .collect();

            assert_eq!(free_blocks.len(), 0);
        }
    }

    #[test]
    fn allocation_works_fine() {
        stable::clear();

        unsafe {
            let mut sma = StableMemoryAllocator::init();

            let mut slices = vec![];

            // try to allocate 1000 MB
            for i in 0..1024 {
                let slice = sma.allocate(1024);

                assert!(
                    slice.get_size_bytes() >= 1024,
                    "Invalid membox size at {}",
                    i
                );

                slices.push(slice);
            }

            assert!(sma.get_allocated_size() >= 1024 * 1024);

            for i in 0..1024 {
                let mut slice = slices[i];
                slice = sma.reallocate(slice, 2 * 1024).anyway();

                assert!(
                    slice.get_size_bytes() >= 2 * 1024,
                    "Invalid membox size at {}",
                    i
                );

                slices[i] = slice;
            }

            assert!(sma.get_allocated_size() >= 2 * 1024 * 1024);

            for i in 0..1024 {
                let slice = slices[i];
                sma.deallocate(slice);
            }

            assert_eq!(sma.get_allocated_size(), 0);
        }
    }

    #[test]
    fn basic_flow_works_fine() {
        unsafe {
            stable::clear();

            let mut allocator = StableMemoryAllocator::init();
            allocator.store();

            let mut allocator = StableMemoryAllocator::retrieve();

            let slice1 = allocator.allocate(100);
            let slice1 = allocator.reallocate(slice1, 200).anyway();

            let slice2 = allocator.allocate(100);
            let slice3 = allocator.allocate(100);

            allocator.deallocate(slice1);
            let slice2 = allocator.reallocate(slice2, 200).anyway();

            allocator.store();

            let allocator = StableMemoryAllocator::retrieve();

            let mut allocator = StableMemoryAllocator::retrieve();

            let mut slices = Vec::new();
            for _ in 0..5000 {
                slices.push(allocator.allocate(100));
            }

            slices.shuffle(&mut thread_rng());

            for slice in slices {
                allocator.deallocate(slice);
            }
        }
    }
}
