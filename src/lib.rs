#![feature(thread_local)]
#![feature(generic_const_exprs)]

extern crate core;

use crate::mem::allocator::StableMemoryAllocator;
use mem::s_slice::SSlice;
use std::cell::RefCell;

mod benches;
pub mod collections;
pub mod macros;
pub mod mem;
pub mod primitive;
pub mod utils;

pub use ic_stable_memory_derive::{StableDrop, StableType};

pub use crate::utils::mem_context::{stable, OutOfMemory, PAGE_SIZE_BYTES};
use crate::utils::vars::deinit_vars;
pub use crate::utils::vars::{init_vars, reinit_vars};
use crate::utils::{isoprint, MemMetrics};

#[thread_local]
static STABLE_MEMORY_ALLOCATOR: RefCell<Option<StableMemoryAllocator>> = RefCell::new(None);

#[inline]
pub fn init_allocator(offset: u64) {
    if STABLE_MEMORY_ALLOCATOR.borrow().is_none() {
        let allocator = unsafe { StableMemoryAllocator::init(offset) };

        *STABLE_MEMORY_ALLOCATOR.borrow_mut() = Some(allocator);
    } else {
        unreachable!("StableMemoryAllocator can only be initialized once");
    }
}

#[inline]
pub fn deinit_allocator() {
    if let Some(alloc) = STABLE_MEMORY_ALLOCATOR.take() {
        alloc.store();
    } else {
        unreachable!("StableMemoryAllocator is not initialized");
    }
}

#[inline]
pub fn reinit_allocator(offset: u64) {
    if STABLE_MEMORY_ALLOCATOR.borrow().is_none() {
        let allocator = unsafe { StableMemoryAllocator::reinit(offset) };

        *STABLE_MEMORY_ALLOCATOR.borrow_mut() = Some(allocator);
    } else {
        unreachable!("StableMemoryAllocator can only be initialized once");
    }
}

#[inline]
pub fn allocate(size: usize) -> SSlice {
    if let Some(alloc) = &mut *STABLE_MEMORY_ALLOCATOR.borrow_mut() {
        alloc.allocate(size)
    } else {
        unreachable!("StableMemoryAllocator is not initialized");
    }
}

#[inline]
pub fn deallocate(slice: SSlice) {
    if let Some(alloc) = &mut *STABLE_MEMORY_ALLOCATOR.borrow_mut() {
        alloc.deallocate(slice)
    } else {
        unreachable!("StableMemoryAllocator is not initialized");
    }
}

#[inline]
pub fn mark_for_lazy_deallocation(ptr: u64) {
    if let Some(alloc) = &mut *STABLE_MEMORY_ALLOCATOR.borrow_mut() {
        alloc.mark_for_lazy_deallocation(ptr)
    } else {
        unreachable!("StableMemoryAllocator is not initialized");
    }
}

#[inline]
pub fn deallocate_lazy() {
    if let Some(alloc) = &mut *STABLE_MEMORY_ALLOCATOR.borrow_mut() {
        alloc.deallocate_lazy();
    } else {
        unreachable!("StableMemoryAllocator is not initialized");
    }
}

#[inline]
pub fn reallocate(slice: SSlice, new_size: usize) -> Result<SSlice, SSlice> {
    if let Some(alloc) = &mut *STABLE_MEMORY_ALLOCATOR.borrow_mut() {
        alloc.reallocate(slice, new_size)
    } else {
        unreachable!("StableMemoryAllocator is not initialized");
    }
}

#[inline]
pub fn get_allocated_size() -> u64 {
    if let Some(alloc) = &*STABLE_MEMORY_ALLOCATOR.borrow() {
        alloc.get_allocated_size()
    } else {
        unreachable!("StableMemoryAllocator is not initialized");
    }
}

#[inline]
pub fn get_free_size() -> u64 {
    if let Some(alloc) = &*STABLE_MEMORY_ALLOCATOR.borrow() {
        alloc.get_free_size()
    } else {
        unreachable!("StableMemoryAllocator is not initialized");
    }
}

#[inline]
pub fn _set_custom_data_ptr(idx: usize, data_ptr: u64) {
    if let Some(alloc) = &mut *STABLE_MEMORY_ALLOCATOR.borrow_mut() {
        alloc.set_custom_data_ptr(idx, data_ptr)
    } else {
        unreachable!("StableMemoryAllocator is not initialized");
    }
}

#[inline]
pub fn _get_custom_data_ptr(idx: usize) -> u64 {
    if let Some(alloc) = &*STABLE_MEMORY_ALLOCATOR.borrow() {
        alloc.get_custom_data_ptr(idx)
    } else {
        unreachable!("StableMemoryAllocator is not initialized");
    }
}

#[inline]
pub fn get_mem_metrics() -> MemMetrics {
    MemMetrics {
        available: stable::size_pages() * PAGE_SIZE_BYTES as u64,
        free: get_free_size(),
        allocated: get_allocated_size(),
    }
}

#[inline]
pub fn _debug_print_allocator() {
    if let Some(alloc) = &*STABLE_MEMORY_ALLOCATOR.borrow_mut() {
        isoprint(format!("{:?}", alloc).as_str());
    } else {
        unreachable!("StableMemoryAllocator is not initialized");
    }
}

#[inline]
pub fn stable_memory_init(should_grow: bool, allocator_pointer: u64) {
    if should_grow {
        stable::grow(1).expect("Out of memory (stable_memory_init)");
    }

    init_allocator(allocator_pointer);
    init_vars();
}

#[inline]
pub fn stable_memory_pre_upgrade() {
    deinit_vars();
    deinit_allocator();
}

#[inline]
pub fn stable_memory_post_upgrade(allocator_pointer: u64) {
    reinit_allocator(allocator_pointer);
    reinit_vars();
}

#[cfg(test)]
mod tests {
    use crate::mem::allocator::EMPTY_PTR;
    use crate::mem::Anyway;
    use crate::{
        _debug_print_allocator, _get_custom_data_ptr, _set_custom_data_ptr, allocate, deallocate,
        get_allocated_size, get_free_size, get_mem_metrics, init_allocator, reallocate,
        stable_memory_init, stable_memory_post_upgrade, stable_memory_pre_upgrade,
    };
    use crate::{deinit_allocator, reinit_allocator, stable, SSlice};

    #[test]
    fn basic_flow_works_fine() {
        stable_memory_init(true, 0);
        stable_memory_pre_upgrade();
        stable_memory_post_upgrade(0);

        let b = allocate(100);
        let b = reallocate(b, 200).anyway();
        deallocate(b);

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

    #[test]
    #[should_panic]
    fn init_allocator_twice_should_panic() {
        stable::grow(1).expect("Out of memory (stable_memory_init)");
        init_allocator(0);
        init_allocator(0);
    }

    #[test]
    #[should_panic]
    fn deinit_allocator_should_panic() {
        deinit_allocator();
    }

    #[test]
    #[should_panic]
    fn reinit_allocator_twice_should_panic() {
        stable::grow(1).expect("Out of memory (stable_memory_init)");
        init_allocator(0);
        reinit_allocator(0);
    }

    #[test]
    #[should_panic]
    fn allocate_without_allocator_should_panic() {
        allocate(10);
    }

    #[test]
    #[should_panic]
    fn deallocate_without_allocator_should_panic() {
        deallocate(SSlice::new(0, 10, false));
    }

    #[test]
    #[should_panic]
    fn reallocate_without_allocator_should_panic() {
        reallocate(SSlice::new(0, 10, false), 20);
    }

    #[test]
    #[should_panic]
    fn get_allocated_size_without_allocator_should_panic() {
        get_allocated_size();
    }

    #[test]
    #[should_panic]
    fn get_free_size_without_allocator_should_panic() {
        get_free_size();
    }

    #[test]
    #[should_panic]
    fn get_custom_data_without_allocator_should_panic() {
        _get_custom_data_ptr(0);
    }

    #[test]
    #[should_panic]
    fn set_custom_data_without_allocator_should_panic() {
        _set_custom_data_ptr(0, 0);
    }

    #[test]
    #[should_panic]
    fn debug_print_without_allocator_should_panic() {
        _debug_print_allocator();
    }
}
