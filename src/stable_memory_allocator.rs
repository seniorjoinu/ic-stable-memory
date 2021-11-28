use crate::mem_block::{
    MemBlock, MemBlockSide, MEM_BLOCK_OVERHEAD_BYTES, MIN_MEM_BLOCK_SIZE_BYTES,
};
use crate::mem_context::MemContext;
use crate::types::{
    CollectionDeclarationPtr, SMAError, SegregationClassPtr, Word, EMPTY_WORD, MAGIC,
    MAX_COLLECTION_DECLARATIONS, MAX_SEGREGATION_CLASSES, PAGE_SIZE_BYTES,
};
use crate::utils::fast_log2_32;
use std::marker::PhantomData;
use std::mem::size_of;

pub struct StableMemoryAllocator<T: MemContext> {
    pub segregation_size_classes: [SegregationClassPtr; MAX_SEGREGATION_CLASSES],
    pub collection_declarations: [CollectionDeclarationPtr; MAX_COLLECTION_DECLARATIONS],
    pub(crate) marker: PhantomData<T>,
    pub offset: Word,
}

// TODO: remove flush - replace with specialized functions

impl<T: MemContext + Clone> StableMemoryAllocator<T> {
    const SIZE: usize = (MAGIC.len()
        + MAX_SEGREGATION_CLASSES * size_of::<SegregationClassPtr>()
        + MAX_COLLECTION_DECLARATIONS * size_of::<CollectionDeclarationPtr>());

    pub fn allocate(&mut self, size: usize, context: &mut T) -> Result<MemBlock<T>, SMAError> {
        let mut mem_block = if let Some((appropriate_mem_block, seg_class_idx)) =
            self.find_appropriate_free_mem_block(size, context)
        {
            if appropriate_mem_block.size - size >= MIN_MEM_BLOCK_SIZE_BYTES {
                // split the block in two
                let (old_mem_block, mut new_free_block) =
                    appropriate_mem_block.split_mem_block(size, context);

                // then remove the old one from free list and add a new one to it
                self.remove_block_from_free_list(&old_mem_block, seg_class_idx, context);
                self.add_block_to_free_list(&mut new_free_block, context);

                old_mem_block
            } else {
                // remove the whole block from free list
                self.remove_block_from_free_list(&appropriate_mem_block, seg_class_idx, context);
                appropriate_mem_block
            }
        } else {
            // this block is not added to the free list yet, so we won't remove it from there
            // can return OOM error
            let big_mem_block = self.grow_and_create_new_free_block(size, context)?;

            // check if the block is too big and split
            if big_mem_block.size - size >= MIN_MEM_BLOCK_SIZE_BYTES {
                let (old_mem_block, mut new_free_block) =
                    big_mem_block.split_mem_block(size, context);

                self.add_block_to_free_list(&mut new_free_block, context);

                old_mem_block
            } else {
                big_mem_block
            }
        };

        mem_block.set_allocated(true, context);

        // TODO: remove
        self.flush(context)?;

        Ok(mem_block)
    }

    fn grow_and_create_new_free_block(
        &mut self,
        size: usize,
        context: &mut T,
    ) -> Result<MemBlock<T>, SMAError> {
        let offset = context.size_pages() * PAGE_SIZE_BYTES as Word;

        let mut size_need_pages = size / PAGE_SIZE_BYTES;
        if size % PAGE_SIZE_BYTES > 0 {
            size_need_pages += 1;
        }

        context
            .grow(size_need_pages as u64)
            .map_err(|_| SMAError::OutOfMemory)?;

        let mem_block = MemBlock::write_free_at(
            offset,
            size_need_pages * PAGE_SIZE_BYTES - (MEM_BLOCK_OVERHEAD_BYTES * 2),
            EMPTY_WORD,
            EMPTY_WORD,
            context,
        );

        Ok(mem_block)
    }

    fn remove_block_from_free_list(
        &mut self,
        mem_block: &MemBlock<T>,
        mem_block_seg_class_idx: usize,
        context: &mut T,
    ) {
        let prev_offset = mem_block.get_prev_free();
        let next_offset = mem_block.get_next_free();

        if prev_offset != EMPTY_WORD && next_offset != EMPTY_WORD {
            let mut prev = MemBlock::read_at(prev_offset, MemBlockSide::Start, context);
            let mut next = MemBlock::read_at(next_offset, MemBlockSide::Start, context);

            prev.set_next_free(next_offset, context);
            next.set_prev_free(prev_offset, context);
        } else if prev_offset != EMPTY_WORD {
            let mut prev = MemBlock::read_at(prev_offset, MemBlockSide::Start, context);
            prev.set_next_free(next_offset, context);
        } else if next_offset != EMPTY_WORD {
            let mut next = MemBlock::read_at(next_offset, MemBlockSide::Start, context);
            next.set_prev_free(prev_offset, context);
        } else {
            // appropriate is the only one in the class - delete the whole class
            // TODO: add persistence
            self.segregation_size_classes[mem_block_seg_class_idx] = EMPTY_WORD;
        }
    }

    fn add_block_to_free_list(&mut self, new_mem_block: &mut MemBlock<T>, context: &mut T) {
        let seg_class_idx = self.find_seg_class_idx(new_mem_block.size);

        // if there are no blocks in this class - just insert
        if self.segregation_size_classes[seg_class_idx] == EMPTY_WORD {
            // TODO: add persistence
            self.segregation_size_classes[seg_class_idx] = new_mem_block.offset;

            return;
        }

        // if there are some blocks - find a place for it, such as addr(prev) < addr(new) < addr(next)
        let mut cur_mem_block = MemBlock::read_at(
            self.segregation_size_classes[seg_class_idx],
            MemBlockSide::Start,
            context,
        );

        // TODO: remove
        if cur_mem_block.get_prev_free() != EMPTY_WORD {
            unreachable!();
        }

        // if the inserting block address is lesser than the first address in the free list - insert before
        if new_mem_block.offset < cur_mem_block.offset {
            self.segregation_size_classes[seg_class_idx] = new_mem_block.offset;
            cur_mem_block.set_prev_free(new_mem_block.offset, context);
            new_mem_block.set_next_free(cur_mem_block.offset, context);

            return;
        }

        // if there is only one mem block in the free list - insert after
        if cur_mem_block.get_next_free() == EMPTY_WORD {
            cur_mem_block.set_next_free(new_mem_block.offset, context);
            new_mem_block.set_prev_free(cur_mem_block.offset, context);

            return;
        }

        // otherwise - try to find a place in between or at the end of the free list
        let mut next_mem_block =
            MemBlock::read_at(cur_mem_block.get_next_free(), MemBlockSide::Start, context);

        loop {
            if new_mem_block.offset > cur_mem_block.offset
                && new_mem_block.offset < next_mem_block.offset
            {
                cur_mem_block.set_next_free(new_mem_block.offset, context);
                new_mem_block.set_prev_free(cur_mem_block.offset, context);

                next_mem_block.set_prev_free(new_mem_block.offset, context);
                new_mem_block.set_next_free(next_mem_block.offset, context);

                return;
            }

            if next_mem_block.get_next_free() == EMPTY_WORD {
                next_mem_block.set_next_free(new_mem_block.offset, context);
                new_mem_block.set_prev_free(next_mem_block.offset, context);

                return;
            }

            cur_mem_block = next_mem_block;
            next_mem_block =
                MemBlock::read_at(cur_mem_block.get_next_free(), MemBlockSide::Start, context);
        }
    }

    // find a free block that has a size bigger than the provided size, but optimal (not too big)
    // if there is none - return None
    fn find_appropriate_free_mem_block(
        &self,
        size: usize,
        context: &mut T,
    ) -> Option<(MemBlock<T>, usize)> {
        let initial_seg_class_idx = self.find_seg_class_idx(size);
        let mut result: Option<(MemBlock<T>, usize)> = None;

        // for each segregation class, starting from the most appropriate (closer)
        for seg_class_idx in initial_seg_class_idx..MAX_SEGREGATION_CLASSES {
            // skip if there is no free blocks at all
            if self.segregation_size_classes[seg_class_idx] == EMPTY_WORD {
                continue;
            }

            // try to find at least one appropriate (size is bigger) free block
            let mut appropriate_found = false;
            let mut appropriate_free_mem_block = MemBlock::read_at(
                self.segregation_size_classes[seg_class_idx],
                MemBlockSide::Start,
                context,
            );
            let mut next_free = appropriate_free_mem_block.get_next_free();

            loop {
                if appropriate_free_mem_block.size < size {
                    if next_free == EMPTY_WORD {
                        break;
                    }

                    appropriate_free_mem_block =
                        MemBlock::read_at(next_free, MemBlockSide::Start, context);
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
                if next_free == EMPTY_WORD {
                    break;
                }

                let mut next_free_mem_block =
                    MemBlock::read_at(next_free, MemBlockSide::Start, context);

                if next_free_mem_block.size < size {
                    next_free = next_free_mem_block.get_next_free();

                    if next_free == EMPTY_WORD {
                        break;
                    }

                    continue;
                }

                if appropriate_free_mem_block.size - size > next_free_mem_block.size - size {
                    appropriate_free_mem_block = next_free_mem_block.clone();
                }

                next_free = next_free_mem_block.get_next_free();

                if next_free == EMPTY_WORD {
                    break;
                }
            }

            // return the one closest to provided size
            result = Some((appropriate_free_mem_block, seg_class_idx));
        }

        result
    }

    pub fn init(offset: Word, context: &mut T) -> Result<Self, SMAError> {
        Self::init_grow_if_need(offset, context)?;

        let mut this = StableMemoryAllocator {
            segregation_size_classes: [SegregationClassPtr::default(); MAX_SEGREGATION_CLASSES],
            collection_declarations: [CollectionDeclarationPtr::default();
                MAX_COLLECTION_DECLARATIONS],
            marker: PhantomData,
            offset,
        };

        let initial_mem_block =
            this.init_first_free_mem_block(offset + Self::SIZE as Word, context)?;
        this.flush(context)?;

        Ok(this)
    }

    pub fn reinit(mut offset: Word, context: &T) -> Result<Self, SMAError> {
        // checking magic sequence
        let mut magic_buf = [0u8; MAGIC.len()];
        context.read(offset, &mut magic_buf);

        if magic_buf != MAGIC {
            return Err(SMAError::InvalidMagicSequence);
        }

        offset += MAGIC.len() as Word;

        // reading segregation classes
        let mut segregation_classes_buf =
            [0u8; MAX_SEGREGATION_CLASSES * size_of::<SegregationClassPtr>()];
        context.read(offset, &mut segregation_classes_buf);

        let mut segregation_size_classes =
            [SegregationClassPtr::default(); MAX_SEGREGATION_CLASSES];
        segregation_classes_buf
            .chunks_exact(size_of::<SegregationClassPtr>())
            .enumerate()
            .for_each(|(idx, it)| {
                let mut buf = [0u8; size_of::<SegregationClassPtr>()];
                buf.copy_from_slice(it);

                segregation_size_classes[idx] = SegregationClassPtr::from_le_bytes(buf);
            });

        // reading collection declarations
        offset += (MAX_SEGREGATION_CLASSES * size_of::<SegregationClassPtr>()) as Word;

        let mut collection_declarations_buf =
            [0u8; MAX_COLLECTION_DECLARATIONS * size_of::<CollectionDeclarationPtr>()];
        context.read(offset, &mut collection_declarations_buf);

        let mut collection_declarations =
            [CollectionDeclarationPtr::default(); MAX_COLLECTION_DECLARATIONS];
        collection_declarations_buf
            .chunks_exact(size_of::<CollectionDeclarationPtr>())
            .enumerate()
            .for_each(|(idx, it)| {
                let mut buf = [0u8; size_of::<CollectionDeclarationPtr>()];
                buf.copy_from_slice(it);

                collection_declarations[idx] = CollectionDeclarationPtr::from_le_bytes(buf);
            });

        // returning
        Ok(Self {
            collection_declarations,
            segregation_size_classes,
            marker: PhantomData,
            offset,
        })
    }

    // TODO: rewrite using low-level functions
    fn init_first_free_mem_block(
        &mut self,
        offset: Word,
        context: &mut T,
    ) -> Result<MemBlock<T>, SMAError> {
        let grown_bytes = context.size_pages() * PAGE_SIZE_BYTES as Word;

        if offset > grown_bytes {
            unreachable!();
        }

        let mem_block_size_bytes = (grown_bytes - offset) as usize - MEM_BLOCK_OVERHEAD_BYTES * 2;
        if mem_block_size_bytes < MIN_MEM_BLOCK_SIZE_BYTES {
            context.grow(1).map_err(|_| SMAError::OutOfMemory)?;
        }

        let seg_idx = self.find_seg_class_idx(mem_block_size_bytes);

        let mem_block = MemBlock::write_free_at(offset, mem_block_size_bytes, 0, 0, context);
        self.segregation_size_classes[seg_idx] = offset;

        Ok(mem_block)
    }

    fn find_seg_class_idx(&self, block_size_bytes: usize) -> usize {
        let log = fast_log2_32(block_size_bytes as u32);

        if log > 3 {
            log as usize - 4
        } else {
            0
        }
    }

    fn flush(&mut self, context: &mut T) -> Result<(), SMAError> {
        let mut payload = vec![];

        payload.extend(MAGIC);

        for i in self.segregation_size_classes {
            payload.extend(i.to_le_bytes());
        }

        for i in self.collection_declarations {
            payload.extend(i.to_le_bytes());
        }

        context.write(self.offset, &payload);

        Ok(())
    }

    fn is_magic(offset: Word, context: &T) -> Option<bool> {
        if !context.offset_exists(offset) {
            return None;
        }

        let mut buf = [0u8; 4];
        context.read(offset, &mut buf);

        Some(buf == MAGIC)
    }

    fn init_grow_if_need(offset: Word, context: &mut T) -> Result<(), SMAError> {
        let size_need_bytes = offset
            + MAGIC.len() as Word
            + MAX_SEGREGATION_CLASSES as Word * size_of::<SegregationClassPtr>() as Word
            + MAX_COLLECTION_DECLARATIONS as Word * size_of::<CollectionDeclarationPtr>() as Word;

        let mut size_need_pages = size_need_bytes / PAGE_SIZE_BYTES as Word;
        if size_need_bytes % PAGE_SIZE_BYTES as Word > 0 {
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
}

#[cfg(test)]
mod tests {
    use crate::mem_block::{MemBlock, MemBlockSide, MEM_BLOCK_OVERHEAD_BYTES};
    use crate::mem_context::{MemContext, TestMemContext};
    use crate::stable_memory_allocator::StableMemoryAllocator;
    use crate::types::{Word, EMPTY_WORD, PAGE_SIZE_BYTES};

    #[test]
    fn init_works_fine() {
        let mut context = TestMemContext::default();
        let allocator = StableMemoryAllocator::init(0, &mut context).ok().unwrap();

        let initial_free_mem_block = MemBlock::read_at(
            StableMemoryAllocator::<TestMemContext>::SIZE as Word,
            MemBlockSide::Start,
            &mut context,
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
            EMPTY_WORD,
            "Initial mem block should contain no next block"
        );
        assert_eq!(
            initial_free_mem_block.get_prev_free(),
            EMPTY_WORD,
            "Initial mem block should contain no prev block"
        );
        assert_eq!(
            initial_free_mem_block.offset
                + (initial_free_mem_block.size + MEM_BLOCK_OVERHEAD_BYTES * 2) as Word,
            context.size_pages() * PAGE_SIZE_BYTES as Word,
            "Invalid total size"
        );
        assert_eq!(
            initial_free_mem_block.offset,
            StableMemoryAllocator::<TestMemContext>::SIZE as Word,
            "Invalid SMA size"
        );

        let allocator_re = StableMemoryAllocator::reinit(0, &context).ok().unwrap();

        assert_eq!(
            allocator.segregation_size_classes, allocator_re.segregation_size_classes,
            "Segregation size classes mismatch"
        );
        assert_eq!(
            allocator.collection_declarations, allocator_re.collection_declarations,
            "Collection declarations mismatch"
        );
    }
}
