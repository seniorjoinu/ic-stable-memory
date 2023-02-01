pub mod fixed_size;
pub mod dyn_size;

pub use fixed_size::{AsFixedSizeBytes, Buffer};
pub use dyn_size::AsDynSizeBytes;