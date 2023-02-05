#[cfg(test)]
mod btree_set_benchmark {
    use crate::collections::btree_set::SBTreeSet;
    use crate::{init_allocator, measure, stable, stable_memory_init};
    use rand::seq::SliceRandom;
    use rand::thread_rng;
    use std::collections::BTreeSet;

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
            let mut classic_btree_set = BTreeSet::new();

            measure!("Classic btree set insert", ITERATIONS, {
                for i in 0..ITERATIONS {
                    classic_btree_set.insert(example[i]);
                }
            });

            measure!("Classic btree set search", ITERATIONS, {
                for i in 0..ITERATIONS {
                    classic_btree_set.contains(&example[i]);
                }
            });

            measure!("Classic btree set remove", ITERATIONS, {
                for i in 0..ITERATIONS {
                    classic_btree_set.remove(&example[i]);
                }
            });
        }

        {
            stable::clear();
            stable_memory_init();

            let mut stable_btree_map = SBTreeSet::new();

            measure!("Stable btree set insert", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_btree_map.insert(example[i]);
                }
            });

            measure!("Stable btree set search", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_btree_map.contains(&example[i]);
                }
            });

            measure!("Stable btree set remove", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_btree_map.remove(&example[i]);
                }
            });
        }
    }
}
