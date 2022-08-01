#[cfg(test)]
mod hash_map_benchmark {
    use crate::collections::hash_map::SHashMap;
    use crate::{init_allocator, measure, stable};
    use std::collections::HashMap;

    const ITERATIONS: usize = 10_000;

    #[test]
    fn body() {
        {
            let mut classic_hash_map = HashMap::new();

            measure!("Classic hash map insert", ITERATIONS, {
                for i in 0..ITERATIONS {
                    classic_hash_map.insert(i, String::from("Some short string"));
                }
            });

            measure!("Classic hash map search", ITERATIONS, {
                for i in 0..ITERATIONS {
                    classic_hash_map.get(&i).unwrap();
                }
            });

            measure!("Classic hash map remove", ITERATIONS, {
                for i in 0..ITERATIONS {
                    classic_hash_map.remove(&i).unwrap();
                }
            });
        }

        {
            stable::clear();
            stable::grow(1).unwrap();
            init_allocator(0);

            let mut stable_hash_map = SHashMap::new();

            measure!("Stable hash map insert", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_hash_map.insert(i, String::from("Some short string"));
                }
            });

            measure!("Stable hash map search", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_hash_map.get_cloned(&i).unwrap();
                }
            });

            measure!("Stable hash map remove", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_hash_map.remove(&i).unwrap();
                }
            });
        }
    }
}
