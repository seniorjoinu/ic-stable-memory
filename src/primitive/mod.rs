use crate::encoding::AsFixedSizeBytes;
use candid::{Int, Nat, Principal};

pub mod s_box;
pub mod s_ref;
pub mod s_ref_mut;

pub trait StableType {
    #[inline]
    fn stable_memory_consume(&mut self) {}

    #[inline]
    fn stable_memory_unconsume(&mut self) {}

    #[inline]
    fn is_consumed_by_stable_memory(&self) -> bool {
        false
    }

    #[inline]
    unsafe fn free(&mut self) {}
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

impl StableType for Vec<u8> {}
impl StableType for Principal {}
impl StableType for Nat {}
impl StableType for Int {}

pub trait StableDrop {
    type Output;

    unsafe fn stable_drop(self) -> Self::Output;
}

pub trait StableAllocated: AsFixedSizeBytes {
    fn move_to_stable(&mut self);
    fn remove_from_stable(&mut self);
}

macro_rules! impl_for_primitive {
    ($ty:ty) => {
        impl StableAllocated for $ty {
            #[inline]
            fn move_to_stable(&mut self) {}

            #[inline]
            fn remove_from_stable(&mut self) {}
        }

        impl StableDrop for $ty {
            type Output = ();

            #[inline]
            unsafe fn stable_drop(self) {}
        }
    };
}

impl_for_primitive!(u8);
impl_for_primitive!(u16);
impl_for_primitive!(u32);
impl_for_primitive!(u64);
impl_for_primitive!(u128);
impl_for_primitive!(usize);
impl_for_primitive!(i8);
impl_for_primitive!(i16);
impl_for_primitive!(i32);
impl_for_primitive!(i64);
impl_for_primitive!(i128);
impl_for_primitive!(isize);
impl_for_primitive!(f32);
impl_for_primitive!(f64);
impl_for_primitive!(bool);
impl_for_primitive!(());

impl_for_primitive!([u8; 0]);
impl_for_primitive!([u8; 1]);
impl_for_primitive!([u8; 2]);
impl_for_primitive!([u8; 4]);
impl_for_primitive!([u8; 8]);
impl_for_primitive!([u8; 16]);
impl_for_primitive!([u8; 30]); // for principals
impl_for_primitive!([u8; 32]);
impl_for_primitive!([u8; 64]);
impl_for_primitive!([u8; 128]);
impl_for_primitive!([u8; 256]);
impl_for_primitive!([u8; 512]);
impl_for_primitive!([u8; 1024]);
impl_for_primitive!([u8; 2048]);
impl_for_primitive!([u8; 4096]);

impl_for_primitive!(Principal);
impl_for_primitive!(Nat);
impl_for_primitive!(Int);
