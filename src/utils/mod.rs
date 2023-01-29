#[allow(unused_imports)]
use ic_cdk::{print, trap};

pub mod certification;
pub mod encoding;
pub mod math;
pub mod mem_context;

pub struct MemMetrics {
    pub available: u64,
    pub free: u64,
    pub allocated: u64,
}

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
