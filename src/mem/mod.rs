//! Various functions and data structures to organize and work with raw stable memory.
//!
//! This crate provides a notion of "stable memory ownership". This is a set of techniques, that
//! make data stored in stable memory appear like it gets automatically garbage collected by Rust's borrower.
//! Each stable memory primitive includes a `stable drop flag` - a special flag, that is used by
//! Rust to understand, whether it should release stable memory, when it naturally drops the value,
//! or not.
//!
//! Stable drop flag rules are simple:
//! 1. When you write something to stable memory, set its drop flag to `off` position.
//! 2. When you read something from stable memory, and you don't have an intention to move it,
//! set the drop flag to `off` position.
//! 3. When you read something from stable memory with an intention to move this data somewhere else,
//! set the drop flag to `on` position.
//! 4. When you [Drop] the value, if the drop flag is `on` - call [StableType::stable_drop](crate::StableType::stable_drop),
//! otherwise just [Drop].
//!
//! These rules are transparently managed at runtime. For users of this crate it appears like
//! stable memory can "own" some value. A set of lifetimes applied later, on the data structure layer,
//! to make it seem like the value is owned by a particular stable data structure.
//!
//! If you're thinking of implementing your own data structure using this crate, check [this](https://github.com/seniorjoinu/ic-stable-memory/docs/user-defined-data-structures.md)
//! document for more info on this topic.

use crate::encoding::{AsFixedSizeBytes, Buffer};
use crate::primitive::StableType;
use crate::stable;

pub mod allocator;
pub mod free_block;
pub mod s_slice;

/// A pointer to something is stable memory.
///
/// Just a handy alias for [u64].
pub type StablePtr = u64;
pub(crate) type StablePtrBuf = <u64 as AsFixedSizeBytes>::Buf;

#[inline]
pub(crate) fn stable_ptr_buf() -> StablePtrBuf {
    StablePtrBuf::new(<StablePtr as AsFixedSizeBytes>::SIZE)
}

/// Reads raw bytes from stable memory.
///
/// Under the hood simply calls [stable64_read](ic_cdk::api::stable::stable64_read).
///
/// # Safety
/// Make sure you're reading from a valid memory block. All kinds of bad things can happen.
/// Also, this function does not handle stable memory `ownership` in any way, so you have to make sure
/// your data won't get stable-dropped manually. See [crate::SBox] for an example of how this can be done.
#[inline]
pub unsafe fn read_bytes(ptr: StablePtr, buf: &mut [u8]) {
    stable::read(ptr, buf);
}

/// Write raw bytes to stable memory.
///
/// Under the hood simply calls [stable64_write](ic_cdk::api::stable::stable64_write).
///
/// # Safety
/// Make sure you're writing to a valid memory block. All kinds of bad things can happen.
/// Also, this function does not handle stable memory `ownership` in any way, so you have to make sure
/// your data won't get stable-dropped manually. See [SBox](crate::SBox) for an example of how this can be done.
#[inline]
pub unsafe fn write_bytes(ptr: StablePtr, buf: &[u8]) {
    stable::write(ptr, buf);
}

fn read_fixed<T: AsFixedSizeBytes>(ptr: StablePtr) -> T {
    let mut b = T::Buf::new(T::SIZE);
    stable::read(ptr, b._deref_mut());

    T::from_fixed_size_bytes(b._deref())
}

/// Reads a [StableType](crate::StableType) value *that won't move* implementing [AsFixedSizeBytes](crate::AsFixedSizeBytes) trait from stable memory.
///
/// See also [read_fixed_for_move].
///
/// This function creates an intermediate buffer of [AsFixedSizeBytes::SIZE](crate::AsFixedSizeBytes::SIZE) bytes,
/// reads bytes from stable memory into it, then deserializes this buffer into a value itself and
/// then sets its stable drop flag to `off` position.
///
/// # Safety
/// Make sure you're reading from a valid memory block. All kinds of bad things can happen.
/// This function manages stable memory `ownership`, this value *won't* be stable-dropped after it goes
/// out of scope. Make sure you treat it accordingly.
#[inline]
pub unsafe fn read_fixed_for_reference<T: AsFixedSizeBytes + StableType>(ptr: StablePtr) -> T {
    let mut it = read_fixed::<T>(ptr);
    it.stable_drop_flag_off();

    it
}

/// Reads a [StableType](crate::StableType) value *that will move* implementing [AsFixedSizeBytes](crate::AsFixedSizeBytes) trait from stable memory.
///
/// See also [read_fixed_for_reference].
///
/// This function creates an intermediate buffer of [AsFixedSizeBytes::SIZE](crate::AsFixedSizeBytes::SIZE) bytes,
/// reads bytes from stable memory into it, then deserializes this buffer into a value itself and
/// then sets its stable drop flag to `on` position.
///
/// # Safety
/// Make sure you're reading from a valid memory block. All kinds of bad things can happen.
/// This function manages stable memory `ownership`, this value *will* be stable-dropped after it goes
/// out of scope. Make sure you treat it accordingly.
#[inline]
pub unsafe fn read_fixed_for_move<T: AsFixedSizeBytes + StableType>(ptr: StablePtr) -> T {
    let mut it = read_fixed::<T>(ptr);
    it.stable_drop_flag_on();

    it
}

/// Writes a [StableType](crate::StableType) value implementing [AsFixedSizeBytes](crate::AsFixedSizeBytes) trait to stable memory.
///
/// This function creates an intermediate buffer of [AsFixedSizeBytes::SIZE](crate::AsFixedSizeBytes::SIZE) bytes,
/// serializes the value into that buffer, writes the buffer into stable memory and then sets the stable
/// drop flag of the value to `off` position.
///
/// # Safety
/// Make sure you're writing to a valid memory block. All kinds of bad things can happen.
/// This function manages stable memory `ownership`, this value *won't* be stable-dropped after it goes
/// out of scope. Make sure you treat it accordingly.
#[inline]
pub unsafe fn write_fixed<T: AsFixedSizeBytes + StableType>(ptr: StablePtr, it: &mut T) {
    it.stable_drop_flag_off();
    stable::write(ptr, it.as_new_fixed_size_bytes()._deref())
}

/// Wipes out stable memory, making it zero pages again.
///
/// Utility function which is only available for targets other than `wasm`. Useful for tests.
///
/// # Safety
/// Make sure to drop all previously created stable structures and reinit the allocator.
#[cfg(not(target_family = "wasm"))]
#[inline]
pub unsafe fn clear() {
    stable::clear();
}
