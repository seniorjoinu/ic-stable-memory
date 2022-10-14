use ic_cdk::{print, trap};
use smallvec::{smallvec, SmallVec};
use std::mem;
use std::mem::size_of;

pub mod ic_types;
pub mod math;
pub mod mem_context;
pub mod phantom_data;
pub mod vars;

pub struct MemMetrics {
    pub available: u64,
    pub free: u64,
    pub allocated: u64,
}

#[cfg(target_family = "wasm")]
pub fn isoprint(str: &str) {
    print(str)
}

#[cfg(not(target_family = "wasm"))]
pub fn isoprint(str: &str) {
    println!("{}", str)
}

#[cfg(target_family = "wasm")]
pub fn _isotrap(str: &str) {
    trap(str);
}

#[cfg(not(target_family = "wasm"))]
pub fn _isotrap(str: &str) {
    panic!("{}", str);
}

macro_rules! isotrap {
    ($($exprs:expr),*) => {{
        $crate::utils::_isotrap(format!($($exprs),*).as_str());
        unreachable!("");
    }};
}

pub(crate) use isotrap;

#[inline]
pub fn u8_smallvec(size: usize) -> SmallVec<[u8; 64]> {
    smallvec![0; size]
}
