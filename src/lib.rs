use crate::mem::allocator::StableMemoryAllocator;
use crate::primitive::s_unsafe_cell::SUnsafeCell;
use ic_cdk::print;
use primitive::s_slice::SSlice;

mod benchmarks;
pub mod collections;
pub mod macros;
pub mod mem;
pub mod primitive;
pub mod utils;

pub use crate::utils::mem_context::{stable, OutOfMemory, PAGE_SIZE_BYTES};
pub use crate::utils::vars::{init_vars, reinit_vars, store_vars};
use crate::utils::MemMetrics;

static mut STABLE_MEMORY_ALLOCATOR: Option<SSlice<StableMemoryAllocator>> = None;

pub fn init_allocator(offset: u64) {
    unsafe {
        if STABLE_MEMORY_ALLOCATOR.is_none() {
            let allocator = SSlice::<StableMemoryAllocator>::init(offset);

            STABLE_MEMORY_ALLOCATOR = Some(allocator)
        } else {
            unreachable!("StableMemoryAllocator can only be initialized once");
        }
    }
}

pub fn reinit_allocator(offset: u64) {
    unsafe {
        if STABLE_MEMORY_ALLOCATOR.is_none() {
            let allocator = SSlice::<StableMemoryAllocator>::reinit(offset)
                .expect("Unable to reinit StableMemoryAllocator");

            STABLE_MEMORY_ALLOCATOR = Some(allocator)
        } else {
            unreachable!("StableMemoryAllocator can only be initialized once")
        }
    }
}

fn get_allocator() -> SSlice<StableMemoryAllocator> {
    unsafe { STABLE_MEMORY_ALLOCATOR.as_ref().unwrap().clone() }
}

pub fn allocate<T>(size: usize) -> SSlice<T> {
    get_allocator().allocate(size)
}

pub fn deallocate<T>(membox: SSlice<T>) {
    get_allocator().deallocate(membox)
}

pub fn reallocate<T>(membox: SSlice<T>, new_size: usize) -> SSlice<T> {
    get_allocator().reallocate(membox, new_size)
}

pub fn set_max_allocation_pages(pages: u32) {
    get_allocator().set_max_allocation_pages(pages)
}

pub fn get_max_allocation_pages() -> u32 {
    get_allocator().get_max_allocation_pages()
}

pub fn set_max_grow_pages(pages: u64) {
    get_allocator().set_max_grow_pages(pages)
}

pub fn get_max_grow_pages() -> u64 {
    get_allocator().get_max_grow_pages()
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

pub fn _set_custom_data_ptr(idx: usize, data_ptr: u64) {
    get_allocator().set_custom_data_ptr(idx, data_ptr)
}

pub fn get_mem_metrics() -> MemMetrics {
    MemMetrics {
        available: stable::size_pages() * PAGE_SIZE_BYTES as u64,
        free: get_free_size(),
        allocated: get_allocated_size(),
    }
}

pub fn _get_custom_data_ptr(idx: usize) -> u64 {
    get_allocator().get_custom_data_ptr(idx)
}

pub fn _debug_print_allocator() {
    print(format!("{:?}", get_allocator()))
}

pub fn stable_memory_init(should_grow: bool, allocator_pointer: u64) {
    if should_grow {
        stable::grow(1).expect("Out of memory (stable_memory_init)");
    }

    init_allocator(allocator_pointer);
    init_vars();
}

pub fn stable_memory_pre_upgrade() {
    store_vars();
}

pub fn stable_memory_post_upgrade(allocator_pointer: u64) {
    reinit_allocator(allocator_pointer);
    reinit_vars();
}
