use std::mem::size_of;

pub mod s_box;
pub mod s_box_mut;
pub mod s_ref_cell;
pub mod s_unsafe_cell;

pub trait StackAllocated<T, A>
where
    A: AsRef<[u8]> + AsMut<[u8]>,
{
    fn size_of_u8_array() -> usize;
    fn fixed_size_u8_array() -> A;
    fn as_u8_slice(it: &T) -> &[u8];
    fn from_u8_fixed_size_array(arr: A) -> T;
}

impl<T> StackAllocated<T, [u8; size_of::<T>()]> for T
where
    T: Copy + NotReference,
    [u8; size_of::<T>()]: Sized,
{
    #[inline]
    fn size_of_u8_array() -> usize {
        size_of::<T>()
    }

    #[inline]
    fn fixed_size_u8_array() -> [u8; size_of::<T>()] {
        [0u8; size_of::<T>()]
    }

    #[inline]
    fn as_u8_slice(it: &T) -> &[u8] {
        unsafe { std::slice::from_raw_parts(std::mem::transmute(it), size_of::<T>()) }
    }

    #[inline]
    fn from_u8_fixed_size_array(arr: [u8; size_of::<T>()]) -> T {
        unsafe { *(&arr as *const [u8; size_of::<T>()] as *const T) }
    }
}

pub auto trait NotReference {}
impl<'a, T> !NotReference for &'a T {}
impl<'a, T> !NotReference for &'a mut T {}
