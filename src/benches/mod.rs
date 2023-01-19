use std::time::{SystemTime, UNIX_EPOCH};

mod binary_heap;
mod btree_map;
mod btree_set;
mod certified_map;
mod hash_map;
mod hash_set;
mod log;
mod vec;

#[ignore]
pub fn now_milli() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}

#[macro_export]
macro_rules! measure {
    ($name:literal, $iterations:expr, $it:block) => {
        let before = $crate::benches::now_milli();
        $it;
        let after = $crate::benches::now_milli();

        println!(
            "{} {} iterations: {} ms",
            stringify!($name),
            $iterations,
            after - before
        );
    };
}
