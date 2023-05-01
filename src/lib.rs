#![warn(missing_docs)]

//! This crate provides a number of "stable" data structures - collections that use canister's stable
//! memory for storage, as well as other primitives that allow storing all data of your canister in stable memory.
//! This crate also provides the [`SCertifiedBTreeMap`](collections::SCertifiedBTreeMap) - a Merkle-tree based collection that can
//! be used to include custom data into canister's certified state tree.
//!
//! This documentation only covers API and some implementation details. For more useful info and
//! tutorials, please visit [project's Github page](https://github.com/seniorjoinu/ic-stable-memory).
//!
//! # Features
//! 1. Stable data structures release their memory automatically, following Rust's ownership rules.
//! 2. Stable data structures also obey and enforce Rust's borrowing and lifetime rules.
//! 3. Each data structure is aware of the limited nature of memory in IC and allows programmatic
//! reaction for situations when your canister is out of stable memory.
//! 3. Each data structure's performance is reasonably close to its std's analog.
//! 4. Supported stable data structures: box, vec, log, hash-map, hash-set, btree-map, btree-set, certified-map.
//! 5. In addition to these data structures, this crate provides you with a fully featured toolset
//! to build your own data structure, if you need something more domain-specific.
use crate::mem::allocator::StableMemoryAllocator;
use mem::s_slice::SSlice;
use std::cell::RefCell;

mod benches;
/// All collections provided by this crate
pub mod collections;
/// Traits and algorithms for internal data encoding
pub mod encoding;
/// Stable memory allocator and related structs
pub mod mem;
/// Stable memory smart-pointers
pub mod primitive;
/// Stable memory native types
pub mod types;
/// Various utilities: certification, stable memory API wrapper etc.
pub mod utils;

pub use ic_stable_memory_derive as derive;

use crate::utils::isoprint;
pub use crate::utils::mem_context::{stable, OutOfMemory, PAGE_SIZE_BYTES};
pub use encoding::{AsDynSizeBytes, AsFixedSizeBytes, Buffer};
pub use primitive::s_box::SBox;
pub use primitive::StableType;
pub use utils::certification::{
    empty, empty_hash, fork, fork_hash, labeled, labeled_hash, leaf, leaf_hash, pruned, AsHashTree,
    AsHashableBytes, HashTree,
};

thread_local! {
    static STABLE_MEMORY_ALLOCATOR: RefCell<Option<StableMemoryAllocator>> = RefCell::new(None);
}

/// Initializes the [memory allocator](mem::allocator::StableMemoryAllocator).
///
/// This function should be called *ONLY ONCE* during the lifetime of a canister. For canisters,
/// that are being build using this crate from scratch, the most apropriate place to call it is the
/// `#[init]` canister method. For canisters which are migrating from standard data structures thic
/// function should be added as a first line of `#[post_upgrade]` canister method and later (right after
/// this canister upgrade happens) in the next code revision it should be replaced with [stable_memory_post_upgrade()].
///
/// Stable memory allocator is stored inside a `thread_local!` static variable at runtime.
///
/// Works the same way as [init_allocator(0)].
///
/// # Panics
/// Panics if the allocator is already initialized.
///
/// # Examples
/// For new canisters:
/// ```rust
/// # use ic_stable_memory::stable_memory_init;
/// #[ic_cdk_macros::init]
/// fn init() {
///     stable_memory_init();
///
///     // the rest of the initialization
/// }
/// ```
///
/// For migrating canisters:
/// ```rust
/// // canister version N
/// # use ic_stable_memory::stable_memory_init;
/// #[ic_cdk_macros::post_upgrade]
/// fn post_upgrade() {
///     stable_memory_init();
///
///     // move data from standard collections into "stable" ones
/// }
/// ```
/// ```rust
/// // canister version N+1
/// # use ic_stable_memory::stable_memory_post_upgrade;
/// #[ic_cdk_macros::post_upgrade]
/// fn post_upgrade() {
///     stable_memory_post_upgrade();
///
///     // the rest of canister's reinitialization
/// }
/// ```
#[inline]
pub fn stable_memory_init() {
    init_allocator(0);
}

/// Persists the memory allocator into stable memory between canister upgrades.
///
/// See also [stable_memory_post_upgrade].
///
/// This function should be called as the last step of the `#[pre_ugrade]` canister method.
///
/// It works by first writing the allocator to an `SBox` and then writing a pointer to that `SBox` into
/// frist 8 bytes of stable memory (offsets [0..8)). `thread_local!` static variable that stores the
/// allocator also gets cleared, if this function is executed successfully.
///
/// If it was impossible to allocate a memory block of required size, this function returns an [OutOfMemory]
/// error. For tips on possible ways of resolving an [OutOfMemory] error visit [this page](https://github.com/seniorjoinu/ic-stable-memory/docs/out-of-memory-error-handling.md).
///
/// This function is an alias for [deinit_allocator()].
///
/// # Example
/// ```rust
/// # use ic_stable_memory::stable_memory_pre_upgrade;
/// #[ic_cdk_macros::pre_upgrade]
/// fn pre_upgrade() {
///     // other pre-upgrade routine
///
///     if stable_memory_pre_upgrade().is_err() {
///         panic!("Out of stable memory")
///     }
/// }
/// ```
///
/// # Panics
/// Panics if there is no initialized stable memory allocator.
#[inline]
pub fn stable_memory_pre_upgrade() -> Result<(), OutOfMemory> {
    deinit_allocator()
}

/// Retrieves the memory allocator from stable memory.
///
/// See also [stable_memory_pre_upgrade].
///
/// This function should be called as the first step of the `#[post_upgrade]` canister method.
///
/// The process is exactly the same as in `stable_memory_pre_upgrade`, but in reverse order. It reads
/// first 8 bytes of stable memory to get a pointer. Then the `SBox` located at that pointer is read
/// and "unboxed" into the allocator. Then the allocator is assigned back to the `thread_local!` variable.
///
/// This function is an alias for [reinit_allocator()].
///
/// # Example
/// ```rust
/// use ic_stable_memory::stable_memory_post_upgrade;
/// #[ic_cdk_macros::post_upgrade]
/// fn post_upgrade() {
///     stable_memory_post_upgrade();
///
///     // other post-upgrade routine
/// }
/// ```
///
/// # Panics
/// This function will panic if:
/// 1. there is no valid pointer stored at first 8 bytes of stable memory,
/// 2. there is no valid `SBox` was found at that location,
/// 3. deserialization step during `SBox`'s "unboxing" failed due to invalid data stored inside this `SBox`,
/// 4. if there was an already initialized stable memory allocator.
#[inline]
pub fn stable_memory_post_upgrade() {
    reinit_allocator();
}

/// An alias for [stable_memory_init], but allows limiting the maximum number of stable memory pages
/// that the allocator can grow. [init_allocator(0)] works exactly the same as [stable_memory_init()].
///
/// This function is useful for testing, when one wants to see how a canister behaves when there is
/// only a little of stable memory available.
///
/// If this function is invoked in a canister that already grown more stable memory pages than the
/// argument states (this can happen for canisters that migrate from standard collections to "stable" ones),
/// then the actual number of already grown pages is used as a maximum number of pages
/// instead of what is passed as an argument.
///
/// Passing a `0` as an argument has a special "infinite" meaning, which means "grow as many pages
/// as needed, while it is possible".
///
/// Internally calls [StableMemoryAllocator::init](mem::allocator::StableMemoryAllocator::init).
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

/// An alias for [stable_memory_pre_upgrade].
///
/// Internally calls [StableMemoryAllocator::store](mem::allocator::StableMemoryAllocator::store).
///
/// # Panics
/// Panics if there is no initialized stable memory allocator.
#[inline]
pub fn deinit_allocator() -> Result<(), OutOfMemory> {
    STABLE_MEMORY_ALLOCATOR.with(|it: &RefCell<Option<StableMemoryAllocator>>| {
        if let Some(mut alloc) = it.take() {
            let res = alloc.store();
            if res.is_err() {
                *it.borrow_mut() = Some(alloc);
            }

            res
        } else {
            unreachable!("StableMemoryAllocator is not initialized");
        }
    })
}

/// An alias for [stable_memory_post_upgrade].
///
/// Internally calls [StableMemoryAllocator::retrieve](mem::allocator::StableMemoryAllocator::retrieve).
///
/// # Panics
/// Panics if there is no initialized stable memory allocator.
#[inline]
pub fn reinit_allocator() {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if it.borrow().is_none() {
            let allocator = StableMemoryAllocator::retrieve();

            *it.borrow_mut() = Some(allocator);
        } else {
            unreachable!("StableMemoryAllocator can only be initialized once");
        }
    });
}

/// Persists a pointer to an [SBox] between canister upgrades mapped to some unique [usize] key.
///
/// See also [retrieve_custom_data].
///
/// Despite the fact that stable memory data structures from this crate store all data completely on
/// stable memory, they themselves are stored on stack. Exactly how standard data structures ([Vec],
/// for example) store their data on heap, but themselves are stored on stack. This means that in order
/// to persist this data structures between canister upgrades, we have to temporary store them on
/// stable memory aswell.
///
/// This function allows one to do that, by assigning a unique [usize] index to the stored data, which was
/// previously stored in [SBox].
///
/// This function should be used in the `#[pre_upgrade]` canister method. Right before
/// [stable_memory_pre_upgrade()] invocation. This function can be used multiple times, but one should
/// make sure they always keep track of keys they are assigning custom data to. An attempt to assign
/// two values to a single key will lead to losing the data that was assigned first. *Be careful!*
///
/// Internally calls [StableMemoryAllocator::store_custom_data](mem::allocator::StableMemoryAllocator::store_custom_data).
///
/// # Example
/// ```rust
/// # use ic_stable_memory::collections::SHashMap;
/// # use ic_stable_memory::{retrieve_custom_data, SBox, stable_memory_init, stable_memory_post_upgrade, stable_memory_pre_upgrade, store_custom_data};
/// # unsafe { ic_stable_memory::mem::clear(); }
/// # stable_memory_init();
/// static mut STATE: Option<SHashMap<u64, u64>> = None;
///
/// #[ic_cdk_macros::pre_upgrade]
/// fn pre_upgrade() {
///     let state = unsafe { STATE.take().unwrap() };
///     let boxed_state = SBox::new(state).expect("Out of memory");
///
///     store_custom_data(1, boxed_state);
///
///     // always as a last expression of "pre_upgrade"
///     stable_memory_pre_upgrade();
/// }
///
/// #[ic_cdk_macros::post_upgrade]
/// fn post_upgrade() {
///     // always as a first expression of "post_upgrade"
///     stable_memory_post_upgrade();
///
///     let boxed_state = retrieve_custom_data::<SHashMap<u64, u64>>(1)
///         .expect("Key not found");
///
///     unsafe { STATE = Some(boxed_state.into_inner()); }
/// }
/// ```
///
/// One can also persist other data this way
/// ```rust
/// # use ic_stable_memory::{retrieve_custom_data, SBox, stable_memory_post_upgrade, stable_memory_pre_upgrade, store_custom_data};
///
/// #[ic_cdk_macros::pre_upgrade]
/// fn pre_upgrade() {
///     let very_important_string = String::from("THE PASSWORD IS 42");
///     let boxed_string = SBox::new(very_important_string)
///         .expect("Out of memory");
///     
///     store_custom_data(2, boxed_string);
///
///     // always as a last expression of "pre_upgrade"
///     stable_memory_pre_upgrade();
/// }
///
/// #[ic_cdk_macros::post_upgrade]
/// fn post_upgrade() {
///     // always as a first expression of "post_upgrade"
///     stable_memory_post_upgrade();
///
///     let boxed_string = retrieve_custom_data::<String>(2)
///         .expect("Key not found");
///
///     let very_important_string = boxed_string.into_inner();
/// }
/// ```
///
/// # Panics
/// Panics if there is no initialized stable memory allocator.
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

/// Retrieves a pointer to some [SBox] stored previously.
///
/// See also [store_custom_data].
///
/// This function is intended to be invoked inside the `#[post_upgrade]` canister method. Right after
/// [stable_memory_post_upgrade()] invocation. After retrieval, the key gets "forgotten", allowing
/// reusing it again for other data.
///
/// Any panic in the `#[post_upgrade]` canister method results in broken canister.
/// Please, *be careful*.
///
/// Internally calls [StableMemoryAllocator::retrieve_custom_data](mem::allocator::StableMemoryAllocator::retrieve_custom_data).
///
/// # Examples
/// See examples of [store_custom_data].
///
/// # Panics
/// Panics if there is no initialized stable memory allocator.
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

/// Attempts to allocate a new [SSlice] of at least the required size or returns an [OutOfMemory] error
/// if there is no continuous stable memory memory block of that size can be allocated.
///
/// Memory block that is returned *can be bigger* than requested. This happens because:
/// 1. Sizes for allocation are always getting rounded up to the next multiple of 8 bytes. For example,
/// if requested 100 bytes to allocate requested, the resulting memory block can't be smaller than 104 bytes.
/// 2. Minimum memory block size is 16 bytes. To find out more see documentation for [the allocator](mem::allocator::StableMemoryAllocator).
/// 3. If the allocator has a free block of size less than `requested size + minimum block size`,
/// this block won't be split and will be returned as is.
///
/// If the allocator only has a memory block which is bigger than `requested size + minimum block size`,
/// that block gets split it two. The first half is returned as the result of this function, and the other
/// half goes back to the free list.
///
/// If the allocator has no apropriate free memory block to allocate, it will try to grow stable memory
/// by the number of pages enough to allocate a block of that size. If it can't grow due to lack of
/// stable memory in a subnet or due to reaching `max_pages` limit set earlier - it will return an
/// [OutOfMemory] error.
///
/// Internally calls [StableMemoryAllocator::allocate](mem::allocator::StableMemoryAllocator::allocate).
///
/// # Example
/// ```rust
/// // slice size is in [104..136) bytes range, despite requesting for only 100 bytes
/// # use ic_stable_memory::{allocate, stable_memory_init};
/// # unsafe { ic_stable_memory::mem::clear(); }
/// # stable_memory_init();
/// # unsafe {
/// let slice = allocate(100).expect("Not enough stable memory");
/// # }
/// ```
///
/// # Panics
/// Panics if there is no initialized stable memory allocator.
///
/// # Safety
/// Don't forget to [deallocate] the memory block, when you're done!
#[inline]
pub unsafe fn allocate(size: u64) -> Result<SSlice, OutOfMemory> {
    STABLE_MEMORY_ALLOCATOR.with(|it| {
        if let Some(alloc) = &mut *it.borrow_mut() {
            alloc.allocate(size)
        } else {
            unreachable!("StableMemoryAllocator is not initialized");
        }
    })
}

/// Deallocates an already allocated [SSlice] freeing it's memory.
///
/// Supplied [SSlice] get's transformed into [FreeBlock](mem::free_block::FreeBlock) and then an
/// attempt to merge it with neighboring (physically) free blocks is performed.
///
/// Internally calls [StableMemoryAllocator::deallocate](mem::allocator::StableMemoryAllocator::deallocate).
///
/// # Example
/// ```rust
/// # use ic_stable_memory::{allocate, deallocate, stable_memory_init};
/// # unsafe { ic_stable_memory::mem::clear(); }
/// # stable_memory_init();
/// # unsafe {
/// let slice = allocate(100).expect("Out of memory");
/// deallocate(slice);
/// # }
/// ```
///
/// # Panics
/// Panics if there is no initialized stable memory allocator.
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

/// Attempts to reallocate a memory block growing its size and possibly moving its content to a new
/// location.
///
/// At first it tries to perform an `inplace reallocation` - check if the next neighboring (physically)
/// memory block is also free. If that is so, this neighboring (or only a chunk of it, if it's too big)
/// free block gets merged with the one passed as an argument to this function and returned as a result.
/// This process does not move the data.
///
/// If there is no neighboring free block, than a sequence of operations is performed:
/// 1. Copy the data to a heap-allocated byte buffer.
/// 2. Deallocate the [SSlice] passed as an argument to this function.
/// 3. Allocate a new [SSlice] of the requested size, possibly returning an [OutOfMemory] error.
/// 4. Copy data from the byte buffer to this new [SSlice].
/// 5. Return it as a result.
/// This process moves the data.
///
/// If the requested new size is less than the actual size of the [SSlice] passed as an argument,
/// the function does nothing and returns this [SSlice] as a result back.
///
/// Internally calls [StableMemoryAllocator::reallocate](mem::allocator::StableMemoryAllocator::reallocate).
///
/// # Example
/// ```rust
/// # use ic_stable_memory::{allocate, stable_memory_init, reallocate};
/// # unsafe { ic_stable_memory::mem::clear(); }
/// # stable_memory_init();
/// # unsafe {
/// let slice = allocate(100).expect("Out of memory");
/// let bigger_slice = reallocate(slice, 200).expect("Out of memory");
/// # }
/// ```
///
/// # Panics
/// Panics if there is no initialized stable memory allocator.
/// Reallocating [SSlice]s bigger than [u32::MAX] bytes will also panic.
///
/// # Safety
/// Don't forget to [deallocate] the memory block, when you're done!
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

/// Checks if it would be possible to allocate a block of stable memory of the provided size right now.
///
/// The allocator will check its free list for a block of appropriate size. If there is no such free
/// block, it will try to grow stable memory by the number of pages enough to fit this size.
///
/// Returns `true` if a block was found. Returns `false` if an attempt to grow stable memory resulted in
/// an [OutOfMemory] error.
///
/// Internally calls [StableMemoryAllocator::make_sure_can_allocate](mem::allocator::StableMemoryAllocator::make_sure_can_allocate).
///
/// # Example
/// ```rust
/// # use ic_stable_memory::{make_sure_can_allocate, stable_memory_init};
/// # unsafe { ic_stable_memory::mem::clear(); }
/// # stable_memory_init();
/// if make_sure_can_allocate(1_000_000) {
///     println!("It is possible to allocate a million bytes of stable memory");
/// }
/// ```
///
/// # Panics
/// Panics if there is no initialized stable memory allocator.
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

/// Returns the amount of stable memory in bytes which is under the allocator's management.
///
/// Always equals to [stable64_size()](ic_cdk::api::stable::stable64_size) - `8`.
///
/// Internally calls [StableMemoryAllocator::get_available_size](mem::allocator::StableMemoryAllocator::get_available_size).
///
/// # Panics
/// Panics if there is no initialized stable memory allocator.
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

/// Returns the amount of free stable memory in bytes.
///
/// Internally calls [StableMemoryAllocator::get_free_size](mem::allocator::StableMemoryAllocator::get_free_size).
///
/// # Panics
/// Panics if there is no initialized stable memory allocator.
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

/// Returns the amount of allocated stable memory in bytes.
///
/// Always equal to [get_available_size()] - [get_free_size()].
///
/// Internally calls [StableMemoryAllocator::get_allocated_size](mem::allocator::StableMemoryAllocator::get_allocated_size).
///
/// # Panics
/// Panics if there is no initialized stable memory allocator.
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

/// Returns `max_pages` parameter.
///
/// See [init_allocator] for more details.
///
/// # Panics
/// Panics if there is no initialized stable memory allocator.
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

        let b = unsafe { allocate(100).unwrap() };
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
        unsafe { allocate(10) };
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
