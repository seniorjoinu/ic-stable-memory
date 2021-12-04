use crate::mem_context::MemContext;
use crate::types::{EMPTY_PTR, PAGE_SIZE_BYTES};
use std::marker::PhantomData;
use std::mem::size_of;

pub const MEM_BLOCK_SIZE_BYTES: usize = size_of::<u64>();
pub const MEM_BLOCK_USED_SIZE_BYTES: usize = 1;
pub const MEM_BLOCK_OVERHEAD_BYTES: usize = (MEM_BLOCK_SIZE_BYTES + MEM_BLOCK_USED_SIZE_BYTES) * 2;
pub const MIN_MEM_BLOCK_SIZE_BYTES: usize = (MEM_BLOCK_OVERHEAD_BYTES + size_of::<u64>()) * 2;
pub const ALLOCATED: u8 = 228;
pub const FREE: u8 = 227;

#[derive(Clone, Copy)]
pub struct MemBlock<T: MemContext + Clone> {
    pub offset: u64,
    pub size: u64,
    pub allocated: bool,
    prev_free: u64,
    next_free: u64,
    pub(crate) marker: PhantomData<T>,
}

pub enum MemBlockSide {
    Start,
    End,
}

impl<T: MemContext + Clone> MemBlock<T> {
    pub fn read_content(&self, mut offset: u64, buf: &mut [u8], context: &T) -> bool {
        if !self.allocated {
            unreachable!();
        }

        if offset + buf.len() as u64 > self.size {
            return false;
        }

        offset += self.offset + MEM_BLOCK_OVERHEAD_BYTES as u64;

        context.read(offset, buf);

        true
    }

    pub fn write_content(&self, mut offset: u64, buf: &[u8], context: &mut T) -> bool {
        if !self.allocated {
            unreachable!();
        }

        if offset + buf.len() as u64 > self.size {
            return false;
        }

        offset += self.offset + MEM_BLOCK_OVERHEAD_BYTES as u64;

        context.write(offset, buf);

        true
    }

    pub fn set_allocated(&mut self, allocated: bool, context: &mut T) {
        if self.allocated == allocated {
            return;
        }

        self.allocated = allocated;
        let allocated_buf = if self.allocated { [ALLOCATED] } else { [FREE] };

        context.write(self.offset + MEM_BLOCK_SIZE_BYTES as u64, &allocated_buf);
        context.write(
            self.offset + self.size + (MEM_BLOCK_OVERHEAD_BYTES + MEM_BLOCK_SIZE_BYTES) as u64,
            &allocated_buf,
        );

        if !allocated {
            let empty_u64_ptr = EMPTY_PTR.to_le_bytes();

            context.write(
                self.offset + MEM_BLOCK_OVERHEAD_BYTES as u64,
                &empty_u64_ptr,
            );
            context.write(
                self.offset + (MEM_BLOCK_OVERHEAD_BYTES + size_of::<u64>()) as u64,
                &empty_u64_ptr,
            );
        }
    }

    pub fn set_prev_free(&mut self, prev_free: u64, context: &mut T) -> u64 {
        if self.allocated {
            unreachable!();
        }

        let cur_prev_free = self.prev_free;
        self.prev_free = prev_free;
        let buf = prev_free.to_le_bytes();
        context.write(self.offset + MEM_BLOCK_OVERHEAD_BYTES as u64, &buf);

        cur_prev_free
    }

    pub fn get_prev_free(&self) -> u64 {
        if self.allocated {
            unreachable!();
        }

        self.prev_free
    }

    pub fn set_next_free(&mut self, next_free: u64, context: &mut T) -> u64 {
        if self.allocated {
            unreachable!();
        }

        let cur_next_free = self.next_free;
        self.next_free = next_free;
        let buf = next_free.to_le_bytes();
        context.write(
            self.offset + (MEM_BLOCK_OVERHEAD_BYTES + size_of::<u64>()) as u64,
            &buf,
        );

        cur_next_free
    }

    pub fn get_next_free(&self) -> u64 {
        if self.allocated {
            unreachable!();
        }

        self.next_free
    }

    pub fn erase(self, context: &mut T) {
        let empty_overhead = [0; MEM_BLOCK_OVERHEAD_BYTES];

        context.write(self.offset, &empty_overhead);
        context.write(
            self.offset + self.size + MEM_BLOCK_OVERHEAD_BYTES as u64,
            &empty_overhead,
        );
    }

    pub fn write_free_at(
        offset: u64,
        size: u64,
        prev: u64,
        next: u64,
        context: &mut T,
    ) -> MemBlock<T> {
        let mut open = vec![];
        open.extend(size.to_le_bytes());
        open.push(FREE);
        open.extend(prev.to_le_bytes());
        open.extend(next.to_le_bytes());

        let mut close = vec![];
        close.extend(size.to_le_bytes());
        close.push(FREE);

        context.write(offset, &open);
        context.write(offset + size + MEM_BLOCK_OVERHEAD_BYTES as u64, &close);

        let empty_u64_ptr = EMPTY_PTR.to_le_bytes();
        context.write(offset + MEM_BLOCK_OVERHEAD_BYTES as u64, &empty_u64_ptr);
        context.write(
            offset + (MEM_BLOCK_OVERHEAD_BYTES + size_of::<u64>()) as u64,
            &empty_u64_ptr,
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
    pub fn read_at(mut offset: u64, side: MemBlockSide, context: &T) -> Option<MemBlock<T>> {
        if offset >= context.size_pages() * PAGE_SIZE_BYTES as u64 {
            return None;
        }

        if matches!(side, MemBlockSide::End) {
            offset -= MEM_BLOCK_OVERHEAD_BYTES as u64;
        }

        // read data stored under the pointer
        let mut size_buf = [0u8; MEM_BLOCK_SIZE_BYTES];
        context.read(offset, &mut size_buf);
        let size = u64::from_le_bytes(size_buf);

        if size == 0 {
            return None;
        }

        let mut allocated_buf = [0u8; MEM_BLOCK_USED_SIZE_BYTES];
        context.read(offset + MEM_BLOCK_SIZE_BYTES as u64, &mut allocated_buf);
        let allocated = if allocated_buf[0] == FREE {
            false
        } else if allocated_buf[0] == ALLOCATED {
            true
        } else {
            return None;
        };

        if matches!(side, MemBlockSide::End) {
            // if that data was at the end - read from the start and compare
            offset -= size + MEM_BLOCK_OVERHEAD_BYTES as u64;

            let size_end = size;
            let allocated_end = allocated;

            let mut size_buf = [0u8; MEM_BLOCK_SIZE_BYTES];
            context.read(offset, &mut size_buf);
            let size_start = u64::from_le_bytes(size_buf);

            let mut allocated_buf = [0u8; MEM_BLOCK_USED_SIZE_BYTES];
            context.read(offset + MEM_BLOCK_SIZE_BYTES as u64, &mut allocated_buf);
            let allocated_start = if allocated_buf[0] == FREE {
                false
            } else if allocated_buf[0] == ALLOCATED {
                true
            } else {
                return None;
            };

            if size_start != size_end || allocated_start != allocated_end {
                return None;
            }
        } else {
            // if that data was at the start - read from the end and compare
            let size_start = size;
            let allocated_start = allocated;

            let mut size_buf = [0u8; MEM_BLOCK_SIZE_BYTES];
            context.read(
                offset + size_start + MEM_BLOCK_OVERHEAD_BYTES as u64,
                &mut size_buf,
            );
            let size_end = u64::from_le_bytes(size_buf);

            let mut allocated_buf = [0u8; MEM_BLOCK_USED_SIZE_BYTES];
            context.read(
                offset + size_start + (MEM_BLOCK_OVERHEAD_BYTES + MEM_BLOCK_SIZE_BYTES) as u64,
                &mut allocated_buf,
            );
            let allocated_end = if allocated_buf[0] == FREE {
                false
            } else if allocated_buf[0] == ALLOCATED {
                true
            } else {
                return None;
            };

            if size_start != size_end || allocated_start != allocated_end {
                return None;
            }
        }

        if allocated {
            Some(MemBlock {
                offset,
                size,
                allocated,
                prev_free: EMPTY_PTR,
                next_free: EMPTY_PTR,
                marker: PhantomData,
            })
        } else {
            let mut prev_buf = [0u8; size_of::<u64>()];
            context.read(offset + MEM_BLOCK_OVERHEAD_BYTES as u64, &mut prev_buf);
            let prev = u64::from_le_bytes(prev_buf);

            let mut next_buf = [0u8; size_of::<u64>()];
            context.read(
                offset + MEM_BLOCK_OVERHEAD_BYTES as u64 + size_of::<u64>() as u64,
                &mut next_buf,
            );
            let next = u64::from_le_bytes(next_buf);

            Some(MemBlock {
                offset,
                size,
                allocated,
                prev_free: prev,
                next_free: next,
                marker: PhantomData,
            })
        }
    }

    // splits a block into two: of size=[size] and of size=[remainder]
    // should only be invoked for blocks which size remainder is bigger than MIN_MEM_BLOCK_SIZE
    pub fn split_mem_block(self, size: u64, context: &mut T) -> (MemBlock<T>, MemBlock<T>) {
        let old_mem_block = MemBlock::write_free_at(
            self.offset,
            size,
            self.get_prev_free(),
            self.get_next_free(),
            context,
        );

        let new_free_block = MemBlock::write_free_at(
            self.offset + size + (MEM_BLOCK_OVERHEAD_BYTES * 2) as u64,
            self.size - size - (MEM_BLOCK_OVERHEAD_BYTES * 2) as u64,
            EMPTY_PTR,
            EMPTY_PTR,
            context,
        );

        (old_mem_block, new_free_block)
    }

    // merges two free mem blocks together returning a new one
    // both blocks should be free!
    pub fn merge_with(self, other: MemBlock<T>, context: &mut T) -> MemBlock<T> {
        let (prev, next) = if self.offset < other.offset {
            (self, other)
        } else {
            (other, self)
        };

        let new_offset = prev.offset;
        let new_size = prev.size + next.size + (MEM_BLOCK_OVERHEAD_BYTES * 2) as u64;

        prev.erase(context);
        next.erase(context);

        MemBlock::write_free_at(new_offset, new_size, EMPTY_PTR, EMPTY_PTR, context)
    }
}
