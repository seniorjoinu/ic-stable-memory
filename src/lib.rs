extern crate core;

use crate::mem::allocator::StableMemoryAllocator;
use mem::s_slice::SSlice;
use std::cell::RefCell;

mod benches;
pub mod collections;
pub mod encoding;
pub mod mem;
pub mod primitive;
pub mod utils;

use crate::mem::StablePtr;
pub use ic_stable_memory_derive::{CandidAsDynSizeBytes, StableDrop, StableType};

pub use crate::utils::mem_context::{stable, OutOfMemory, PAGE_SIZE_BYTES};
use crate::utils::{isoprint, MemMetrics};

thread_local! {
    static STABLE_MEMORY_ALLOCATOR: RefCell<Option<StableMemoryAllocator>> = RefCell::new(None);
}

#[inline]
pub fn init_allocator() {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if it.borrow().is_none() {
            let allocator = unsafe { StableMemoryAllocator::default() };

            *it.borrow_mut() = Some(allocator);
        } else {
            unreachable!("StableMemoryAllocator can only be initialized once");
        }
    })
}

#[inline]
pub fn deinit_allocator() {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if let Some(mut alloc) = it.take() {
            alloc.store();
        } else {
            unreachable!("StableMemoryAllocator is not initialized");
        }
    });
}

#[inline]
pub fn reinit_allocator() {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if it.borrow().is_none() {
            let allocator = unsafe { StableMemoryAllocator::retrieve() };

            *it.borrow_mut() = Some(allocator);
        } else {
            unreachable!("StableMemoryAllocator can only be initialized once");
        }
    });
}

#[inline]
pub fn allocate(size: usize) -> SSlice {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if let Some(alloc) = &mut *it.borrow_mut() {
            alloc.allocate(size)
        } else {
            unreachable!("StableMemoryAllocator is not initialized");
        }
    })
}

#[inline]
pub fn deallocate(slice: SSlice) {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if let Some(alloc) = &mut *it.borrow_mut() {
            alloc.deallocate(slice)
        } else {
            unreachable!("StableMemoryAllocator is not initialized");
        }
    })
}

#[inline]
pub fn reallocate(slice: SSlice, new_size: usize) -> Result<SSlice, SSlice> {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if let Some(alloc) = &mut *it.borrow_mut() {
            alloc.reallocate(slice, new_size)
        } else {
            unreachable!("StableMemoryAllocator is not initialized");
        }
    })
}

#[inline]
pub fn get_allocated_size() -> u64 {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if let Some(alloc) = &*it.borrow() {
            alloc.get_allocated_size()
        } else {
            unreachable!("StableMemoryAllocator is not initialized");
        }
    })
}

#[inline]
pub fn get_free_size() -> u64 {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if let Some(alloc) = &*it.borrow() {
            alloc.get_free_size()
        } else {
            unreachable!("StableMemoryAllocator is not initialized");
        }
    })
}

#[inline]
pub fn set_custom_data_ptr(idx: usize, data_ptr: StablePtr) -> Option<StablePtr> {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if let Some(alloc) = &mut *it.borrow_mut() {
            alloc.set_custom_data_ptr(idx, data_ptr)
        } else {
            unreachable!("StableMemoryAllocator is not initialized");
        }
    })
}

#[inline]
pub fn get_custom_data_ptr(idx: usize) -> Option<StablePtr> {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if let Some(alloc) = &*it.borrow() {
            alloc.get_custom_data_ptr(idx)
        } else {
            unreachable!("StableMemoryAllocator is not initialized");
        }
    })
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
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if let Some(alloc) = &*it.borrow_mut() {
            isoprint(format!("{:?}", alloc).as_str());
        } else {
            unreachable!("StableMemoryAllocator is not initialized");
        }
    })
}

#[inline]
pub fn stable_memory_init() {
    init_allocator();
}

#[inline]
pub fn stable_memory_pre_upgrade() {
    deinit_allocator();
}

#[inline]
pub fn stable_memory_post_upgrade() {
    reinit_allocator();
}

#[cfg(test)]
mod tests {
    use crate::mem::allocator::EMPTY_PTR;
    use crate::utils::Anyway;
    use crate::{
        _debug_print_allocator, allocate, deallocate, get_allocated_size, get_custom_data_ptr,
        get_free_size, get_mem_metrics, init_allocator, reallocate, set_custom_data_ptr,
        stable_memory_init, stable_memory_post_upgrade, stable_memory_pre_upgrade,
    };
    use crate::{deinit_allocator, reinit_allocator, SSlice};

    #[test]
    fn basic_flow_works_fine() {
        stable_memory_init();
        stable_memory_pre_upgrade();
        stable_memory_post_upgrade();

        let b = allocate(100);
        let b = reallocate(b, 200).anyway();
        deallocate(b);

        assert!(get_allocated_size() > 0);
        assert!(get_free_size() > 0);

        _debug_print_allocator();

        assert_eq!(get_custom_data_ptr(1), None);
        set_custom_data_ptr(1, 100);
        assert_eq!(get_custom_data_ptr(1), Some(100));

        let m = get_mem_metrics();
        assert!(m.allocated > 0);
        assert!(m.free > 0);
        assert!(m.available > 0);

        _debug_print_allocator();
    }

    #[test]
    #[should_panic]
    fn init_allocator_twice_should_panic() {
        init_allocator();
        init_allocator();
    }

    #[test]
    #[should_panic]
    fn deinit_allocator_should_panic() {
        deinit_allocator();
    }

    #[test]
    #[should_panic]
    fn reinit_allocator_twice_should_panic() {
        init_allocator();
        reinit_allocator();
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
        get_custom_data_ptr(0);
    }

    #[test]
    #[should_panic]
    fn set_custom_data_without_allocator_should_panic() {
        set_custom_data_ptr(0, 0);
    }

    #[test]
    #[should_panic]
    fn debug_print_without_allocator_should_panic() {
        _debug_print_allocator();
    }
}
