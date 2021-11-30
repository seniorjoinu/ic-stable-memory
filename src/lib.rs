use crate::mem_context::StableMemContext;
use crate::stable_memory_allocator::StableMemoryAllocator;
use crate::types::{SMAError, Word};
use ic_cdk::trap;

pub mod mem_block;
pub mod mem_context;
pub mod stable_memory_allocator;
pub mod types;
pub mod utils;

pub static mut STABLE_MEMORY_ALLOCATOR: Option<StableMemoryAllocator<StableMemContext>> = None;

pub fn init_allocator(offset: Word) {
    let allocator =
        StableMemoryAllocator::init(offset, &mut StableMemContext).unwrap_or_else(|e| {
            trap(format!("Unable to init StableMemoryAllocator: {:?}", e).as_str())
        });

    unsafe { STABLE_MEMORY_ALLOCATOR = Some(allocator) }
}

pub fn reinit_allocator(offset: Word) {
    let allocator = StableMemoryAllocator::reinit(offset, &StableMemContext).unwrap_or_else(|e| {
        trap(format!("Unable to reinit StableMemoryAllocator: {:?}", e).as_str())
    });

    unsafe { STABLE_MEMORY_ALLOCATOR = Some(allocator) }
}

pub fn get_allocator() -> &'static mut StableMemoryAllocator<StableMemContext> {
    unsafe {
        match STABLE_MEMORY_ALLOCATOR.as_mut() {
            Some(sma) => sma,
            None => trap("StableMemoryAllocator is not initialized"),
        }
    }
}

pub fn stable_alloc(size: usize) -> Result<Word, SMAError> {
    let sma = get_allocator();

    sma.allocate(size, &mut StableMemContext)
}

pub fn stable_dealloc(offset: Word) {
    let sma = get_allocator();

    sma.deallocate(offset, &mut StableMemContext);
}

pub fn stable_realloc(offset: Word, wanted_size: usize) -> Result<Word, SMAError> {
    let sma = get_allocator();

    sma.reallocate(offset, wanted_size, &mut StableMemContext)
}

pub fn set_stable_collection_declaration(declaration_id: usize, ptr: Word) {
    let sma = get_allocator();

    sma.set_collection_declaration(declaration_id, ptr, &mut StableMemContext);
}
