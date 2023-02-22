#[cfg(test)]
mod hash_map_benchmark {
    use crate::collections::hash_map::SHashMap;
    use crate::{measure, stable, stable_memory_init};
    use rand::seq::SliceRandom;
    use rand::thread_rng;
    use std::collections::HashMap;

    const ITERATIONS: usize = 1_000_000;

    #[test]
    #[ignore]
    fn body_direct() {
        let mut example = Vec::new();
        for i in 0..ITERATIONS {
            example.push(i);
        }
        example.shuffle(&mut thread_rng());

        {
            let mut classic_hash_map = HashMap::new();

            measure!("Classic hash map insert", ITERATIONS, {
                for i in 0..ITERATIONS {
                    classic_hash_map.insert(example[i], example[i]);
                }
            });

            measure!("Classic hash map search", ITERATIONS, {
                for i in 0..ITERATIONS {
                    classic_hash_map.get(&example[i]).unwrap();
                }
            });

            measure!("Classic hash map remove", ITERATIONS, {
                for i in 0..ITERATIONS {
                    classic_hash_map.remove(&example[i]).unwrap();
                }
            });
        }

        {
            stable::clear();
            stable_memory_init();

            let mut stable_hash_map = SHashMap::new();

            measure!("Stable hash map insert", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_hash_map.insert(example[i], example[i]);
                }
            });

            measure!("Stable hash map search", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_hash_map.get(&example[i]).unwrap();
                }
            });

            measure!("Stable hash map remove", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_hash_map.remove(&example[i]).unwrap();
                }
            });
        }
    }
}
