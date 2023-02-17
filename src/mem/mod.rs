use crate::encoding::{AsFixedSizeBytes, Buffer};
use crate::primitive::StableType;
use crate::stable;

pub mod allocator;
pub mod free_block;
pub mod s_slice;

pub type StablePtr = u64;
pub type StablePtrBuf = <u64 as AsFixedSizeBytes>::Buf;

#[inline]
pub fn stable_ptr_buf() -> StablePtrBuf {
    StablePtrBuf::new(<StablePtr as AsFixedSizeBytes>::SIZE)
}

#[cfg(not(target_family = "wasm"))]
#[inline]
pub unsafe fn clear() {
    stable::clear();
}

#[inline]
pub unsafe fn read_bytes(ptr: StablePtr, buf: &mut [u8]) {
    stable::read(ptr, buf);
}

#[inline]
pub unsafe fn write_bytes(ptr: StablePtr, buf: &[u8]) {
    stable::write(ptr, buf);
}

fn read_fixed<T: AsFixedSizeBytes>(ptr: StablePtr) -> T {
    let mut b = T::Buf::new(T::SIZE);
    stable::read(ptr, b._deref_mut());

    T::from_fixed_size_bytes(b._deref())
}

#[inline]
pub unsafe fn read_fixed_for_reference<T: AsFixedSizeBytes + StableType>(ptr: StablePtr) -> T {
    let mut it = read_fixed::<T>(ptr);
    it.stable_drop_flag_off();

    it
}

#[inline]
pub unsafe fn read_fixed_for_move<T: AsFixedSizeBytes + StableType>(ptr: StablePtr) -> T {
    let mut it = read_fixed::<T>(ptr);
    it.stable_drop_flag_on();

    it
}

#[inline]
pub unsafe fn write_fixed<T: AsFixedSizeBytes + StableType>(ptr: StablePtr, it: &mut T) {
    it.stable_drop_flag_off();
    stable::write(ptr, it.as_new_fixed_size_bytes()._deref())
}
