use std::mem::size_of;

pub mod s_box;
pub mod s_box_mut;

pub trait StackAllocated<T, A>: Sized
where
    A: AsMut<[u8]> + AsRef<[u8]>,
{
    fn size_of_u8_array() -> usize;
    fn fixed_size_u8_array() -> A;
    fn to_u8_fixed_size_array(it: T) -> A;
    fn from_u8_fixed_size_array(arr: A) -> T;
}

impl<T> StackAllocated<T, [u8; size_of::<T>()]> for T
where
    T: NotReference + Copy,
{
    #[inline]
    fn size_of_u8_array() -> usize {
        size_of::<Self>()
    }

    #[inline]
    fn fixed_size_u8_array() -> [u8; size_of::<Self>()] {
        [0u8; size_of::<Self>()]
    }

    #[inline]
    fn to_u8_fixed_size_array(it: Self) -> [u8; size_of::<Self>()] {
        unsafe { *(&it as *const Self as *const [u8; size_of::<Self>()]) }
    }

    #[inline]
    fn from_u8_fixed_size_array(arr: [u8; size_of::<Self>()]) -> Self {
        unsafe { *(&arr as *const [u8; size_of::<Self>()] as *const Self) }
    }
}

pub auto trait NotReference {}
impl<'a, T> !NotReference for &'a T {}
impl<'a, T> !NotReference for &'a mut T {}
impl<T> !NotReference for *const T {}
impl<T> !NotReference for *mut T {}
