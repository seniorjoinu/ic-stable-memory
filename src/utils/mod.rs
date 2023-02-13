pub mod certification;
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

#[cfg(not(target_family = "wasm"))]
#[inline]
pub fn isoprint(str: &str) {
    println!("{}", str)
}

pub trait DebuglessUnwrap<T> {
    fn debugless_unwrap(self) -> T;
}

impl<R, E> DebuglessUnwrap<R> for Result<R, E> {
    fn debugless_unwrap(self) -> R {
        match self {
            Err(_) => panic!("Unwrapped a Result type without debug info"),
            Ok(r) => r
        }
    }
}