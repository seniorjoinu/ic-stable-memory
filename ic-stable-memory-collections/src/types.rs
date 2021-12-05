use ic_stable_memory_allocator::types::SMAError;

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