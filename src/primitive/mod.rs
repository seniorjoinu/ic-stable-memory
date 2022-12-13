use crate::utils::encoding::AsFixedSizeBytes;

pub mod s_box;
pub mod s_box_mut;

pub trait StableAllocated: AsFixedSizeBytes {
    fn move_to_stable(&mut self);
    fn remove_from_stable(&mut self);

    unsafe fn stable_drop(self);
}

macro_rules! impl_for_primitive {
    ($ty:ty) => {
        impl StableAllocated for $ty {
            #[inline]
            fn move_to_stable(&mut self) {}

            #[inline]
            fn remove_from_stable(&mut self) {}

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
