//! Various utilities used by this crate

#[doc(hidden)]
pub mod certification;
#[doc(hidden)]
pub mod math;
pub mod mem_context;
#[cfg(test)]
pub mod test;

#[cfg(target_family = "wasm")]
use ic_cdk::print;

#[cfg(target_family = "wasm")]
#[inline]
pub fn isoprint(str: &str) {
    print(str)
}

/// Prints a value to stdout. Locally uses `println!` macro, on canister uses [ic_cdk::print] function.
#[cfg(not(target_family = "wasm"))]
#[inline]
pub fn isoprint(str: &str) {
    println!("{}", str)
}

/// Unwraps a [Result], but does not require [Debug] to be implemented on `T`
pub trait DebuglessUnwrap<T> {
    #[doc(hidden)]
    fn debugless_unwrap(self) -> T;
}

impl<R, E> DebuglessUnwrap<R> for Result<R, E> {
    fn debugless_unwrap(self) -> R {
        match self {
            Err(_) => panic!("Unwrapped a Result type without debug info"),
            Ok(r) => r,
        }
    }
}
