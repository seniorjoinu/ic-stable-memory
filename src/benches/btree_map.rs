#[cfg(test)]
mod btree_map_benchmark {
    use crate::collections::btree_map::SBTreeMap;
    use crate::{init_allocator, measure, stable, stable_memory_init};
    use rand::seq::SliceRandom;
    use rand::thread_rng;
    use std::collections::BTreeMap;

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
            let mut classic_btree_map = BTreeMap::new();

            measure!("Classic btree map insert", ITERATIONS, {
                for i in 0..ITERATIONS {
                    classic_btree_map.insert(example[i], example[i]);
                }
            });

            measure!("Classic btree map search", ITERATIONS, {
                for i in 0..ITERATIONS {
                    classic_btree_map.get(&example[i]).unwrap();
                }
            });

            measure!("Classic btree map remove", ITERATIONS, {
                for i in 0..ITERATIONS {
                    classic_btree_map.remove(&example[i]).unwrap();
                }
            });
        }

        {
            stable::clear();
            stable_memory_init();

            let mut stable_btree_map = SBTreeMap::new();

            measure!("Stable btree map insert", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_btree_map.insert(example[i], example[i]);
                }
            });

            measure!("Stable btree map search", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_btree_map.get(&example[i]).unwrap();
                }
            });

            measure!("Stable btree map remove", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_btree_map.remove(&example[i]).unwrap();
                }
            });
        }
    }
}
