use crate::mem_context::{stable, OutOfMemory};
use crate::membox::{MemBox, Side, Size, Word, MEM_BOX_MIN_SIZE};
use crate::types::{EMPTY_PTR, PAGE_SIZE_BYTES};
use crate::utils::fast_log2;
use std::mem::size_of;

pub const MAGIC: [u8; 4] = [b'S', b'M', b'A', b'M'];
pub const SEG_CLASS_PTRS_COUNT: Size = Size::BITS as Size - 4;
pub const CUSTOM_DATA_PTRS_COUNT: Size = 4;

pub type SegClassId = u32;
pub type CustomDataId = Size;

fn get_seg_class_id(size: Size) -> SegClassId {
    let mut log = fast_log2(size);

    if 2usize.pow(log) < size {
        log += 1;
    }

    if log > 3 {
        (log - 4) as SegClassId
    } else {
        0
    }
}

fn align_size(seg_class_id: SegClassId) -> Size {
    if seg_class_id == 0 {
        size_of::<Word>() * 2
    } else {
        2usize.pow(seg_class_id + 4)
    }
}

fn factorize_size(mut size: Size) -> Vec<Size> {
    let mut result = vec![];

    while size > 0 {
        let log = fast_log2(size);
        let factorized = 2usize.pow(log);

        result.push(factorized);
        size -= factorized;
    }

    result
}

#[derive(Debug)]
pub struct Free;

impl MemBox<Free> {
    pub fn set_prev_free_ptr(&mut self, prev_ptr: Word) {
        self.assert_allocated(false, None);

        self.write_word(0, prev_ptr);
    }

    pub fn get_prev_free_ptr(&self) -> Word {
        self.assert_allocated(false, None);

        self.read_word(0)
    }

    pub fn set_next_free_ptr(&mut self, next_ptr: Word) {
        self.assert_allocated(false, None);

        self.write_word(size_of::<Word>(), next_ptr);
    }

    pub fn get_next_free_ptr(&self) -> Word {
        self.assert_allocated(false, None);

        self.read_word(size_of::<Word>())
    }
}

#[derive(Debug)]
pub struct StableMemoryAllocator;

impl MemBox<StableMemoryAllocator> {
    const SIZE: Size = MAGIC.len()
        + SEG_CLASS_PTRS_COUNT * size_of::<Word>()
        + CUSTOM_DATA_PTRS_COUNT * size_of::<Word>();

    const _EMPTY: [u8; Self::SIZE - MAGIC.len()] = [0u8; Self::SIZE - MAGIC.len()];

    /// # Safety
    pub unsafe fn init(offset: Word) -> Self {
        let mut allocator = MemBox::<StableMemoryAllocator>::new(offset, Self::SIZE, true);

        allocator.write(0, &MAGIC);
        allocator.write(MAGIC.len(), &Self::_EMPTY);

        let total_free_size =
            stable::size_pages() * PAGE_SIZE_BYTES as Word - allocator.get_next_neighbor_ptr();

        if total_free_size > 0 {
            let mut ptr = allocator.get_next_neighbor_ptr();

            let free_mem_box = MemBox::<Free>::new_total_size(ptr, total_free_size as Size, false);
            allocator.push_free_membox(free_mem_box);
        }

        allocator
    }

    /// # Safety
    pub unsafe fn reinit(offset: Word) -> Option<Self> {
        let membox = MemBox::<StableMemoryAllocator>::from_ptr(offset, Side::Start)?;
        let (size, allocated) = membox.get_meta();
        if !allocated || size != Self::SIZE {
            return None;
        }

        let mut magic = [0u8; MAGIC.len()];
        membox.read(0, &mut magic);
        if magic != MAGIC {
            return None;
        }

        Some(membox)
    }

    fn push_free_membox(&mut self, mut membox: MemBox<Free>) {
        let (size, allocated) = membox.get_meta();
        membox.assert_allocated(false, Some(allocated));

        let seg_class_id = get_seg_class_id(size);

        let head_opt = unsafe { self.get_seg_class_head(seg_class_id) };

        self.set_seg_class_head(seg_class_id, membox.get_ptr());
        membox.set_prev_free_ptr(EMPTY_PTR);

        // TODO: check if neighbors are also here and merge if needed

        match head_opt {
            None => {
                membox.set_next_free_ptr(EMPTY_PTR);
            }
            Some(mut head) => {
                membox.set_next_free_ptr(head.get_ptr());

                head.set_prev_free_ptr(membox.get_ptr());
            }
        }
    }

    fn find_free_membox(&mut self, size: Size) -> Result<MemBox<Free>, OutOfMemory> {
        let mut seg_class_id = get_seg_class_id(size);
        let free_membox_opt = unsafe { self.get_seg_class_head(seg_class_id) };

        // iterate over this seg class, until big enough membox found or til it ends
        if let Some(mut free_membox) = free_membox_opt {
            let mut first_iteration = true;

            loop {
                let (membox_size, _) = free_membox.get_meta();

                // if valid membox found,
                if membox_size >= size {
                    if first_iteration {
                        self.set_seg_class_head(seg_class_id, free_membox.get_next_free_ptr());
                    } else {
                        let mut prev = unsafe {
                            MemBox::<Free>::from_ptr(free_membox.get_prev_free_ptr(), Side::Start)
                                .unwrap()
                        };
                        let mut next = unsafe {
                            MemBox::<Free>::from_ptr(free_membox.get_next_free_ptr(), Side::Start)
                                .unwrap()
                        };

                        prev.set_next_free_ptr(next.get_ptr());
                        next.set_prev_free_ptr(prev.get_ptr());
                    }

                    free_membox.set_prev_free_ptr(EMPTY_PTR);
                    free_membox.set_next_free_ptr(EMPTY_PTR);

                    return Ok(free_membox);
                }

                let next_ptr = free_membox.get_next_free_ptr();
                if next_ptr == EMPTY_PTR {
                    break;
                }

                first_iteration = false;
                free_membox = unsafe { MemBox::<Free>::from_ptr(next_ptr, Side::Start).unwrap() };
            }
        }

        // if no appropriate membox was found previously, try to find any of larger size
        let mut free_membox_opt = None;
        seg_class_id += 1;

        while seg_class_id < SEG_CLASS_PTRS_COUNT as u32 {
            free_membox_opt = unsafe { self.get_seg_class_head(seg_class_id) };

            if free_membox_opt.is_some() {
                break;
            }

            seg_class_id += 1;
        }

        match free_membox_opt {
            // if at least one such a big membox found, pop it, split in two, take first, push second
            Some(free_membox) => {
                self.set_seg_class_head(seg_class_id, free_membox.get_next_free_ptr());

                let (result, additional) = unsafe { free_membox.split(size).unwrap() };
                self.push_free_membox(additional);

                Ok(result)
            }
            // otherwise, grow and if grown successfully, split in two, take first, push second
            None => {
                let pages_to_grow = size / PAGE_SIZE_BYTES + 1;
                let prev_total_free_size =
                    stable::grow(pages_to_grow as u64)? * PAGE_SIZE_BYTES as Word;

                let total_free_size =
                    stable::size_pages() * PAGE_SIZE_BYTES as Word - prev_total_free_size;

                let ptr = prev_total_free_size;

                let new_free_membox =
                    unsafe { MemBox::<Free>::new_total_size(ptr, total_free_size as Size, false) };

                match unsafe { new_free_membox.split(size) } {
                    Ok((result, additional)) => {
                        self.push_free_membox(additional);

                        Ok(result)
                    }
                    Err(new_free_membox) => Ok(new_free_membox),
                }
            }
        }
    }

    unsafe fn get_seg_class_head(&self, id: SegClassId) -> Option<MemBox<Free>> {
        let ptr = self.read_word(Self::get_seg_class_head_offset(id));
        if ptr == EMPTY_PTR {
            return None;
        }

        Some(MemBox::<Free>::from_ptr(ptr, Side::Start).unwrap())
    }

    fn set_seg_class_head(&mut self, id: SegClassId, head_ptr: Word) {
        self.write_word(Self::get_seg_class_head_offset(id), head_ptr);
    }

    fn get_seg_class_head_offset(seg_class_id: SegClassId) -> Size {
        assert!(seg_class_id < SEG_CLASS_PTRS_COUNT as SegClassId);

        MAGIC.len() + seg_class_id as Size * size_of::<Word>()
    }

    fn get_custom_data_offset(id: CustomDataId) -> Size {
        assert!(id < CUSTOM_DATA_PTRS_COUNT);

        MAGIC.len() + SEG_CLASS_PTRS_COUNT * size_of::<Word>() + id * size_of::<Word>()
    }
}
/*
pub struct StableMemoryAllocator {
    pub segregation_size_classes: [SegregationClassPtr; SEG_CLASS_PTRS_COUNT],
    pub custom_data: [u64; CUSTOM_DATA_PTRS_COUNT],
    pub offset: u64,
}

impl<T: MemContext + Clone> StableMemoryAllocator<T> {
    const SIZE: usize = MAGIC.len()
        + SEG_CLASS_PTRS_COUNT * size_of::<SegregationClassPtr>()
        + CUSTOM_DATA_PTRS_COUNT * size_of::<u64>();

    pub fn allocate(&mut self, size: u64, context: &mut T) -> Result<StableBox<T>, SMAError> {
        let mut mem_block = if let Some((appropriate_mem_block, seg_class_idx)) =
            self.find_appropriate_free_mem_block(size, context)
        {
            self.remove_block_from_free_list(&appropriate_mem_block, seg_class_idx, context);

            self.split_if_needed(appropriate_mem_block, size, context)
        } else {
            // this block is not added to the free list yet, so we won't remove it from there
            // can return OOM error
            let big_mem_block = self.grow_and_create_new_free_block(
                size + (MEM_BLOCK_OVERHEAD_BYTES * 2) as u64,
                context,
            )?;

            self.split_if_needed(big_mem_block, size, context)
        };

        mem_block.set_allocated(true, context);

        Ok(mem_block)
    }

    pub fn deallocate(&mut self, offset: u64, context: &mut T) {
        let mut mem_block = StableBox::read_at(offset, MemBlockSide::Start, context)
            .unwrap_or_else(|| unreachable!());

        if !mem_block.allocated {
            unreachable!();
        }

        mem_block.set_allocated(false, context);

        mem_block = self.try_merge(mem_block, MemBlockSide::End, context);
        mem_block = self.try_merge(mem_block, MemBlockSide::Start, context);

        self.add_block_to_free_list(&mut mem_block, context);
    }

    pub fn reallocate(
        &mut self,
        offset: u64,
        wanted_size: u64,
        context: &mut T,
    ) -> Result<StableBox<T>, SMAError> {
        let mut mem_block = StableBox::read_at(offset, MemBlockSide::Start, context).unwrap();

        if mem_block.size >= wanted_size {
            return Ok(mem_block);
        }

        if mem_block.size > u32::MAX as u64 {
            return Err(SMAError::ReallocationTooBig);
        }

        let mut content = vec![0u8; mem_block.size as usize];
        mem_block.read_bytes(0, &mut content, context).unwrap();

        mem_block.set_allocated(false, context);

        mem_block = self.try_merge(mem_block, MemBlockSide::Start, context);

        if mem_block.size >= wanted_size {
            mem_block = self.split_if_needed(mem_block, wanted_size, context);
            mem_block.set_allocated(true, context);

            mem_block.write_bytes(0, &content, context).unwrap();

            return Ok(mem_block);
        }

        mem_block.set_allocated(true, context);

        self.deallocate(offset, context);
        self.allocate(wanted_size, context).map(|mut mem_block| {
            mem_block.write_bytes(0, &content, context).unwrap();
            mem_block
        })
    }

    pub fn init(offset: u64, context: &mut T) -> Result<Self, SMAError> {
        Self::init_grow_if_need(offset, context)?;

        context.write(offset, &MAGIC);

        let mut this = StableMemoryAllocator {
            segregation_size_classes: [SegregationClassPtr::default(); SEG_CLASS_PTRS_COUNT],
            custom_data: [u64::default(); CUSTOM_DATA_PTRS_COUNT],
            marker: PhantomData,
            offset,
        };

        this.init_first_free_mem_block(offset + Self::SIZE as u64, context)?;

        Ok(this)
    }

    pub fn reinit(offset: u64, context: &T) -> Result<Self, SMAError> {
        // checking magic sequence
        let mut magic_buf = [0u8; MAGIC.len()];
        context.read(offset, &mut magic_buf);

        if magic_buf != MAGIC {
            return Err(SMAError::InvalidMagicSequence);
        }

        // reading segregation classes
        let mut segregation_classes_buf =
            [0u8; SEG_CLASS_PTRS_COUNT * size_of::<SegregationClassPtr>()];
        context.read(offset + MAGIC.len() as u64, &mut segregation_classes_buf);

        let mut segregation_size_classes =
            [SegregationClassPtr::default(); SEG_CLASS_PTRS_COUNT];

        segregation_classes_buf
            .chunks_exact(size_of::<SegregationClassPtr>())
            .enumerate()
            .for_each(|(idx, it)| {
                let mut buf = [0u8; size_of::<SegregationClassPtr>()];
                buf.copy_from_slice(it);

                segregation_size_classes[idx] = SegregationClassPtr::from_le_bytes(buf);
            });

        // reading custom data
        let mut custom_data_buf = [0u8; CUSTOM_DATA_PTRS_COUNT * size_of::<u64>()];
        context.read(
            offset
                + (MAGIC.len() + SEG_CLASS_PTRS_COUNT * size_of::<SegregationClassPtr>()) as u64,
            &mut custom_data_buf,
        );

        let mut custom_data = [u64::default(); CUSTOM_DATA_PTRS_COUNT];
        custom_data_buf
            .chunks_exact(size_of::<u64>())
            .enumerate()
            .for_each(|(idx, it)| {
                let mut buf = [0u8; size_of::<u64>()];
                buf.copy_from_slice(it);

                custom_data[idx] = u64::from_le_bytes(buf);
            });

        // returning
        Ok(Self {
            segregation_size_classes,
            custom_data,
            marker: PhantomData,
            offset,
        })
    }

    pub fn set_custom_data(&mut self, idx: usize, ptr: u64, context: &mut T) -> bool {
        if idx >= CUSTOM_DATA_PTRS_COUNT {
            return false;
        }

        self.custom_data[idx] = ptr;
        context.write(
            self.offset
                + (MAGIC.len()
                    + SEG_CLASS_PTRS_COUNT * size_of::<SegregationClassPtr>()
                    + idx * size_of::<u64>()) as u64,
            &ptr.to_le_bytes(),
        );

        true
    }

    pub fn get_custom_data(&self, idx: usize) -> u64 {
        self.custom_data[idx]
    }

    fn try_merge(
        &mut self,
        mut mem_block: StableBox<T>,
        side: MemBlockSide,
        context: &mut T,
    ) -> StableBox<T> {
        match side {
            MemBlockSide::Start => {
                if let Some(mut next_mem_block) = StableBox::read_at(
                    mem_block.ptr + mem_block.size + (MEM_BLOCK_OVERHEAD_BYTES * 2) as u64,
                    MemBlockSide::Start,
                    context,
                ) {
                    if !next_mem_block.allocated {
                        next_mem_block =
                            self.try_merge(next_mem_block, MemBlockSide::Start, context);

                        self.remove_block_from_free_list(
                            &next_mem_block,
                            self.find_seg_class_idx(next_mem_block.size),
                            context,
                        );
                        mem_block = mem_block.merge_with(next_mem_block, context);
                    }
                }
            }
            MemBlockSide::End => {
                if let Some(mut prev_mem_block) =
                    StableBox::read_at(mem_block.ptr, MemBlockSide::End, context)
                {
                    if !prev_mem_block.allocated {
                        prev_mem_block = self.try_merge(prev_mem_block, MemBlockSide::End, context);

                        self.remove_block_from_free_list(
                            &prev_mem_block,
                            self.find_seg_class_idx(prev_mem_block.size),
                            context,
                        );
                        mem_block = mem_block.merge_with(prev_mem_block, context);
                    }
                }
            }
        };

        mem_block
    }

    fn grow_and_create_new_free_block(
        &mut self,
        size: u64,
        context: &mut T,
    ) -> Result<StableBox<T>, SMAError> {
        let offset = context.size_pages() * PAGE_SIZE_BYTES as u64;

        let mut size_need_pages = size / PAGE_SIZE_BYTES as u64;
        if size % PAGE_SIZE_BYTES as u64 > 0 {
            size_need_pages += 1;
        }

        context
            .grow(size_need_pages as u64)
            .map_err(|_| SMAError::OutOfMemory)?;

        let mem_block = StableBox::write_free_at(
            offset,
            size_need_pages * PAGE_SIZE_BYTES as u64 - (MEM_BLOCK_OVERHEAD_BYTES * 2) as u64,
            EMPTY_PTR,
            EMPTY_PTR,
            context,
        );

        Ok(mem_block)
    }

    fn remove_block_from_free_list(
        &mut self,
        mem_block: &StableBox<T>,
        mem_block_seg_class_idx: usize,
        context: &mut T,
    ) {
        let prev_offset = mem_block.get_prev_free();
        let next_offset = mem_block.get_next_free();

        if prev_offset != EMPTY_PTR && next_offset != EMPTY_PTR {
            let mut prev = StableBox::read_at(prev_offset, MemBlockSide::Start, context)
                .unwrap_or_else(|| unreachable!());

            let mut next = StableBox::read_at(next_offset, MemBlockSide::Start, context)
                .unwrap_or_else(|| unreachable!());

            prev.set_next_free(next_offset, context);
            next.set_prev_free(prev_offset, context);
        } else if prev_offset != EMPTY_PTR {
            let mut prev = StableBox::read_at(prev_offset, MemBlockSide::Start, context)
                .unwrap_or_else(|| unreachable!());

            prev.set_next_free(EMPTY_PTR, context);
        } else if next_offset != EMPTY_PTR {
            let mut next = StableBox::read_at(next_offset, MemBlockSide::Start, context)
                .unwrap_or_else(|| unreachable!());

            next.set_prev_free(EMPTY_PTR, context);

            // don't forget to make it the first of the class
            self.set_segregation_class(mem_block_seg_class_idx, next.ptr, context);
        } else {
            // appropriate is the only one in the class - delete the whole class
            self.set_segregation_class(mem_block_seg_class_idx, EMPTY_PTR, context);
        }
    }

    fn add_block_to_free_list(&mut self, new_mem_block: &mut StableBox<T>, context: &mut T) {
        let seg_class_idx = self.find_seg_class_idx(new_mem_block.size);

        // if there are no blocks in this class - just insert
        if self.segregation_size_classes[seg_class_idx] == EMPTY_PTR {
            self.set_segregation_class(seg_class_idx, new_mem_block.ptr, context);

            return;
        }

        // if there are some blocks - find a place for it, such as addr(prev) < addr(new) < addr(next)
        let mut cur_mem_block = StableBox::read_at(
            self.segregation_size_classes[seg_class_idx],
            MemBlockSide::Start,
            context,
        )
        .unwrap_or_else(|| unreachable!());

        // TODO: remove
        if cur_mem_block.get_prev_free() != EMPTY_PTR {
            unreachable!();
        }

        // if the inserting block address is lesser than the first address in the free list - insert before
        if new_mem_block.ptr < cur_mem_block.ptr {
            self.set_segregation_class(seg_class_idx, new_mem_block.ptr, context);
            cur_mem_block.set_prev_free(new_mem_block.ptr, context);
            new_mem_block.set_next_free(cur_mem_block.ptr, context);

            return;
        }

        // if there is only one mem block in the free list - insert after
        if cur_mem_block.get_next_free() == EMPTY_PTR {
            cur_mem_block.set_next_free(new_mem_block.ptr, context);
            new_mem_block.set_prev_free(cur_mem_block.ptr, context);

            return;
        }

        // otherwise - try to find a place in between or at the end of the free list
        let mut next_mem_block =
            StableBox::read_at(cur_mem_block.get_next_free(), MemBlockSide::Start, context)
                .unwrap_or_else(|| unreachable!());

        loop {
            if new_mem_block.ptr > cur_mem_block.ptr && new_mem_block.ptr < next_mem_block.ptr {
                cur_mem_block.set_next_free(new_mem_block.ptr, context);
                new_mem_block.set_prev_free(cur_mem_block.ptr, context);

                next_mem_block.set_prev_free(new_mem_block.ptr, context);
                new_mem_block.set_next_free(next_mem_block.ptr, context);

                return;
            }

            if next_mem_block.get_next_free() == EMPTY_PTR {
                next_mem_block.set_next_free(new_mem_block.ptr, context);
                new_mem_block.set_prev_free(next_mem_block.ptr, context);

                return;
            }

            cur_mem_block = next_mem_block;
            next_mem_block =
                StableBox::read_at(cur_mem_block.get_next_free(), MemBlockSide::Start, context)
                    .unwrap_or_else(|| unreachable!());
        }
    }

    // find a free block that has a size bigger than the provided size, but optimal (not too big)
    // if there is none - return None
    fn find_appropriate_free_mem_block(
        &self,
        size: u64,
        context: &mut T,
    ) -> Option<(StableBox<T>, usize)> {
        let initial_seg_class_idx = self.find_seg_class_idx(size);
        let mut result: Option<(StableBox<T>, usize)> = None;

        // for each segregation class, starting from the most appropriate (closer)
        for seg_class_idx in initial_seg_class_idx..SEG_CLASS_PTRS_COUNT {
            // skip if there is no free blocks at all
            if self.segregation_size_classes[seg_class_idx] == EMPTY_PTR {
                continue;
            }

            // try to find at least one appropriate (size is bigger) free block
            let mut appropriate_found = false;
            let mut appropriate_free_mem_block = StableBox::read_at(
                self.segregation_size_classes[seg_class_idx],
                MemBlockSide::Start,
                context,
            )
            .unwrap_or_else(|| unreachable!());
            let mut next_free = appropriate_free_mem_block.get_next_free();

            loop {
                if appropriate_free_mem_block.size < size {
                    if next_free == EMPTY_PTR {
                        break;
                    }

                    appropriate_free_mem_block =
                        StableBox::read_at(next_free, MemBlockSide::Start, context)
                            .unwrap_or_else(|| unreachable!());
                    next_free = appropriate_free_mem_block.get_next_free();
                } else {
                    appropriate_found = true;
                    break;
                }
            }

            if !appropriate_found {
                continue;
            }

            // then try to find a block that is closer to the provided size in the remainder of blocks of this segregation class
            loop {
                if next_free == EMPTY_PTR {
                    break;
                }

                let mut next_free_mem_block =
                    StableBox::read_at(next_free, MemBlockSide::Start, context)
                        .unwrap_or_else(|| unreachable!());

                if next_free_mem_block.size < size {
                    next_free = next_free_mem_block.get_next_free();

                    if next_free == EMPTY_PTR {
                        break;
                    }

                    continue;
                }

                if appropriate_free_mem_block.size - size > next_free_mem_block.size - size {
                    appropriate_free_mem_block = next_free_mem_block.clone();
                }

                next_free = next_free_mem_block.get_next_free();

                if next_free == EMPTY_PTR {
                    break;
                }
            }

            // return the one closest to provided size
            result = Some((appropriate_free_mem_block, seg_class_idx));
        }

        result
    }

    // TODO: rewrite using low-level functions
    fn init_first_free_mem_block(
        &mut self,
        offset: u64,
        context: &mut T,
    ) -> Result<StableBox<T>, SMAError> {
        let grown_bytes = context.size_pages() * PAGE_SIZE_BYTES as u64;

        if offset > grown_bytes {
            unreachable!();
        }

        let mem_block_size_bytes = grown_bytes - offset - (MEM_BLOCK_OVERHEAD_BYTES * 2) as u64;
        if mem_block_size_bytes < MIN_MEM_BLOCK_SIZE_BYTES as u64 {
            context.grow(1).map_err(|_| SMAError::OutOfMemory)?;
        }

        let seg_idx = self.find_seg_class_idx(mem_block_size_bytes);
        let mem_block = StableBox::write_free_at(offset, mem_block_size_bytes, 0, 0, context);

        self.set_segregation_class(seg_idx, offset, context);

        Ok(mem_block)
    }

    fn find_seg_class_idx(&self, block_size_bytes: u64) -> usize {
        let log = fast_log2_64(block_size_bytes);

        if log > 3 {
            log as usize - 4
        } else {
            0
        }
    }

    fn set_segregation_class(
        &mut self,
        seg_class_idx: usize,
        ptr: SegregationClassPtr,
        context: &mut T,
    ) {
        if seg_class_idx >= SEG_CLASS_PTRS_COUNT {
            unreachable!();
        }

        self.segregation_size_classes[seg_class_idx] = ptr;
        let buf = ptr.to_le_bytes();

        context.write(
            self.offset + (MAGIC.len() + seg_class_idx * size_of::<SegregationClassPtr>()) as u64,
            &buf,
        );
    }

    fn init_grow_if_need(offset: u64, context: &mut T) -> Result<(), SMAError> {
        let size_need_bytes = offset
            + MAGIC.len() as u64
            + SEG_CLASS_PTRS_COUNT as u64 * size_of::<SegregationClassPtr>() as u64;

        let mut size_need_pages = size_need_bytes / PAGE_SIZE_BYTES as u64;
        if size_need_bytes % PAGE_SIZE_BYTES as u64 > 0 {
            size_need_pages += 1;
        }

        let size_have_pages = context.size_pages();

        if size_have_pages < size_need_pages {
            context
                .grow(size_need_pages - size_have_pages)
                .map_err(|_| SMAError::OutOfMemory)?;
        }

        Ok(())
    }

    fn split_if_needed(
        &mut self,
        mem_block: StableBox<T>,
        size: u64,
        context: &mut T,
    ) -> StableBox<T> {
        if mem_block.size - size >= MIN_MEM_BLOCK_SIZE_BYTES as u64 {
            let (old_mem_block, mut new_free_block) = mem_block.split_mem_block(size, context);

            self.add_block_to_free_list(&mut new_free_block, context);

            old_mem_block
        } else {
            mem_block
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::mem_block::{MemBlockSide, StableBox, MEM_BLOCK_OVERHEAD_BYTES};
    use crate::mem_context::{MemContext, TestMemContext};
    use crate::stable_memory_allocator::StableMemoryAllocator;
    use crate::types::{EMPTY_PTR, PAGE_SIZE_BYTES};

    #[test]
    fn init_works_fine() {
        let mut context = TestMemContext::default();
        let mut allocator = StableMemoryAllocator::init(0, &mut context).ok().unwrap();

        let initial_free_mem_block = StableBox::read_at(
            StableMemoryAllocator::<TestMemContext>::SIZE as u64,
            MemBlockSide::Start,
            &context,
        )
        .unwrap();

        assert_eq!(allocator.offset, 0, "Allocator offset is invalid");
        assert_eq!(
            initial_free_mem_block.ptr,
            StableMemoryAllocator::<TestMemContext>::SIZE as u64,
            "Initial free block offset is invalid"
        );
        assert!(
            initial_free_mem_block.size > 0,
            "Bad initial mem block size"
        );
        assert!(
            !initial_free_mem_block.allocated,
            "Initial mem block is used"
        );
        assert_eq!(
            initial_free_mem_block.get_next_free(),
            EMPTY_PTR,
            "Initial mem block should contain no next block"
        );
        assert_eq!(
            initial_free_mem_block.get_prev_free(),
            EMPTY_PTR,
            "Initial mem block should contain no prev block"
        );
        assert_eq!(
            initial_free_mem_block.ptr
                + initial_free_mem_block.size
                + (MEM_BLOCK_OVERHEAD_BYTES * 2) as u64,
            context.size_pages() * PAGE_SIZE_BYTES as u64,
            "Invalid total size"
        );
        assert_eq!(
            initial_free_mem_block.ptr,
            StableMemoryAllocator::<TestMemContext>::SIZE as u64,
            "Invalid SMA size"
        );

        allocator.set_custom_data(0, 10, &mut context);

        let allocator_re = StableMemoryAllocator::reinit(0, &context).ok().unwrap();

        assert_eq!(
            allocator.segregation_size_classes, allocator_re.segregation_size_classes,
            "Segregation size classes mismatch"
        );

        assert_eq!(
            allocator.custom_data, allocator_re.custom_data,
            "Custom data mismatch"
        );

        assert_eq!(allocator.offset, allocator_re.offset, "Offset mismatch");

        assert_eq!(
            allocator_re.get_custom_data(0),
            10,
            "Custom data entry mismatch"
        );
    }

    #[test]
    fn allocation_works_fine() {
        let mut context = TestMemContext::default();
        let mut sma = StableMemoryAllocator::init(0, &mut context).ok().unwrap();

        let mut mem_block = sma.allocate(1000, &mut context).ok().unwrap();

        let c = [b'k', b'e', b'k'];

        let res = mem_block.write_bytes(0, &c, &mut context).ok().unwrap();

        let mut content = [0u8; 1000];
        mem_block
            .read_bytes(0, &mut content, &context)
            .ok()
            .unwrap();

        assert_eq!(content.len(), 1000, "Invalid content length 1");
        assert_eq!(content[0..3], c, "Invalid content");

        let mut content = [0u8; 100 * 1024];
        let mem_block = sma.allocate(100 * 1024, &mut context).ok().unwrap();

        mem_block
            .read_bytes(0, &mut content, &context)
            .ok()
            .unwrap();
        assert_eq!(content.len(), 100 * 1024, "Invalid length 2");
    }

    #[test]
    fn deallocate_works_fine() {
        let mut context = TestMemContext::default();
        let mut sma = StableMemoryAllocator::init(0, &mut context).ok().unwrap();

        let mem_block_1 = sma.allocate(1000, &mut context).ok().unwrap();
        let mem_block_2 = sma.allocate(200, &mut context).ok().unwrap();
        let mem_block_3 = sma.allocate(12345, &mut context).ok().unwrap();
        let mem_block_4 = sma.allocate(65636, &mut context).ok().unwrap();
        let mem_block_5 = sma.allocate(123, &mut context).ok().unwrap();

        assert!(
            mem_block_1.ptr != mem_block_2.ptr
                && mem_block_2.ptr != mem_block_3.ptr
                && mem_block_3.ptr != mem_block_4.ptr
                && mem_block_4.ptr != mem_block_5.ptr,
            "allocate worked wrong"
        );
        assert!(
            mem_block_1.read_bytes(0, &mut [0; 1000], &context).is_ok(),
            "should be able to read first 1"
        );
        assert!(
            mem_block_2.read_bytes(0, &mut [0; 200], &context).is_ok(),
            "should be able to read second 1"
        );
        assert!(
            mem_block_3.read_bytes(0, &mut [0; 12345], &context).is_ok(),
            "should be able to read third 1"
        );
        assert!(
            mem_block_4.read_bytes(0, &mut [0; 65636], &context).is_ok(),
            "should be able to read forth 1"
        );
        assert!(
            mem_block_5.read_bytes(0, &mut [0; 123], &context).is_ok(),
            "should be able to read fifth 1"
        );

        sma.deallocate(mem_block_3.ptr, &mut context);
        sma.deallocate(mem_block_5.ptr, &mut context);
        sma.deallocate(mem_block_1.ptr, &mut context);
        sma.deallocate(mem_block_2.ptr, &mut context);
        sma.deallocate(mem_block_4.ptr, &mut context);

        assert_eq!(
            sma.segregation_size_classes
                .iter()
                .filter(|&&it| it != EMPTY_PTR)
                .count(),
            1,
            "there should be only one large deallocated mem block"
        );
    }

    #[test]
    fn reallocate_works_fine() {
        let mut context = TestMemContext::default();
        let mut sma = StableMemoryAllocator::init(0, &mut context).ok().unwrap();

        let mut mem_block_1 = sma.allocate(1000, &mut context).ok().unwrap();
        let mut mem_block_2 = sma.allocate(200, &mut context).ok().unwrap();
        let mut mem_block_3 = sma.allocate(2000, &mut context).ok().unwrap();

        let data = [b't', b'e', b's', b't'];
        mem_block_1
            .write_bytes(0, &data, &mut context)
            .ok()
            .unwrap();

        sma.deallocate(mem_block_2.ptr, &mut context);

        let mem_block_1 = sma
            .reallocate(mem_block_1.ptr, 1164, &mut context)
            .ok()
            .unwrap();

        let mut data_1 = [0u8; 4];

        mem_block_1
            .read_bytes(0, &mut data_1, &context)
            .ok()
            .unwrap();

        assert_eq!(data, data_1, "data changed across reallocations");

        let mem_block_1 = sma
            .reallocate(mem_block_1.ptr, 2000, &mut context)
            .ok()
            .unwrap();

        mem_block_1
            .read_bytes(0, &mut data_1, &context)
            .ok()
            .unwrap();

        assert_eq!(data, data_1, "data changed across reallocations");
    }
}
*/
