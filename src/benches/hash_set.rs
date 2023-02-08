#[cfg(test)]
mod hash_set_benchmark {
    use crate::collections::hash_set::SHashSet;
    use crate::{measure, stable, stable_memory_init};
    use rand::seq::SliceRandom;
    use rand::thread_rng;
    use std::collections::HashSet;

    const ITERATIONS: usize = 1_000_000;

    #[test]
    #[ignore]
    fn body() {
        let mut example = Vec::new();
        for i in 0..ITERATIONS {
            example.push(i);
        }
        example.shuffle(&mut thread_rng());

        {
            let mut classic_hash_set = HashSet::new();

            measure!("Classic hash set insert", ITERATIONS, {
                for i in 0..ITERATIONS {
                    classic_hash_set.insert(example[i]);
                }
            });

            measure!("Classic hash set search", ITERATIONS, {
                for i in 0..ITERATIONS {
                    classic_hash_set.contains(&example[i]);
                }
            });

            measure!("Classic hash set remove", ITERATIONS, {
                for i in 0..ITERATIONS {
                    classic_hash_set.remove(&example[i]);
                }
            });
        }

        {
            stable::clear();
            stable_memory_init();

            let mut stable_hash_set = SHashSet::new();

            measure!("Stable hash set insert", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_hash_set.insert(example[i]);
                }
            });

            measure!("Stable hash set search", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_hash_set.contains(&example[i]);
                }
            });

            measure!("Stable hash set remove", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_hash_set.remove(&example[i]);
                }
            });
        }
    }
}
