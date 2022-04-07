use crate::membox::Word;

pub const PAGE_SIZE_BYTES: usize = 64 * 1024;

/**
This allocator uses segregated explicit free list to track free memory blocks. There are total 60
segregation classes, each taking a u64 of space (60 * 8 = 480 bytes).
Each segregation class is stored inside a single u64 in an array of u64s.
Each index in that array is a separate segregation class of size 2 ** index, starting from 16 (since prev/next links both occupy a u64).
For example: given the array of length = 4, 1st item would contain a pointer to 1-15 bytes free list,
    2nd - to 16-31 bytes free list, 3rd - to 32-63 bytes, 4th - to 64-2**32 bytes.
 */
pub type SegregationClassPtr = u64;

pub const EMPTY_PTR: Word = Word::MAX;

#[derive(Debug)]
pub enum SMAError {
    AlreadyInitialized,
    OutOfMemory,
    InvalidMagicSequence,
    NoMemBlockAtAddress,
    OutOfBounds,
    ReallocationTooBig,
}
