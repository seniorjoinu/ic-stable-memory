use crate::mem::allocator::StableMemoryAllocator;
use crate::primitive::s_unsafe_cell::SUnsafeCell;
use ic_cdk::print;
use primitive::s_slice::SSlice;
use std::cell::RefCell;

mod benchmarks;
pub mod collections;
pub mod macros;
pub mod mem;
pub mod primitive;
pub mod utils;

pub use crate::utils::mem_context::{stable, OutOfMemory, PAGE_SIZE_BYTES};
pub use crate::utils::vars::{init_vars, reinit_vars, store_vars};
use crate::utils::MemMetrics;

thread_local! {
    static STABLE_MEMORY_ALLOCATOR: RefCell<Option<SSlice<StableMemoryAllocator>>> = RefCell::new(None);
}

pub fn init_allocator(offset: u64) {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if it.borrow().is_none() {
            let allocator = unsafe { SSlice::<StableMemoryAllocator>::init(offset) };

            *it.borrow_mut() = Some(allocator);
        } else {
            unreachable!("StableMemoryAllocator can only be initialized once");
        }
    });
}

pub fn reinit_allocator(offset: u64) {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if it.borrow().is_none() {
            let allocator = unsafe { SSlice::<StableMemoryAllocator>::reinit(offset).unwrap() };

            *it.borrow_mut() = Some(allocator);
        } else {
            unreachable!("StableMemoryAllocator can only be initialized once");
        }
    });
}

pub fn allocate<T>(size: usize) -> SSlice<T> {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if let Some(alloc) = &mut *it.borrow_mut() {
            alloc.allocate(size)
        } else {
            unreachable!("StableMemoryAllocator is not initialized");
        }
    })
}

pub fn deallocate<T>(membox: SSlice<T>) {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if let Some(alloc) = &mut *it.borrow_mut() {
            alloc.deallocate(membox)
        } else {
            unreachable!("StableMemoryAllocator is not initialized");
        }
    })
}

pub fn reallocate<T>(membox: SSlice<T>, new_size: usize) -> SSlice<T> {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if let Some(alloc) = &mut *it.borrow_mut() {
            alloc.reallocate(membox, new_size)
        } else {
            unreachable!("StableMemoryAllocator is not initialized");
        }
    })
}

pub fn set_max_allocation_pages(pages: u32) {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if let Some(alloc) = &mut *it.borrow_mut() {
            alloc.set_max_allocation_pages(pages)
        } else {
            unreachable!("StableMemoryAllocator is not initialized");
        }
    })
}

pub fn get_max_allocation_pages() -> u32 {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if let Some(alloc) = &*it.borrow() {
            alloc.get_max_allocation_pages()
        } else {
            unreachable!("StableMemoryAllocator is not initialized");
        }
    })
}

pub fn set_max_grow_pages(pages: u64) {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if let Some(alloc) = &mut *it.borrow_mut() {
            alloc.set_max_grow_pages(pages)
        } else {
            unreachable!("StableMemoryAllocator is not initialized");
        }
    })
}

pub fn get_max_grow_pages() -> u64 {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if let Some(alloc) = &*it.borrow() {
            alloc.get_max_grow_pages()
        } else {
            unreachable!("StableMemoryAllocator is not initialized");
        }
    })
}

pub fn get_allocated_size() -> u64 {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if let Some(alloc) = &*it.borrow() {
            alloc.get_allocated_size()
        } else {
            unreachable!("StableMemoryAllocator is not initialized");
        }
    })
}

pub fn get_free_size() -> u64 {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if let Some(alloc) = &*it.borrow() {
            alloc.get_free_size()
        } else {
            unreachable!("StableMemoryAllocator is not initialized");
        }
    })
}

pub fn _set_custom_data_ptr(idx: usize, data_ptr: u64) {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if let Some(alloc) = &mut *it.borrow_mut() {
            alloc.set_custom_data_ptr(idx, data_ptr)
        } else {
            unreachable!("StableMemoryAllocator is not initialized");
        }
    })
}

pub fn get_mem_metrics() -> MemMetrics {
    MemMetrics {
        available: stable::size_pages() * PAGE_SIZE_BYTES as u64,
        free: get_free_size(),
        allocated: get_allocated_size(),
    }
}

pub fn _get_custom_data_ptr(idx: usize) -> u64 {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if let Some(alloc) = &*it.borrow() {
            alloc.get_custom_data_ptr(idx)
        } else {
            unreachable!("StableMemoryAllocator is not initialized");
        }
    })
}

pub fn _debug_print_allocator() {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if let Some(alloc) = &*it.borrow_mut() {
            print(format!("{:?}", alloc))
        } else {
            unreachable!("StableMemoryAllocator is not initialized");
        }
    })
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
