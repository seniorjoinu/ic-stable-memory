use crate::encoding::dyn_size::candid_decode_one_allow_trailing;
use crate::encoding::{AsDynSizeBytes, AsFixedSizeBytes, Buffer};
use crate::mem::free_block::FreeBlock;
use crate::mem::s_slice::SSlice;
use crate::mem::StablePtr;
use crate::utils::math::ceil_div;
use crate::{stable, PAGE_SIZE_BYTES};
use candid::{encode_one, CandidType, Deserialize};
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::Write;

pub const ALLOCATOR_PTR: StablePtr = 0;
pub const MIN_PTR: StablePtr = u64::SIZE as u64;
pub const EMPTY_PTR: StablePtr = u64::MAX;

#[derive(Debug, CandidType, Deserialize, Eq, PartialEq)]
pub struct StableMemoryAllocator {
    free_blocks: BTreeMap<usize, Vec<FreeBlock>>,
    custom_data_pointers: HashMap<usize, StablePtr>,
    free_size: u64,
    available_size: u64,
    max_ptr: StablePtr,
}

impl StableMemoryAllocator {
    pub fn init() -> Self {
        let mut it = Self {
            max_ptr: MIN_PTR,
            free_blocks: BTreeMap::default(),
            custom_data_pointers: HashMap::default(),
            free_size: 0,
            available_size: 0,
        };

        let real_max_ptr = stable::size_pages() * PAGE_SIZE_BYTES as u64;
        if real_max_ptr > it.max_ptr {
            let free_block =
                FreeBlock::new_total_size(it.max_ptr, (real_max_ptr - it.max_ptr) as usize);
            it.more_free_size(free_block.get_total_size_bytes());
            it.more_available_size(free_block.get_total_size_bytes());

            it.push_free_block(free_block);
        }

        it
    }

    pub fn allocate(&mut self, mut size: usize) -> SSlice {
        size = Self::pad_size(size);

        // searching for a free block that is equal or bigger in size, than asked
        let mut free_block = if let Some(fb) = self.pop_free_block(size) {
            fb
        } else {
            let fb = self.grow(size);

            self.more_available_size(fb.get_total_size_bytes());
            self.more_free_size(fb.get_total_size_bytes());

            fb
        };

        // if it is bigger - try splitting it in two, taking the first half
        if FreeBlock::can_split(free_block.get_size_bytes(), size) {
            let (a, b) = free_block.split(size);
            self.push_free_block(b);

            free_block = a
        }

        self.less_free_size(free_block.get_total_size_bytes());

        free_block.to_allocated()
    }

    #[inline]
    pub fn deallocate(&mut self, slice: SSlice) {
        let free_block = slice.to_free_block();

        self.more_free_size(free_block.get_total_size_bytes());

        let free_block = self.try_merge_with_neighbors(free_block);
        self.push_free_block(free_block);
    }

    pub fn reallocate(&mut self, slice: SSlice, mut new_size: usize) -> Result<SSlice, SSlice> {
        new_size = Self::pad_size(new_size);

        if new_size <= slice.get_size_bytes() {
            return Ok(slice);
        }

        let free_block = slice.to_free_block();

        // if it is possible to simply "grow" the slice, by merging it with the next neighbor - do that
        let free_block = match self.try_reallocate_in_place(free_block, new_size) {
            Ok(fb) => return Ok(fb.to_allocated()),
            Err(fb) => fb,
        };

        // othewise, get ready for move and copy the data
        let mut b = vec![0u8; slice.get_size_bytes()];
        unsafe { crate::mem::read_bytes(slice.make_ptr_by_offset(0), &mut b) };

        // deallocate the slice
        self.more_free_size(free_block.get_total_size_bytes());

        let free_block = self.try_merge_with_neighbors(free_block);
        self.push_free_block(free_block);

        // allocate a new one
        let new_slice = self.allocate(new_size);

        // put the data back
        unsafe { crate::mem::write_bytes(new_slice.make_ptr_by_offset(0), &b) };

        Ok(new_slice)
    }

    pub fn store(&mut self) {
        // first encode is simply to calculate the required size
        let buf = self.as_dyn_size_bytes();

        // reserving 100 extra bytes in order for the allocator to grow while allocating memory for itself
        let slice = self.allocate(buf.len() + 100);

        let buf = self.as_dyn_size_bytes();

        unsafe { crate::mem::write_bytes(slice.make_ptr_by_offset(0), &buf) };
        unsafe { crate::mem::write_and_own_fixed(0, &mut slice.as_ptr()) };
    }

    pub fn retrieve() -> Self {
        let slice_ptr = unsafe { crate::mem::read_and_disown_fixed(0) };
        let slice = SSlice::from_ptr(slice_ptr).unwrap();

        let mut buf = vec![0u8; slice.get_size_bytes()];
        unsafe { crate::mem::read_bytes(slice.make_ptr_by_offset(0), &mut buf) };

        let mut it = Self::from_dyn_size_bytes(&buf);
        it.deallocate(slice);

        it
    }

    #[inline]
    pub fn get_allocated_size(&self) -> u64 {
        self.available_size - self.free_size
    }

    #[inline]
    pub fn get_available_size(&self) -> u64 {
        self.available_size
    }

    #[inline]
    pub fn get_free_size(&self) -> u64 {
        self.free_size
    }

    #[inline]
    fn more_available_size(&mut self, additional: usize) {
        self.available_size += additional as u64;
    }

    #[inline]
    fn more_free_size(&mut self, additional: usize) {
        self.free_size += additional as u64;
    }

    #[inline]
    fn less_free_size(&mut self, additional: usize) {
        self.free_size -= additional as u64;
    }

    #[inline]
    pub fn set_custom_data_ptr(&mut self, idx: usize, ptr: StablePtr) -> Option<StablePtr> {
        self.custom_data_pointers.insert(idx, ptr)
    }

    #[inline]
    pub fn get_custom_data_ptr(&self, idx: usize) -> Option<StablePtr> {
        self.custom_data_pointers.get(&idx).cloned()
    }

    fn try_reallocate_in_place(
        &mut self,
        mut free_block: FreeBlock,
        new_size: usize,
    ) -> Result<FreeBlock, FreeBlock> {
        if let Some(mut next_neighbor) = free_block.next_neighbor_is_free(self.max_ptr) {
            let mut merged_size = FreeBlock::merged_size(&free_block, &next_neighbor);

            if merged_size < new_size {
                if next_neighbor.get_next_neighbor_ptr() != self.max_ptr {
                    return Err(free_block);
                }

                let fb = self.grow(new_size);

                self.more_available_size(fb.get_total_size_bytes());
                self.more_free_size(fb.get_total_size_bytes());

                self.less_free_size(next_neighbor.get_total_size_bytes());
                self.remove_free_block(&next_neighbor);

                next_neighbor = FreeBlock::merge(next_neighbor, fb);
                merged_size = FreeBlock::merged_size(&free_block, &next_neighbor);
            } else {
                self.less_free_size(next_neighbor.get_total_size_bytes());
                self.remove_free_block(&next_neighbor);
            }

            free_block = FreeBlock::merge(free_block, next_neighbor);

            if !FreeBlock::can_split(merged_size, new_size) {
                return Ok(free_block);
            }

            let (free_block, b) = free_block.split(new_size);

            self.more_free_size(b.get_total_size_bytes());
            self.push_free_block(b);

            return Ok(free_block);
        }

        Err(free_block)
    }

    fn try_merge_with_neighbors(&mut self, mut free_block: FreeBlock) -> FreeBlock {
        if let Some(prev_neighbor) = free_block.prev_neighbor_is_free() {
            self.remove_free_block(&prev_neighbor);

            free_block = FreeBlock::merge(prev_neighbor, free_block);
        };

        if let Some(next_neighbor) = free_block.next_neighbor_is_free(self.max_ptr) {
            self.remove_free_block(&next_neighbor);

            free_block = FreeBlock::merge(free_block, next_neighbor);
        }

        free_block
    }

    fn push_free_block(&mut self, mut free_block: FreeBlock) {
        free_block.persist();

        let blocks = self
            .free_blocks
            .entry(free_block.get_size_bytes())
            .or_default();

        let idx = match blocks.binary_search(&free_block) {
            Ok(_) => unreachable!("there can't be two blocks of the same ptr"),
            Err(idx) => idx,
        };

        blocks.insert(idx, free_block);
    }

    fn pop_free_block(&mut self, size: usize) -> Option<FreeBlock> {
        let (&actual_size, blocks) = self.free_blocks.range_mut(size..).next()?;

        let free_block = unsafe { blocks.pop().unwrap_unchecked() };

        if blocks.is_empty() {
            self.free_blocks.remove(&actual_size);
        }

        Some(free_block)
    }

    fn remove_free_block(&mut self, block: &FreeBlock) {
        let blocks = self.free_blocks.get_mut(&block.get_size_bytes()).unwrap();

        match blocks.binary_search(block) {
            Ok(idx) => {
                blocks.remove(idx);

                if blocks.is_empty() {
                    self.free_blocks.remove(&block.get_size_bytes());
                }
            }
            Err(_) => unreachable!(),
        };
    }

    fn grow(&mut self, size: usize) -> FreeBlock {
        let pages_to_grow = ceil_div(size, PAGE_SIZE_BYTES);

        let previous_pages = match stable::grow(pages_to_grow) {
            Ok(pp) => pp,
            Err(_) => panic!("OutOfMemory"),
        };

        let new_max_ptr = (previous_pages + pages_to_grow) * PAGE_SIZE_BYTES as u64;
        let it = FreeBlock::new_total_size(self.max_ptr, (new_max_ptr - self.max_ptr) as usize);

        self.max_ptr = new_max_ptr;

        it
    }

    pub fn debug_validate_free_blocks(&self) {
        assert_eq!(
            self.available_size,
            stable::size_pages() * PAGE_SIZE_BYTES as u64 - MIN_PTR
        );

        let mut total_free_size = 0u64;
        for blocks in self.free_blocks.values() {
            for free_block in blocks {
                free_block.debug_validate();

                total_free_size += free_block.get_total_size_bytes() as u64;
            }
        }

        assert_eq!(total_free_size, self.free_size);
    }

    pub fn _free_blocks_count(&self) -> usize {
        let mut count = 0;

        for blocks in self.free_blocks.values() {
            for _ in blocks {
                count += 1;
            }
        }

        count
    }

    // minimum size is 16 bytes (32 bytes total size)
    // otherwise size is ceiled to the nearest multiple of 8
    #[inline]
    fn pad_size(size: usize) -> usize {
        if size < StablePtr::SIZE * 2 {
            return StablePtr::SIZE * 2;
        }

        (size + 7) & !7
    }
}

impl AsDynSizeBytes for StableMemoryAllocator {
    #[inline]
    fn as_dyn_size_bytes(&self) -> Vec<u8> {
        encode_one(self).unwrap()
    }

    #[inline]
    fn from_dyn_size_bytes(buf: &[u8]) -> Self {
        candid_decode_one_allow_trailing(buf).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use crate::encoding::AsDynSizeBytes;
    use crate::mem::allocator::MIN_PTR;
    use crate::mem::allocator::{StableMemoryAllocator, PAGE_SIZE_BYTES};
    use crate::mem::free_block::FreeBlock;
    use crate::utils::mem_context::stable;
    use crate::utils::Anyway;
    use crate::SSlice;
    use rand::rngs::ThreadRng;
    use rand::seq::SliceRandom;
    use rand::{thread_rng, Rng};
    use std::mem;

    #[test]
    fn encoding_works_fine() {
        let mut sma = StableMemoryAllocator::init();
        sma.allocate(100);

        let buf = sma.as_dyn_size_bytes();
        let sma_1 = StableMemoryAllocator::from_dyn_size_bytes(&buf);

        assert_eq!(sma, sma_1);

        println!("original {:?}", sma);
        println!("new {:?}", sma_1);
    }

    #[test]
    fn initialization_growing_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();

        unsafe {
            let mut sma = StableMemoryAllocator::init();
            let slice = sma.allocate(100);

            assert_eq!(sma._free_blocks_count(), 1);

            sma.store();

            let sma = StableMemoryAllocator::retrieve();
            assert_eq!(sma._free_blocks_count(), 1);

            sma.debug_validate_free_blocks();
        }
    }

    #[test]
    fn initialization_not_growing_works_fine() {
        stable::clear();

        unsafe {
            let mut sma = StableMemoryAllocator::init();
            let slice = sma.allocate(100);

            assert_eq!(sma._free_blocks_count(), 1);

            sma.store();

            let sma = StableMemoryAllocator::retrieve();
            assert_eq!(sma._free_blocks_count(), 1);

            sma.debug_validate_free_blocks();
        }
    }

    #[derive(Debug)]
    enum Action {
        Alloc(SSlice),
        Dealloc(SSlice),
        Realloc(SSlice, SSlice),
    }

    struct Fuzzer {
        allocator: StableMemoryAllocator,
        slices: Vec<SSlice>,
        log: Vec<Action>,
        total_allocated_size: u64,
        rng: ThreadRng,
    }

    impl Fuzzer {
        fn new() -> Self {
            Self {
                allocator: StableMemoryAllocator::init(),
                slices: Vec::default(),
                log: Vec::default(),
                total_allocated_size: 0,
                rng: thread_rng(),
            }
        }

        fn next(&mut self) {
            match self.rng.gen_range(0..100) {
                // ALLOCATE ~ 50%
                0..=50 => {
                    let slice = self
                        .allocator
                        .allocate(self.rng.gen_range(0..(u16::MAX as usize * 2)));

                    self.log.push(Action::Alloc(slice));
                    self.slices.push(slice);

                    let mut buf = vec![100u8; slice.get_size_bytes()];
                    unsafe { crate::mem::write_bytes(slice.make_ptr_by_offset(0), &buf) };

                    let mut buf2 = vec![0u8; slice.get_size_bytes()];
                    unsafe { crate::mem::read_bytes(slice.make_ptr_by_offset(0), &mut buf2) };

                    assert_eq!(buf, buf2);

                    self.total_allocated_size += slice.get_total_size_bytes() as u64;
                }
                // DEALLOCATE ~ 25%
                51..=75 => {
                    if self.slices.len() < 2 {
                        return self.next();
                    }

                    let slice = self.slices.remove(self.rng.gen_range(0..self.slices.len()));
                    self.log.push(Action::Dealloc(slice));

                    self.total_allocated_size -= slice.get_total_size_bytes() as u64;

                    self.allocator.deallocate(slice);
                }
                // REALLOCATE ~ 25%
                _ => {
                    if self.slices.len() < 2 {
                        return self.next();
                    }

                    let slice = self.slices.remove(self.rng.gen_range(0..self.slices.len()));
                    self.total_allocated_size -= slice.get_total_size_bytes() as u64;

                    let slice1 = self
                        .allocator
                        .reallocate(slice, self.rng.gen_range(0..(u16::MAX as usize * 2)))
                        .anyway();
                    self.total_allocated_size += slice1.get_total_size_bytes() as u64;

                    self.log.push(Action::Realloc(slice, slice1));
                    self.slices.push(slice1);

                    let mut buf = vec![100u8; slice1.get_size_bytes()];
                    unsafe { crate::mem::write_bytes(slice1.make_ptr_by_offset(0), &buf) };

                    let mut buf2 = vec![0u8; slice1.get_size_bytes()];
                    unsafe { crate::mem::read_bytes(slice1.make_ptr_by_offset(0), &mut buf2) };

                    assert_eq!(buf, buf2);
                }
            };

            let res = std::panic::catch_unwind(|| {
                self.allocator.debug_validate_free_blocks();
                assert_eq!(
                    self.allocator.get_allocated_size(),
                    self.total_allocated_size
                );
            });

            if res.is_err() {
                panic!("Error happened: {:?}", self.log);
            }
        }
    }

    #[test]
    fn random_works_fine() {
        stable::clear();

        let mut fuzzer = Fuzzer::new();

        for i in 0..10_000 {
            fuzzer.next();
        }
    }

    #[test]
    fn allocation_works_fine() {
        stable::clear();

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

        sma.debug_validate_free_blocks();
    }

    #[test]
    fn basic_flow_works_fine() {
        unsafe {
            stable::clear();

            let mut allocator = StableMemoryAllocator::init();
            allocator.store();

            let mut allocator = StableMemoryAllocator::retrieve();

            println!("before all - {:?}", allocator);

            let slice1 = allocator.allocate(100);

            println!("allocate 100 (1) - {:?}", allocator);

            let slice1 = allocator.reallocate(slice1, 200).anyway();

            println!("reallocate 100 to 200 (1) - {:?}", allocator);

            let slice2 = allocator.allocate(100);

            println!("allocate 100 more (2) - {:?}", allocator);

            let slice3 = allocator.allocate(100);

            println!("allocate 100 more (3) - {:?}", allocator);

            allocator.deallocate(slice1);

            println!("deallocate (1) - {:?}", allocator);

            let slice2 = allocator.reallocate(slice2, 200).anyway();

            println!("reallocate (2) - {:?}", allocator);

            allocator.deallocate(slice3);

            println!("deallocate (3) - {:?}", allocator);

            allocator.deallocate(slice2);

            println!("deallocate (2) - {:?}", allocator);

            allocator.store();

            let mut allocator = StableMemoryAllocator::retrieve();

            let mut slices = Vec::new();
            for _ in 0..5000 {
                slices.push(allocator.allocate(100));
            }

            slices.shuffle(&mut thread_rng());

            for slice in slices {
                allocator.deallocate(slice);
            }

            assert_eq!(allocator.get_allocated_size(), 0);
            allocator.debug_validate_free_blocks();
            println!("{:?}", allocator);
        }
    }
}
