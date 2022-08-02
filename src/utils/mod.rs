use speedy::{Readable, Writable};

pub mod math;
pub mod mem_context;
pub mod phantom_data;
pub mod vars;

#[derive(Readable, Writable)]
pub struct MemMetrics {
    pub available: u64,
    pub free: u64,
    pub allocated: u64,
}
