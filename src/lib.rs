#![feature(thread_local)]
#![feature(auto_traits)]
#![feature(negative_impls)]
#![feature(const_ptr_read)]
#![feature(generic_const_exprs)]
#![feature(const_trait_impl)]

use crate::mem::allocator::StableMemoryAllocator;
use crate::primitive::s_unsafe_cell::SUnsafeCell;
use mem::s_slice::SSlice;
use std::cell::RefCell;

mod benchmarks;
pub mod collections;
pub mod macros;
pub mod mem;
pub mod primitive;
pub mod utils;

pub use crate::utils::mem_context::{stable, OutOfMemory, PAGE_SIZE_BYTES};
use crate::utils::vars::deinit_vars;
pub use crate::utils::vars::{init_vars, reinit_vars};
use crate::utils::{isoprint, MemMetrics};

#[thread_local]
static STABLE_MEMORY_ALLOCATOR: RefCell<Option<StableMemoryAllocator>> = RefCell::new(None);

pub fn init_allocator(offset: u64) {
    if STABLE_MEMORY_ALLOCATOR.borrow().is_none() {
        let allocator = unsafe { StableMemoryAllocator::init(offset) };

        *STABLE_MEMORY_ALLOCATOR.borrow_mut() = Some(allocator);
    } else {
        unreachable!("StableMemoryAllocator can only be initialized once");
    }
}

pub fn deinit_allocator() {
    if let Some(alloc) = STABLE_MEMORY_ALLOCATOR.take() {
        alloc.store();
    } else {
        unreachable!("StableMemoryAllocator is not initialized");
    }
}

pub fn reinit_allocator(offset: u64) {
    if STABLE_MEMORY_ALLOCATOR.borrow().is_none() {
        let allocator = unsafe { StableMemoryAllocator::reinit(offset) };

        *STABLE_MEMORY_ALLOCATOR.borrow_mut() = Some(allocator);
    } else {
        unreachable!("StableMemoryAllocator can only be initialized once");
    }
}

pub fn allocate(size: usize) -> SSlice {
    if let Some(alloc) = &mut *STABLE_MEMORY_ALLOCATOR.borrow_mut() {
        alloc.allocate(size)
    } else {
        unreachable!("StableMemoryAllocator is not initialized");
    }
}

pub fn deallocate(slice: SSlice) {
    if let Some(alloc) = &mut *STABLE_MEMORY_ALLOCATOR.borrow_mut() {
        alloc.deallocate(slice)
    } else {
        unreachable!("StableMemoryAllocator is not initialized");
    }
}

pub fn reallocate(slice: SSlice, new_size: usize) -> Result<SSlice, SSlice> {
    if let Some(alloc) = &mut *STABLE_MEMORY_ALLOCATOR.borrow_mut() {
        alloc.reallocate(slice, new_size)
    } else {
        unreachable!("StableMemoryAllocator is not initialized");
    }
}

pub fn set_max_allocation_pages(pages: u32) {
    if let Some(alloc) = &mut *STABLE_MEMORY_ALLOCATOR.borrow_mut() {
        alloc.set_max_allocation_pages(pages)
    } else {
        unreachable!("StableMemoryAllocator is not initialized");
    }
}

pub fn get_max_allocation_pages() -> u32 {
    if let Some(alloc) = &*STABLE_MEMORY_ALLOCATOR.borrow() {
        alloc.get_max_allocation_pages()
    } else {
        unreachable!("StableMemoryAllocator is not initialized");
    }
}

pub fn set_max_grow_pages(pages: u64) {
    if let Some(alloc) = &mut *STABLE_MEMORY_ALLOCATOR.borrow_mut() {
        alloc.set_max_grow_pages(pages)
    } else {
        unreachable!("StableMemoryAllocator is not initialized");
    }
}

pub fn get_max_grow_pages() -> u64 {
    if let Some(alloc) = &*STABLE_MEMORY_ALLOCATOR.borrow() {
        alloc.get_max_grow_pages()
    } else {
        unreachable!("StableMemoryAllocator is not initialized");
    }
}

pub fn get_allocated_size() -> u64 {
    if let Some(alloc) = &*STABLE_MEMORY_ALLOCATOR.borrow() {
        alloc.get_allocated_size()
    } else {
        unreachable!("StableMemoryAllocator is not initialized");
    }
}

pub fn get_free_size() -> u64 {
    if let Some(alloc) = &*STABLE_MEMORY_ALLOCATOR.borrow() {
        alloc.get_free_size()
    } else {
        unreachable!("StableMemoryAllocator is not initialized");
    }
}

pub fn _set_custom_data_ptr(idx: usize, data_ptr: u64) {
    if let Some(alloc) = &mut *STABLE_MEMORY_ALLOCATOR.borrow_mut() {
        alloc.set_custom_data_ptr(idx, data_ptr)
    } else {
        unreachable!("StableMemoryAllocator is not initialized");
    }
}

pub fn _get_custom_data_ptr(idx: usize) -> u64 {
    if let Some(alloc) = &*STABLE_MEMORY_ALLOCATOR.borrow() {
        alloc.get_custom_data_ptr(idx)
    } else {
        unreachable!("StableMemoryAllocator is not initialized");
    }
}

pub fn get_mem_metrics() -> MemMetrics {
    MemMetrics {
        available: stable::size_pages() * PAGE_SIZE_BYTES as u64,
        free: get_free_size(),
        allocated: get_allocated_size(),
    }
}

pub fn _debug_print_allocator() {
    if let Some(alloc) = &*STABLE_MEMORY_ALLOCATOR.borrow_mut() {
        isoprint(format!("{:?}", alloc).as_str());
    } else {
        unreachable!("StableMemoryAllocator is not initialized");
    }
}

pub fn stable_memory_init(should_grow: bool, allocator_pointer: u64) {
    if should_grow {
        stable::grow(1).expect("Out of memory (stable_memory_init)");
    }

    init_allocator(allocator_pointer);
    init_vars();
}

pub fn stable_memory_pre_upgrade() {
    deinit_vars();
    deinit_allocator();
}

pub fn stable_memory_post_upgrade(allocator_pointer: u64) {
    reinit_allocator(allocator_pointer);
    reinit_vars();
}

#[cfg(test)]
mod tests {
    use crate::mem::allocator::{DEFAULT_MAX_ALLOCATION_PAGES, DEFAULT_MAX_GROW_PAGES, EMPTY_PTR};
    use crate::mem::Anyway;
    use crate::{
        _debug_print_allocator, _get_custom_data_ptr, _set_custom_data_ptr, allocate, deallocate,
        get_allocated_size, get_free_size, get_max_allocation_pages, get_max_grow_pages,
        get_mem_metrics, reallocate, set_max_allocation_pages, set_max_grow_pages,
        stable_memory_init, stable_memory_post_upgrade, stable_memory_pre_upgrade,
    };

    #[test]
    fn basic_flow_works_fine() {
        stable_memory_init(true, 0);
        stable_memory_pre_upgrade();
        stable_memory_post_upgrade(0);

        let b = allocate(100);
        let b = reallocate(b, 200).anyway();
        deallocate(b);

        assert_eq!(get_max_grow_pages(), DEFAULT_MAX_GROW_PAGES);
        assert_eq!(get_max_allocation_pages(), DEFAULT_MAX_ALLOCATION_PAGES);

        set_max_grow_pages(100);
        assert_eq!(get_max_grow_pages(), 100);

        set_max_allocation_pages(100);
        assert_eq!(get_max_allocation_pages(), 100);

        assert!(get_allocated_size() > 0);
        assert!(get_free_size() > 0);

        _debug_print_allocator();

        assert_eq!(_get_custom_data_ptr(1), EMPTY_PTR);
        _set_custom_data_ptr(1, 100);
        assert_eq!(_get_custom_data_ptr(1), 100);

        let m = get_mem_metrics();
        assert!(m.allocated > 0);
        assert!(m.free > 0);
        assert!(m.available > 0);

        _debug_print_allocator();
    }
}
