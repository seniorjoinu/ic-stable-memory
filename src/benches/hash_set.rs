#[cfg(test)]
mod hash_set_benchmark {
    use crate::collections::hash_set::SHashSet;
    use crate::{init_allocator, measure, stable};
    use std::collections::HashSet;

    const ITERATIONS: usize = 100_000;

    #[test]
    #[ignore]
    fn body() {
        {
            let mut classic_hash_set = HashSet::new();

            measure!("Classic hash set insert", ITERATIONS, {
                for i in 0..ITERATIONS {
                    classic_hash_set.insert(i);
                }
            });

            measure!("Classic hash set search", ITERATIONS, {
                for i in 0..ITERATIONS {
                    classic_hash_set.contains(&i);
                }
            });

            measure!("Classic hash set remove", ITERATIONS, {
                for i in 0..ITERATIONS {
                    classic_hash_set.remove(&i);
                }
            });
        }

        {
            stable::clear();
            stable::grow(1).unwrap();
            init_allocator(0);

            let mut stable_hash_set = SHashSet::new();

            measure!("Stable hash set insert", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_hash_set.insert(i);
                }
            });

            measure!("Stable hash set search", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_hash_set.contains(&i);
                }
            });

            measure!("Stable hash set remove", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_hash_set.remove(&i);
                }
            });
        }
    }
}
