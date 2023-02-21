//! Smart-pointers and [StableType] trait

use candid::{Int, Nat, Principal};
use serde_bytes::ByteBuf;
use std::collections::{BTreeSet, HashSet};

/// [SBox] smart-pointer that allows storing dynamically-sized data to stable memory
pub mod s_box;

/// Immutable reference to fixed size data on stable memory
pub mod s_ref;

/// Mutable reference to fixed size data on stable memory
pub mod s_ref_mut;

/// Anything that can be stored on stable memory should implement this trait.
///
/// *None of methods of this trait should be called manually, unless you're implementing your own
/// stable data structure!*
///
/// This trait defines behavior for stable drop function and stable drop flag interactions.
/// In order to implement this trait for a data type, there are two options:
/// 1. Use [derive::StableType](crate::derive::StableType) derive macro. This macro requires that any
/// field of your data type also implements [StableType] trait. For most cases consider this option.
/// Supports non-generic structs and enums.
/// 2. Implement it yourself. For data types which do not contain any stable structures inside, default
/// implementation will work fine. For data types which do contain stable structures, the implementation
/// should definitely override [StableType::stable_drop_flag_off] and [StableType::stable_drop::flag_on]
/// calling the same methods on the underlying stable structures. If you want to implement your own
/// stable data structure, you should definitely implement this trait completely, overriding all of
/// its methods.
///
/// # Examples
/// ```rust
/// # use ic_stable_memory::collections::SVec;
/// # use ic_stable_memory::SBox;
/// # use ic_stable_memory::derive::StableType;
///
/// // implement using derive macro
/// #[derive(StableType)]
/// struct A {
///     x: u64,
///     y: [u32; 4],
///     z: Option<SVec<SBox<String>>>,
/// }
/// ```
///
/// ```rust
/// // provide default implementation, if the type does not contain any stable structures
/// # use ic_stable_memory::StableType;
/// struct A {
///     x: u64,
///     y: [u32; 4],
/// }
///
/// impl StableType for A {}
/// ```
///
/// ```rust
/// # use ic_stable_memory::StableType;
/// enum MyOption<T> {
///     Some(T),
///     None,
/// }
///
/// // provide proxy implementation for generics or types which contain stable data structures inside
/// impl<T: StableType> StableType for MyOption<T> {
///     unsafe fn stable_drop_flag_on(&mut self) {
///         if let MyOption::Some(it) = self {
///             it.stable_drop_flag_on();
///         }
///     }
///
///     unsafe fn stable_drop_flag_off(&mut self) {
///         if let MyOption::Some(it) = self {
///             it.stable_drop_flag_off();
///         }
///     }
/// }
/// ```
///
/// ```rust
/// // provide full implementation, if you're building new stable data structure
/// # use ic_stable_memory::mem::StablePtr;
/// # use ic_stable_memory::StableType;
/// struct ExampleStableDataStructure {
///     drop_flag: bool,
///     ptr: StablePtr,
/// }
///
/// impl StableType for ExampleStableDataStructure {
///     fn should_stable_drop(&self) -> bool {
///         self.drop_flag
///     }
///
///     unsafe fn stable_drop_flag_on(&mut self) {
///        self.drop_flag = true;
///     }
///
///     unsafe fn stable_drop_flag_off(&mut self) {
///         self.drop_flag = false;
///     }
///
///     unsafe fn stable_drop(&mut self) {
///         // deallocate any stable memory managed by this data structure
///     }
/// }
/// ```
pub trait StableType {
    /// Sets stable drop flag to `off` position, if it is applicable
    ///
    /// # Safety
    /// Setting stable drop flag to an invalid position will lead to memory leaks and unexpected panics
    /// and considered undefined behavior.
    #[inline]
    unsafe fn stable_drop_flag_off(&mut self) {}

    /// Should set stable drop flag to `on` position, if it is applicable
    ///
    /// # Safety
    /// Setting stable drop flag to an invalid position will lead to memory leaks and unexpected panics
    /// and considered undefined behavior.
    #[inline]
    unsafe fn stable_drop_flag_on(&mut self) {}

    /// Should return the value of the stable drop flag
    #[inline]
    fn should_stable_drop(&self) -> bool {
        false
    }

    /// Performs stable drop, releasing all underlying stable memory of this data structure
    ///
    /// You only want to implement this trait method, if you're building your own stable data structure.
    /// In that case you may want to call this method during [Drop], so your data structure will clean
    /// itself automatically. You may also want to call this method during some dropping or transforming
    /// methods of your data structure, like `into_inner()` or `into_<type_name>().
    ///
    /// # Safety
    /// Performing stable drop twice is undefined behavior.
    ///
    /// # Example
    /// Implementation of [Drop] for your stable data structure should look something like this
    /// ```rust
    /// # use ic_stable_memory::StableType;
    /// struct A {}
    ///
    /// impl StableType for A {
    ///     // implement as usual
    /// }
    ///
    /// impl Drop for A {
    ///     fn drop(&mut self) {
    ///         if self.should_stable_drop() {
    ///             unsafe { self.stable_drop(); }
    ///         }    
    ///     }
    /// }
    /// ```
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
impl StableType for char {}

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
impl<const N: usize> StableType for [char; N] {}

impl StableType for Principal {}
impl StableType for Nat {}
impl StableType for Int {}

impl StableType for ByteBuf {}
impl<T: StableType> StableType for Option<T> {
    #[inline]
    unsafe fn stable_drop_flag_on(&mut self) {
        if let Some(it) = self {
            it.stable_drop_flag_on();
        }
    }

    #[inline]
    unsafe fn stable_drop_flag_off(&mut self) {
        if let Some(it) = self {
            it.stable_drop_flag_off();
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
impl StableType for Vec<char> {}

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
impl StableType for HashSet<char> {}

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
impl StableType for BTreeSet<char> {}
