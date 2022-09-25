use crate::SSlice;

pub mod allocator;
pub mod free_block;
pub mod s_slice;

pub(crate) trait Anyway {
    fn anyway(self) -> SSlice;
}

impl Anyway for Result<SSlice, SSlice> {
    fn anyway(self) -> SSlice {
        match self {
            Ok(s) => s,
            Err(s) => s,
        }
    }
}
