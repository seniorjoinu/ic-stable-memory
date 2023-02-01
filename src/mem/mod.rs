use crate::stable;
use crate::encoding::{AsFixedSizeBytes, Buffer};

pub mod allocator;
pub mod free_block;
pub mod s_slice;

/*
pub type StablePtr = u64;

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

#[inline]
pub unsafe fn read_fixed_size<T: AsFixedSizeBytes>(ptr: StablePtr) -> T {
    let mut b = T::Buf::new(T::SIZE);
    
    
}*/