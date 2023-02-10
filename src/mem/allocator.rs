use crate::encoding::dyn_size::candid_decode_one_allow_trailing;
use crate::encoding::{AsDynSizeBytes, AsFixedSizeBytes, Buffer};
use crate::mem::free_block::FreeBlock;
use crate::mem::s_slice::SSlice;
use crate::mem::StablePtr;
use crate::primitive::s_box::SBox;
use crate::primitive::StableType;
use crate::utils::math::ceil_div;
use crate::{stable, OutOfMemory, PAGE_SIZE_BYTES};
use candid::{encode_one, CandidType, Deserialize};
use std::collections::{BTreeMap, HashMap};

pub const ALLOCATOR_PTR: StablePtr = 0;
pub const MIN_PTR: StablePtr = u64::SIZE as u64;
pub const EMPTY_PTR: StablePtr = u64::MAX;

#[derive(Debug, CandidType, Deserialize, Eq, PartialEq)]
pub struct StableMemoryAllocator {
    free_blocks: BTreeMap<u64, Vec<FreeBlock>>,
    custom_data_pointers: HashMap<usize, StablePtr>,
    free_size: u64,
    available_size: u64,
    max_ptr: StablePtr,
    max_pages: u64,
}

impl StableMemoryAllocator {
    pub fn init(max_pages: u64) -> Self {
        let mut it = Self {
            max_ptr: MIN_PTR,
            free_blocks: BTreeMap::default(),
            custom_data_pointers: HashMap::default(),
            free_size: 0,
            available_size: 0,
            max_pages,
        };

        let available_pages = stable::size_pages();
        if it.max_pages != 0 && available_pages > it.max_pages {
            it.max_pages = available_pages;
        }

        let real_max_ptr = available_pages * PAGE_SIZE_BYTES;
        if real_max_ptr > it.max_ptr {
            let free_block = FreeBlock::new_total_size(it.max_ptr, real_max_ptr - it.max_ptr);
            it.more_free_size(free_block.get_total_size_bytes());
            it.more_available_size(free_block.get_total_size_bytes());

            it.push_free_block(free_block);
            it.max_ptr = real_max_ptr;
        }

        it
    }

    pub fn make_sure_can_allocate(&mut self, mut size: u64) -> bool {
        size = Self::pad_size(size);

        if self.free_blocks.range(size..).next().is_some() {
            return true;
        }

        if self.max_ptr > MIN_PTR {
            if let Some(last_free_block) =
                FreeBlock::from_rear_ptr(self.max_ptr - StablePtr::SIZE as u64)
            {
                size -= last_free_block.get_size_bytes();
            }
        }

        match self.grow(size) {
            Ok(fb) => {
                self.more_available_size(fb.get_total_size_bytes());
                self.more_free_size(fb.get_total_size_bytes());

                self.push_free_block(fb);

                true
            }
            Err(_) => false,
        }
    }

    #[allow(clippy::never_loop)]
    pub fn allocate(&mut self, mut size: u64) -> Result<SSlice, OutOfMemory> {
        size = Self::pad_size(size);

        // searching for a free block that is equal or bigger in size, than asked
        let free_block = loop {
            if let Some(fb) = self.pop_free_block(size) {
                break fb;
            } else {
                if self.max_ptr > MIN_PTR {
                    if let Some(last_free_block) =
                        FreeBlock::from_rear_ptr(self.max_ptr - StablePtr::SIZE as u64)
                    {
                        let fb = self.grow(size - last_free_block.get_size_bytes())?;

                        self.more_available_size(fb.get_total_size_bytes());
                        self.more_free_size(fb.get_total_size_bytes());

                        self.remove_free_block(&last_free_block);

                        break FreeBlock::merge(last_free_block, fb);
                    }
                }

                let fb = self.grow(size)?;

                self.more_available_size(fb.get_total_size_bytes());
                self.more_free_size(fb.get_total_size_bytes());

                break fb;
            }
        };

        // if it is bigger - try splitting it in two, taking the first half
        let slice = if FreeBlock::can_split(free_block.get_size_bytes(), size) {
            let (a, b) = free_block.split(size);
            let s = a.to_allocated();

            self.push_free_block(b);

            s
        } else {
            free_block.to_allocated()
        };

        self.less_free_size(slice.get_total_size_bytes());

        Ok(slice)
    }

    #[inline]
    pub fn deallocate(&mut self, slice: SSlice) {
        let free_block = slice.to_free_block();

        self.more_free_size(free_block.get_total_size_bytes());
        self.push_free_block(free_block);
    }

    /// # Safety
    /// Reallocation available only for slices smaller than u32::MAX.
    /// Otherwise - UB
    pub unsafe fn reallocate(
        &mut self,
        slice: SSlice,
        mut new_size: u64,
    ) -> Result<SSlice, OutOfMemory> {
        new_size = Self::pad_size(new_size);

        if new_size <= slice.get_size_bytes() {
            return Ok(slice);
        }

        let free_block = slice.to_free_block();

        // if it is possible to simply "grow" the slice, by merging it with the next neighbor - do that
        if let Ok(fb) = self.try_reallocate_in_place(free_block, new_size) {
            return Ok(fb);
        }

        // FIXME: can be more accurate by checking, if can merge with back first
        if !self.make_sure_can_allocate(new_size) {
            return Err(OutOfMemory);
        }

        // othewise, get ready for move and copy the data
        let mut b = vec![0u8; slice.get_size_bytes() as usize];
        unsafe { crate::mem::read_bytes(slice.offset(0), &mut b) };

        // deallocate the slice
        self.more_free_size(free_block.get_total_size_bytes());
        self.push_free_block(free_block);

        // allocate a new one; unwrapping since we've just checked we can allocate that size
        let new_slice = self.allocate(new_size).unwrap();

        // put the data back
        unsafe { crate::mem::write_bytes(new_slice.offset(0), &b) };

        Ok(new_slice)
    }

    pub fn store(&mut self) -> Result<(), OutOfMemory> {
        // first encode is simply to calculate the required size
        let buf = self.as_dyn_size_bytes();

        // reserving 100 extra bytes in order for the allocator to grow while allocating memory for itself
        let slice = self.allocate(buf.len() as u64 + 100)?;

        let buf = self.as_dyn_size_bytes();

        unsafe { crate::mem::write_bytes(slice.offset(0), &buf) };
        unsafe { crate::mem::write_and_own_fixed(0, &mut slice.as_ptr()) };

        Ok(())
    }

    pub fn retrieve() -> Self {
        let slice_ptr = unsafe { crate::mem::read_and_disown_fixed(0) };
        let slice = SSlice::from_ptr(slice_ptr).unwrap();

        let mut buf = vec![0u8; slice.get_size_bytes() as usize];
        unsafe { crate::mem::read_bytes(slice.offset(0), &mut buf) };

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
    fn more_available_size(&mut self, additional: u64) {
        self.available_size += additional;
    }

    #[inline]
    fn more_free_size(&mut self, additional: u64) {
        self.free_size += additional;
    }

    #[inline]
    fn less_free_size(&mut self, additional: u64) {
        self.free_size -= additional;
    }

    #[inline]
    pub fn store_custom_data<T: AsDynSizeBytes + StableType>(
        &mut self,
        idx: usize,
        mut data: SBox<T>,
    ) {
        unsafe { data.assume_owned_by_stable_memory() };

        self.custom_data_pointers.insert(idx, data.as_ptr());
    }

    #[inline]
    pub fn retrieve_custom_data<T: AsDynSizeBytes + StableType>(
        &mut self,
        idx: usize,
    ) -> Option<SBox<T>> {
        let mut b = unsafe { SBox::from_ptr(self.custom_data_pointers.remove(&idx)?) };
        unsafe { b.assume_not_owned_by_stable_memory() };

        Some(b)
    }

    #[inline]
    pub fn get_max_pages(&self) -> u64 {
        self.max_pages
    }

    fn try_reallocate_in_place(
        &mut self,
        mut free_block: FreeBlock,
        new_size: u64,
    ) -> Result<SSlice, Result<FreeBlock, OutOfMemory>> {
        if let Some(mut next_neighbor) = free_block.next_neighbor_is_free(self.max_ptr) {
            let mut merged_size = FreeBlock::merged_size(&free_block, &next_neighbor);

            if merged_size < new_size {
                if next_neighbor.get_next_neighbor_ptr() != self.max_ptr {
                    return Err(Ok(free_block));
                }

                let fb = self.grow(new_size).map_err(Err)?;

                self.more_available_size(fb.get_total_size_bytes());

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
                return Ok(free_block.to_allocated());
            }

            let (free_block, b) = free_block.split(new_size);

            let slice = free_block.to_allocated();

            self.more_free_size(b.get_total_size_bytes());
            self.push_free_block(b);

            return Ok(slice);
        }

        Err(Ok(free_block))
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
        free_block = self.try_merge_with_neighbors(free_block);

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

    fn pop_free_block(&mut self, size: u64) -> Option<FreeBlock> {
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
            Err(_) => unreachable!("Free block not found {:?} {:?}", block, self.free_blocks),
        };
    }

    fn grow(&mut self, mut size: u64) -> Result<FreeBlock, OutOfMemory> {
        size = FreeBlock::to_total_size(size);
        let pages_to_grow = ceil_div(size, PAGE_SIZE_BYTES);
        let available_pages = stable::size_pages();

        if self.max_pages != 0 && available_pages + pages_to_grow > self.max_pages {
            return Err(OutOfMemory);
        }

        if stable::grow(pages_to_grow).is_err() {
            return Err(OutOfMemory);
        }

        let new_max_ptr = (available_pages + pages_to_grow) * PAGE_SIZE_BYTES;
        let it = FreeBlock::new_total_size(self.max_ptr, new_max_ptr - self.max_ptr);

        self.max_ptr = new_max_ptr;

        Ok(it)
    }

    pub fn debug_validate_free_blocks(&self) {
        assert!(
            self.available_size == 0
                || self.available_size == stable::size_pages() * PAGE_SIZE_BYTES - MIN_PTR
        );

        let mut total_free_size = 0u64;
        for blocks in self.free_blocks.values() {
            for free_block in blocks {
                free_block.debug_validate();

                total_free_size += free_block.get_total_size_bytes();
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
    fn pad_size(size: u64) -> u64 {
        if size < (StablePtr::SIZE * 2) as u64 {
            return (StablePtr::SIZE * 2) as u64;
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
    use crate::mem::allocator::StableMemoryAllocator;
    use crate::primitive::s_box::SBox;
    use crate::utils::mem_context::stable;
    use crate::SSlice;
    use rand::rngs::ThreadRng;
    use rand::seq::SliceRandom;
    use rand::{thread_rng, Rng};

    #[test]
    fn encoding_works_fine() {
        let mut sma = StableMemoryAllocator::init(0);
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
            let mut sma = StableMemoryAllocator::init(0);
            println!("{:?}", sma);

            let slice = sma.allocate(100).unwrap();
            println!("{:?}", sma);

            sma.store_custom_data(1, SBox::new(10u64).unwrap());

            assert_eq!(sma._free_blocks_count(), 1);

            sma.store();

            println!("after store {:?}", sma);
            let mut sma = StableMemoryAllocator::retrieve();

            assert_eq!(sma.retrieve_custom_data::<u64>(1).unwrap().into_inner(), 10);

            println!("after retrieve {:?}", sma);
            assert_eq!(sma._free_blocks_count(), 1);

            sma.debug_validate_free_blocks();
        }
    }

    #[test]
    fn initialization_not_growing_works_fine() {
        stable::clear();

        unsafe {
            let mut sma = StableMemoryAllocator::init(0);
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
        AllocOOM(u64),
        Dealloc(SSlice),
        Realloc(SSlice, SSlice),
        ReallocOOM(u64),
        CanisterUpgrade,
        CanisterUpgradeOOM,
    }

    struct Fuzzer {
        allocator: StableMemoryAllocator,
        slices: Vec<SSlice>,
        log: Vec<Action>,
        total_allocated_size: u64,
        rng: ThreadRng,
    }

    impl Fuzzer {
        fn new(max_pages: u64) -> Self {
            Self {
                allocator: StableMemoryAllocator::init(max_pages),
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
                    let size = self.rng.gen_range(0..(u16::MAX as u64 * 2));

                    if self.allocator.make_sure_can_allocate(size) {
                        let slice = self.allocator.allocate(size).unwrap();

                        self.log.push(Action::Alloc(slice));
                        self.slices.push(slice);

                        let mut buf = vec![100u8; slice.get_size_bytes() as usize];
                        unsafe { crate::mem::write_bytes(slice.offset(0), &buf) };

                        let mut buf2 = vec![0u8; slice.get_size_bytes() as usize];
                        unsafe { crate::mem::read_bytes(slice.offset(0), &mut buf2) };

                        assert_eq!(buf, buf2);

                        self.total_allocated_size += slice.get_total_size_bytes() as u64;
                    } else {
                        assert!(self.allocator.allocate(size).is_err());
                        self.log.push(Action::AllocOOM(size));
                    }
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
                76..=98 => {
                    if self.slices.len() < 2 {
                        return self.next();
                    }

                    let idx_to_remove = self.rng.gen_range(0..self.slices.len());
                    let size = self.rng.gen_range(0..(u16::MAX as u64 * 2));

                    let slice = self.slices[idx_to_remove];
                    if let Ok(slice1) = unsafe { self.allocator.reallocate(slice, size) } {
                        self.total_allocated_size -= slice.get_total_size_bytes();

                        self.slices.remove(idx_to_remove);
                        self.total_allocated_size += slice1.get_total_size_bytes();

                        self.log.push(Action::Realloc(slice, slice1));
                        self.slices.push(slice1);

                        let mut buf = vec![100u8; slice1.get_size_bytes() as usize];
                        unsafe { crate::mem::write_bytes(slice1.offset(0), &buf) };

                        let mut buf2 = vec![0u8; slice1.get_size_bytes() as usize];
                        unsafe { crate::mem::read_bytes(slice1.offset(0), &mut buf2) };

                        assert_eq!(buf, buf2);
                    } else {
                        self.log.push(Action::ReallocOOM(size));
                    }
                }
                // CANISTER UPGRADE ~1%
                _ => {
                    if self.allocator.store().is_ok() {
                        self.allocator = StableMemoryAllocator::retrieve();

                        self.log.push(Action::CanisterUpgrade);
                    } else {
                        self.log.push(Action::CanisterUpgradeOOM);
                    }
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
                panic!("{:?} {:?}", self.log.last().unwrap(), self.allocator);
            }
        }
    }

    #[test]
    fn random_works_fine() {
        stable::clear();

        let mut fuzzer = Fuzzer::new(0);

        for i in 0..10_000 {
            fuzzer.next();
        }

        for action in &fuzzer.log {
            match action {
                Action::Alloc(_)
                | Action::Realloc(_, _)
                | Action::Dealloc(_)
                | Action::CanisterUpgrade => {}
                _ => panic!("Fuzzer cant OOM here"),
            }
        }

        let mut fuzzer = Fuzzer::new(30);

        for i in 0..10_000 {
            fuzzer.next();
        }
    }

    #[test]
    fn allocation_works_fine() {
        stable::clear();

        let mut sma = StableMemoryAllocator::init(0);

        let mut slices = vec![];

        // try to allocate 1000 MB
        for i in 0..1024 {
            let slice = sma.allocate(1024).unwrap();

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
            slice = unsafe { sma.reallocate(slice, 2 * 1024).unwrap() };

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

            let mut allocator = StableMemoryAllocator::init(0);
            allocator.store();

            let mut allocator = StableMemoryAllocator::retrieve();

            println!("before all - {:?}", allocator);

            let slice1 = allocator.allocate(100).unwrap();

            println!("allocate 100 (1) - {:?}", allocator);

            let slice1 = allocator.reallocate(slice1, 200).unwrap();

            println!("reallocate 100 to 200 (1) - {:?}", allocator);

            let slice2 = allocator.allocate(100).unwrap();

            println!("allocate 100 more (2) - {:?}", allocator);

            let slice3 = allocator.allocate(100).unwrap();

            println!("allocate 100 more (3) - {:?}", allocator);

            allocator.deallocate(slice1);

            println!("deallocate (1) - {:?}", allocator);

            let slice2 = allocator.reallocate(slice2, 200).unwrap();

            println!("reallocate (2) - {:?}", allocator);

            allocator.deallocate(slice3);

            println!("deallocate (3) - {:?}", allocator);

            allocator.deallocate(slice2);

            println!("deallocate (2) - {:?}", allocator);

            allocator.store();

            let mut allocator = StableMemoryAllocator::retrieve();

            let mut slices = Vec::new();
            for _ in 0..5000 {
                slices.push(allocator.allocate(100).unwrap());
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
