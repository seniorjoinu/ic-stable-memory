use candid::{CandidType, Deserialize};

pub mod encode;
pub mod math;
pub mod mem_context;
pub mod vars;

#[derive(CandidType, Deserialize)]
pub struct MemMetrics {
    pub available: u64,
    pub free: u64,
    pub allocated: u64,
}
