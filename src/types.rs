use ic_cdk::api::stable::StableMemoryError;
use ic_cdk::export::candid::{CandidType, Deserialize, Error as CandidError};

pub const PAGE_SIZE_BYTES: usize = 64 * 1024;

pub type Word = u64;
pub type CollectionDeclarationPtr = Word;

/**
This allocator uses segregated explicit free list to track free memory blocks.
Each segregation class is stored inside a single Word in an array of Words.
Each index in that array is a separate segregation class of size 2 ** index, starting from 16 (since prev/next links both occupy a Word).
For example: given the array of length = 4, 1st item would contain a pointer to 1-15 bytes free list,
    2nd - to 16-31 bytes free list, 3rd - to 32-63 bytes, 4th - to 64-2**32 bytes.
 */
pub type SegregationClassPtr = Word;

pub const EMPTY_WORD: Word = 0;
pub const MAGIC: [u8; 4] = [1, 3, 3, 7];
pub const MAX_COLLECTION_DECLARATIONS: usize = 224;
pub const MAX_SEGREGATION_CLASSES: usize = 32;

pub enum SMAError {
    AlreadyInitialized,
    OutOfMemory,
    InvalidMagicSequence,
    NoMemBlockAtAddress,
}
