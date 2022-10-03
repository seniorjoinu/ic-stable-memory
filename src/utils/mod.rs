use ic_cdk::{print, trap};
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

pub auto trait NotReference {}

impl<'a, T> !NotReference for &'a T {}
impl<'a, T> !NotReference for &'a mut T {}

// FIGURE OUT HOW TO ENCODE IT

pub unsafe fn any_as_u8_slice<T>(p: &T) -> &[u8] {
    std::slice::from_raw_parts(std::mem::transmute(p), std::mem::size_of::<T>())
}

pub const unsafe fn u8_slice_as_any<T: Copy>(slice: &[u8]) -> T {
    if slice.len() != size_of::<T>() {
        unreachable!()
    } else {
        std::ptr::read(slice.as_ptr() as *const T)
    }
}

#[cfg(test)]
mod tests {
    use crate::utils::{any_as_u8_slice, u8_slice_as_any};
    use std::mem::size_of;

    #[derive(Copy, Clone, Eq, PartialEq, Debug)]
    struct A {
        pub x: u8,
        pub y: u16,
    }

    #[test]
    fn test() {
        unsafe {
            let a = A { x: 10, y: 300 };

            let slice = any_as_u8_slice(&a);

            let a2: A = u8_slice_as_any(slice);

            assert_eq!(a, a2);
        }
    }
}
