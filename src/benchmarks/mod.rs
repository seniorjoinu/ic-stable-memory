use std::time::{SystemTime, UNIX_EPOCH};

mod binary_heap;
mod btree_map;
mod btree_set;
mod hash_map;
mod hash_set;
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
        let before = $crate::benchmarks::now_milli();
        $it;
        let after = $crate::benchmarks::now_milli();

        println!(
            "{} {} iterations: {} ms",
            stringify!($name),
            $iterations,
            after - before
        );
    };
}
