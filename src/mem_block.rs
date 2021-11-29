use crate::mem_context::MemContext;
use crate::types::{Word, EMPTY_WORD, PAGE_SIZE_BYTES};
use std::cmp::min_by;
use std::marker::PhantomData;
use std::mem::size_of;

pub const MEM_BLOCK_SIZE_BYTES: usize = size_of::<usize>();
pub const MEM_BLOCK_USED_SIZE_BYTES: usize = 1;
pub const MEM_BLOCK_OVERHEAD_BYTES: usize = (MEM_BLOCK_SIZE_BYTES + MEM_BLOCK_USED_SIZE_BYTES) * 2;
pub const MIN_MEM_BLOCK_SIZE_BYTES: usize = (MEM_BLOCK_OVERHEAD_BYTES + size_of::<Word>()) * 2;

#[derive(Clone, Copy)]
pub struct MemBlock<T: MemContext + Clone> {
    pub offset: Word,
    pub size: usize,
    pub allocated: bool,
    prev_free: Word,
    next_free: Word,
    pub(crate) marker: PhantomData<T>,
}

pub enum MemBlockSide {
    Start,
    End,
}

impl<T: MemContext + Clone> MemBlock<T> {
    pub fn read_content(&self, context: &T) -> Vec<u8> {
        if !self.allocated {
            unreachable!();
        }

        let mut buf = vec![0; self.size];
        context.read(self.offset + MEM_BLOCK_OVERHEAD_BYTES as Word, &mut buf);

        buf
    }

    pub fn write_content(&self, content: &[u8], context: &mut T) -> bool {
        if !self.allocated {
            unreachable!();
        }

        if content.len() > self.size {
            return false;
        }

        context.write(self.offset + MEM_BLOCK_OVERHEAD_BYTES as Word, content);

        true
    }

    pub fn set_allocated(&mut self, allocated: bool, context: &mut T) {
        if self.allocated == allocated {
            return;
        }

        self.allocated = allocated;
        let allocated_buf = if self.allocated { [1u8] } else { [0u8] };

        context.write(self.offset + MEM_BLOCK_SIZE_BYTES as Word, &allocated_buf);
    }

    pub fn set_prev_free(&mut self, prev_free: Word, context: &mut T) -> Word {
        if self.allocated {
            unreachable!();
        }

        let cur_prev_free = self.prev_free;
        self.prev_free = prev_free;
        let buf = prev_free.to_le_bytes();
        context.write(self.offset + MEM_BLOCK_OVERHEAD_BYTES as Word, &buf);

        cur_prev_free
    }

    pub fn get_prev_free(&self) -> Word {
        if self.allocated {
            unreachable!();
        }

        self.prev_free
    }

    pub fn set_next_free(&mut self, next_free: Word, context: &mut T) -> Word {
        if self.allocated {
            unreachable!();
        }

        let cur_next_free = self.next_free;
        self.next_free = next_free;
        let buf = next_free.to_le_bytes();
        context.write(
            self.offset + (MEM_BLOCK_OVERHEAD_BYTES + size_of::<Word>()) as Word,
            &buf,
        );

        cur_next_free
    }

    pub fn get_next_free(&self) -> Word {
        if self.allocated {
            unreachable!();
        }

        self.next_free
    }

    pub fn write_free_at(
        offset: Word,
        size: usize,
        prev: Word,
        next: Word,
        context: &mut T,
    ) -> MemBlock<T> {
        let mut open = vec![];
        open.extend(size.to_le_bytes());
        open.push(0);
        open.extend(prev.to_le_bytes());
        open.extend(next.to_le_bytes());

        let mut close = vec![];
        close.extend(size.to_le_bytes());
        close.push(0);

        context.write(offset, &open);
        context.write(
            offset + MEM_BLOCK_OVERHEAD_BYTES as Word + size as Word,
            &close,
        );

        MemBlock {
            offset,
            size,
            prev_free: prev,
            next_free: next,
            allocated: false,
            marker: PhantomData,
        }
    }

    // offset should always point to boundary (use `side` param to specify):
    //
    //  v here
    // [size, used, data..., size, used]
    //
    //                                 v or here
    // [size, used, data..., size, used]
    pub fn read_at(mut offset: Word, side: MemBlockSide, context: &mut T) -> Option<MemBlock<T>> {
        if offset >= context.size_pages() * PAGE_SIZE_BYTES as Word {
            return None;
        }

        if matches!(side, MemBlockSide::End) {
            offset -= MEM_BLOCK_OVERHEAD_BYTES as Word;
        }

        // read data stored under the pointer
        let mut size_buf = [0u8; MEM_BLOCK_SIZE_BYTES];
        context.read(offset, &mut size_buf);
        let size = usize::from_le_bytes(size_buf);

        let mut used_buf = [0u8; MEM_BLOCK_USED_SIZE_BYTES];
        context.read(offset + MEM_BLOCK_SIZE_BYTES as Word, &mut used_buf);
        let used = if used_buf[0] == 0 {
            false
        } else if used_buf[0] == 1 {
            true
        } else {
            return None;
        };

        if matches!(side, MemBlockSide::End) {
            // if that data was at the end - read from the start and compare
            offset -= (size + MEM_BLOCK_OVERHEAD_BYTES) as Word;

            let size_end = size;
            let used_end = used;

            let mut size_buf = [0u8; MEM_BLOCK_SIZE_BYTES];
            context.read(offset, &mut size_buf);
            let size_start = usize::from_le_bytes(size_buf);

            let mut used_buf = [0u8; MEM_BLOCK_USED_SIZE_BYTES];
            context.read(offset + MEM_BLOCK_SIZE_BYTES as Word, &mut used_buf);
            let used_start = if used_buf[0] == 0 {
                false
            } else if used_buf[0] == 1 {
                true
            } else {
                return None;
            };

            if size_start != size_end || used_start != used_end {
                return None;
            }
        } else {
            // if that data was at the start - read from the end and compare
            let size_start = size;
            let used_start = used;

            let mut size_buf = [0u8; MEM_BLOCK_SIZE_BYTES];
            context.read(
                offset + (MEM_BLOCK_OVERHEAD_BYTES + size_start) as Word,
                &mut size_buf,
            );
            let size_end = usize::from_le_bytes(size_buf);

            let mut used_buf = [0u8; MEM_BLOCK_USED_SIZE_BYTES];
            context.read(
                offset + (MEM_BLOCK_OVERHEAD_BYTES + size_start + MEM_BLOCK_SIZE_BYTES) as Word,
                &mut used_buf,
            );
            let used_end = if used_buf[0] == 0 {
                false
            } else if used_buf[0] == 1 {
                true
            } else {
                return None;
            };

            if size_start != size_end || used_start != used_end {
                return None;
            }
        }

        if used {
            Some(MemBlock {
                offset,
                size,
                allocated: used,
                prev_free: EMPTY_WORD,
                next_free: EMPTY_WORD,
                marker: PhantomData,
            })
        } else {
            let mut prev_buf = [0u8; size_of::<Word>()];
            context.read(offset + MEM_BLOCK_OVERHEAD_BYTES as Word, &mut prev_buf);
            let prev = Word::from_le_bytes(prev_buf);

            let mut next_buf = [0u8; size_of::<Word>()];
            context.read(
                offset + MEM_BLOCK_OVERHEAD_BYTES as Word + size_of::<Word>() as Word,
                &mut next_buf,
            );
            let next = Word::from_le_bytes(next_buf);

            Some(MemBlock {
                offset,
                size,
                allocated: used,
                prev_free: prev,
                next_free: next,
                marker: PhantomData,
            })
        }
    }

    // splits a block into two: of size=[size] and of size=[remainder]
    // should only be invoked for blocks which size remainder is bigger than MIN_MEM_BLOCK_SIZE
    pub fn split_mem_block(mut self, size: usize, context: &mut T) -> (MemBlock<T>, MemBlock<T>) {
        let new_free_block = MemBlock::write_free_at(
            self.offset + (MEM_BLOCK_OVERHEAD_BYTES * 2 + size) as Word,
            self.size - size - MEM_BLOCK_OVERHEAD_BYTES * 2,
            EMPTY_WORD,
            EMPTY_WORD,
            context,
        );

        let old_mem_block = MemBlock::write_free_at(
            self.offset,
            size,
            self.get_prev_free(),
            self.get_next_free(),
            context,
        );

        (old_mem_block, new_free_block)
    }

    // merges two free mem blocks together returning a new one
    // both blocks should be free!
    pub fn merge_with(mut self, other: MemBlock<T>, context: &mut T) -> MemBlock<T> {
        let (prev, next) = if self.offset < other.offset {
            (self, other)
        } else {
            (other, self)
        };

        MemBlock::write_free_at(
            prev.offset,
            prev.size + next.size + MEM_BLOCK_OVERHEAD_BYTES * 2,
            EMPTY_WORD,
            EMPTY_WORD,
            context,
        )
    }
}
