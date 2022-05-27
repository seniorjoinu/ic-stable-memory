#![feature(auto_traits, negative_impls)]
#![feature(local_key_cell_methods)]

use crate::mem::allocator::StableMemoryAllocator;
use primitive::raw_s_cell::RawSCell;
use utils::mem_context::OutOfMemory;

pub mod collections;
pub mod mem;
pub mod primitive;
pub mod utils;

static mut STABLE_MEMORY_ALLOCATOR: Option<RawSCell<StableMemoryAllocator>> = None;

pub fn init_allocator(offset: u64) {
    unsafe {
        if STABLE_MEMORY_ALLOCATOR.is_none() {
            let allocator = RawSCell::<StableMemoryAllocator>::init(offset);

            STABLE_MEMORY_ALLOCATOR = Some(allocator)
        } else {
            unreachable!("StableMemoryAllocator can only be initialized once");
        }
    }
}

pub fn reinit_allocator(offset: u64) {
    unsafe {
        if STABLE_MEMORY_ALLOCATOR.is_none() {
            let allocator = RawSCell::<StableMemoryAllocator>::reinit(offset)
                .expect("Unable to reinit StableMemoryAllocator");

            STABLE_MEMORY_ALLOCATOR = Some(allocator)
        } else {
            unreachable!("StableMemoryAllocator can only be initialized once")
        }
    }
}

fn get_allocator() -> RawSCell<StableMemoryAllocator> {
    unsafe { STABLE_MEMORY_ALLOCATOR.as_ref().unwrap().clone() }
}

pub fn allocate<T>(size: usize) -> Result<RawSCell<T>, OutOfMemory> {
    get_allocator().allocate(size)
}

pub fn deallocate<T>(membox: RawSCell<T>) {
    get_allocator().deallocate(membox)
}

pub fn reallocate<T>(membox: RawSCell<T>, new_size: usize) -> Result<RawSCell<T>, OutOfMemory> {
    get_allocator().reallocate(membox, new_size)
}

pub fn reset() {
    get_allocator().reset()
}

pub fn get_allocated_size() -> u64 {
    get_allocator().get_allocated_size()
}

pub fn get_free_size() -> u64 {
    get_allocator().get_free_size()
}
