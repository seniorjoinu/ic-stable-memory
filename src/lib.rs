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

use crate::encoding::AsDynSizeBytes;
/// ПРОВЕРИТЬ ТУДУХИ
/// ПРОВЕРИТЬ ГЕТ-МУТ
/// ФАЗЗЕРЫ ДЛЯ ВСЕГО
/// ОБЩИЙ ФАЗЗЕР НА БОЛЬШОЙ СТЕЙТ ИЗ ВСЕГО ПОДРЯД
/// ПРОВЕРИТЬ СЕРТИФАЙД АССЕТС
/// НАПИСАТЬ ДОКУМЕНТАЦИЮ + ПОМЕТИТЬ ФИКСМИ
use crate::primitive::s_box::SBox;
use crate::primitive::StableType;
pub use ic_stable_memory_derive::{CandidAsDynSizeBytes, StableDrop, StableType};

use crate::utils::isoprint;
pub use crate::utils::mem_context::{stable, OutOfMemory, PAGE_SIZE_BYTES};

thread_local! {
    static STABLE_MEMORY_ALLOCATOR: RefCell<Option<StableMemoryAllocator>> = RefCell::new(None);
}

#[inline]
pub fn init_allocator(max_pages: u64) {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if it.borrow().is_none() {
            let allocator = StableMemoryAllocator::init(max_pages);

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
pub fn allocate(size: u64) -> Result<SSlice, OutOfMemory> {
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

/// Safety:
/// Reallocating slices bigger than u32::MAX bytes will panic, since data has to be moved into
/// the new location and this move is implemented via reading all bytes from the old location into
/// a regular Vec<u8> and then writing them into the new location.
/// Make sure, you're only reallocating slices smaller than usize::MAX.
#[inline]
pub unsafe fn reallocate(slice: SSlice, new_size: u64) -> Result<SSlice, OutOfMemory> {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if let Some(alloc) = &mut *it.borrow_mut() {
            alloc.reallocate(slice, new_size)
        } else {
            unreachable!("StableMemoryAllocator is not initialized");
        }
    })
}

#[inline]
pub fn make_sure_can_allocate(size: u64) -> bool {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if let Some(alloc) = &mut *it.borrow_mut() {
            alloc.make_sure_can_allocate(size)
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
pub fn get_available_size() -> u64 {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if let Some(alloc) = &*it.borrow() {
            alloc.get_available_size()
        } else {
            unreachable!("StableMemoryAllocator is not initialized");
        }
    })
}

#[inline]
pub fn store_custom_data<T: StableType + AsDynSizeBytes>(idx: usize, data: SBox<T>) {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if let Some(alloc) = &mut *it.borrow_mut() {
            alloc.store_custom_data(idx, data)
        } else {
            unreachable!("StableMemoryAllocator is not initialized");
        }
    })
}

#[inline]
pub fn retrieve_custom_data<T: StableType + AsDynSizeBytes>(idx: usize) -> Option<SBox<T>> {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if let Some(alloc) = &mut *it.borrow_mut() {
            alloc.retrieve_custom_data(idx)
        } else {
            unreachable!("StableMemoryAllocator is not initialized");
        }
    })
}

#[inline]
pub fn get_max_pages() -> u64 {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if let Some(alloc) = &*it.borrow() {
            alloc.get_max_pages()
        } else {
            unreachable!("StableMemoryAllocator is not initialized");
        }
    })
}

#[inline]
pub fn _debug_validate_allocator() {
    STABLE_MEMORY_ALLOCATOR.with(|it: &RefCell<Option<StableMemoryAllocator>>| {
        if let Some(alloc) = &*it.borrow() {
            alloc.debug_validate_free_blocks();
        } else {
            unreachable!("StableMemoryAllocator is not initialized");
        }
    })
}

#[inline]
pub fn _debug_print_allocator() {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if let Some(alloc) = &*it.borrow_mut() {
            isoprint(format!("{alloc:?}").as_str());
        } else {
            unreachable!("StableMemoryAllocator is not initialized");
        }
    })
}

#[inline]
pub fn stable_memory_init() {
    init_allocator(0);
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
    use crate::{
        _debug_print_allocator, allocate, deallocate, get_allocated_size, get_free_size,
        init_allocator, reallocate, retrieve_custom_data, stable_memory_init,
        stable_memory_post_upgrade, stable_memory_pre_upgrade, store_custom_data, SBox,
    };
    use crate::{deinit_allocator, reinit_allocator, SSlice};

    #[test]
    fn basic_flow_works_fine() {
        stable_memory_init();
        stable_memory_pre_upgrade();
        stable_memory_post_upgrade();

        let b = allocate(100).unwrap();
        let b = unsafe { reallocate(b, 200).unwrap() };
        deallocate(b);

        assert_eq!(get_allocated_size(), 0);
        assert!(get_free_size() > 0);

        _debug_print_allocator();

        assert_eq!(retrieve_custom_data::<u64>(1), None);
        store_custom_data(1, SBox::new(100u64).unwrap());
        assert_eq!(retrieve_custom_data::<u64>(1).unwrap().into_inner(), 100);

        _debug_print_allocator();
    }

    #[test]
    #[should_panic]
    fn init_allocator_twice_should_panic() {
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
        init_allocator(0);
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
        unsafe { reallocate(SSlice::new(0, 10, false), 20) };
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
        retrieve_custom_data::<u64>(0);
    }

    #[test]
    #[should_panic]
    fn set_custom_data_without_allocator_should_panic() {
        store_custom_data(0, SBox::new(0).unwrap());
    }

    #[test]
    #[should_panic]
    fn debug_print_without_allocator_should_panic() {
        _debug_print_allocator();
    }
}
