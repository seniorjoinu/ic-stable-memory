use speedy::{Readable, Writable};

pub mod ic_types;
pub mod math;
pub mod mem_context;
pub mod phantom_data;
pub mod vars;

pub struct MemMetrics {
    pub available: u64,
    pub free: u64,
    pub allocated: u64,
}
