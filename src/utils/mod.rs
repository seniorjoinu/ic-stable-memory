use ic_cdk::{print, trap};
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
#[allow(clippy::uninit_vec)]
pub unsafe fn uninit_u8_vec_of_size(size: usize) -> Vec<u8> {
    let mut vec = Vec::with_capacity(size);
    vec.set_len(size);

    vec
}
