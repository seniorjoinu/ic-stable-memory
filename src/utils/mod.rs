use crate::mem::s_slice::SSlice;

pub mod certification;
pub mod math;
pub mod mem_context;

pub struct MemMetrics {
    pub available: u64,
    pub free: u64,
    pub allocated: u64,
}

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

#[cfg(target_family = "wasm")]
use ic_cdk::print;

#[cfg(target_family = "wasm")]
#[inline]
pub fn isoprint(str: &str) {
    print(str)
}

#[cfg(not(target_family = "wasm"))]
#[inline]
pub fn isoprint(str: &str) {
    println!("{}", str)
}
