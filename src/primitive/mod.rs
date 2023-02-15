use candid::{Int, Nat, Principal};
use serde_bytes::ByteBuf;
use std::collections::{BTreeSet, HashSet};

pub mod s_box;
pub mod s_ref;
pub mod s_ref_mut;

pub trait StableType {
    #[inline]
    unsafe fn assume_owned_by_stable_memory(&mut self) {}

    #[inline]
    unsafe fn assume_not_owned_by_stable_memory(&mut self) {}

    #[inline]
    fn is_owned_by_stable_memory(&self) -> bool {
        false
    }

    #[inline]
    unsafe fn stable_drop(&mut self) {}
}

impl StableType for () {}
impl StableType for bool {}
impl StableType for u8 {}
impl StableType for i8 {}
impl StableType for u16 {}
impl StableType for i16 {}
impl StableType for u32 {}
impl StableType for i32 {}
impl StableType for u64 {}
impl StableType for i64 {}
impl StableType for u128 {}
impl StableType for i128 {}
impl StableType for usize {}
impl StableType for isize {}
impl StableType for f32 {}
impl StableType for f64 {}

impl<const N: usize> StableType for [(); N] {}
impl<const N: usize> StableType for [bool; N] {}
impl<const N: usize> StableType for [u8; N] {}
impl<const N: usize> StableType for [i8; N] {}
impl<const N: usize> StableType for [u16; N] {}
impl<const N: usize> StableType for [i16; N] {}
impl<const N: usize> StableType for [u32; N] {}
impl<const N: usize> StableType for [i32; N] {}
impl<const N: usize> StableType for [u64; N] {}
impl<const N: usize> StableType for [i64; N] {}
impl<const N: usize> StableType for [u128; N] {}
impl<const N: usize> StableType for [i128; N] {}
impl<const N: usize> StableType for [usize; N] {}
impl<const N: usize> StableType for [isize; N] {}
impl<const N: usize> StableType for [f32; N] {}
impl<const N: usize> StableType for [f64; N] {}

impl StableType for Principal {}
impl StableType for Nat {}
impl StableType for Int {}

impl StableType for ByteBuf {}

impl<T: StableType> StableType for Option<T> {
    #[inline]
    unsafe fn assume_owned_by_stable_memory(&mut self) {
        if let Some(it) = self.as_mut() {
            it.assume_owned_by_stable_memory();
        }
    }

    #[inline]
    unsafe fn assume_not_owned_by_stable_memory(&mut self) {
        if let Some(it) = self.as_mut() {
            it.assume_not_owned_by_stable_memory();
        }
    }
}

impl StableType for String {}
impl StableType for Vec<u8> {}
impl StableType for Vec<i8> {}
impl StableType for Vec<u16> {}
impl StableType for Vec<i16> {}
impl StableType for Vec<u32> {}
impl StableType for Vec<i32> {}
impl StableType for Vec<u64> {}
impl StableType for Vec<i64> {}
impl StableType for Vec<u128> {}
impl StableType for Vec<i128> {}
impl StableType for Vec<usize> {}
impl StableType for Vec<isize> {}
impl StableType for Vec<f32> {}
impl StableType for Vec<f64> {}
impl StableType for Vec<()> {}
impl StableType for Vec<bool> {}

impl StableType for Vec<Principal> {}
impl StableType for Vec<Nat> {}
impl StableType for Vec<Int> {}

impl StableType for HashSet<u8> {}
impl StableType for HashSet<i8> {}
impl StableType for HashSet<u16> {}
impl StableType for HashSet<i16> {}
impl StableType for HashSet<u32> {}
impl StableType for HashSet<i32> {}
impl StableType for HashSet<u64> {}
impl StableType for HashSet<i64> {}
impl StableType for HashSet<u128> {}
impl StableType for HashSet<i128> {}
impl StableType for HashSet<usize> {}
impl StableType for HashSet<isize> {}
impl StableType for HashSet<f32> {}
impl StableType for HashSet<f64> {}
impl StableType for HashSet<()> {}
impl StableType for HashSet<bool> {}

impl StableType for BTreeSet<u8> {}
impl StableType for BTreeSet<i8> {}
impl StableType for BTreeSet<u16> {}
impl StableType for BTreeSet<i16> {}
impl StableType for BTreeSet<u32> {}
impl StableType for BTreeSet<i32> {}
impl StableType for BTreeSet<u64> {}
impl StableType for BTreeSet<i64> {}
impl StableType for BTreeSet<u128> {}
impl StableType for BTreeSet<i128> {}
impl StableType for BTreeSet<usize> {}
impl StableType for BTreeSet<isize> {}
impl StableType for BTreeSet<f32> {}
impl StableType for BTreeSet<f64> {}
impl StableType for BTreeSet<()> {}
impl StableType for BTreeSet<bool> {}
