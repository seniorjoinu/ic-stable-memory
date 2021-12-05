use ic_stable_memory_allocator::types::SMAError;

pub const STABLE_LINKED_LIST_MARKER: [u8; 1] = [6];
pub const STABLE_ARRAY_LIST_MARKER: [u8; 1] = [10];

#[derive(Debug)]
pub enum StableVecError {
    SMAError(SMAError),
    MarkerMismatch,
}

#[derive(Debug)]
pub enum StableLinkedListError {
    SMAError(SMAError),
    MarkerMismatch,
}

#[derive(Debug)]
pub enum StableArrayListError {
    SMAError(SMAError),
    StableLinkedListError(StableLinkedListError),
    MarkerMismatch,
    IndexOutOfBounds,
}
