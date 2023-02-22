//! Traits and encoding algorithms used by this crate.
//!
//! Encoding relies on a notion of fixed and dynamically sized data. Fixed size encoding is performed
//! via custom algorithm heavily based on const generics. Dynamically sized data encoding can be overriden
//! by users of this crate, with a custom default implementation.
//!
//! By default this crate provides an implementation of [AsDynSizeBytes] trait for many types. But if
//! one wants to use their own serialization engine for unsized data (for example, [Speedy](https://docs.rs/speedy/latest/speedy/)),
//! they can do the following:
//! 
//! ```toml
//! // Cargo.toml
//! 
//! [dependencies]
//! ic-stable-memory = { version: "0.4", features = ["custom_dyn_encoding"] } 
//! ```
//! 
//! This will disable all default implementations of [AsDynSizeBytes] trait allowing you to implement
//! this trait by yourself in whatever way you prefer.

pub mod dyn_size;
pub mod fixed_size;

pub use dyn_size::AsDynSizeBytes;
pub use fixed_size::{AsFixedSizeBytes, Buffer};
