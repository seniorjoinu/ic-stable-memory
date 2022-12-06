#[cfg(test)]
mod btree_map_benchmark {
    use crate::collections::certified_hash_map::map::SCertifiedSet;
    use crate::{init_allocator, measure, stable};
    use std::collections::BTreeMap;

    const ITERATIONS: usize = 10_000;

    #[test]
    #[ignore]
    fn cmp_with_new_hashmap() {
        {
            let mut classic_btree_map = BTreeMap::new();

            measure!("Classic btree map insert", ITERATIONS, {
                for i in 0..ITERATIONS {
                    classic_btree_map.insert(i, i);
                }
            });

            measure!("Classic btree map search", ITERATIONS, {
                for i in 0..ITERATIONS {
                    classic_btree_map.get(&i).unwrap();
                }
            });

            measure!("Classic btree map remove", ITERATIONS, {
                for i in 0..ITERATIONS {
                    classic_btree_map.remove(&i).unwrap();
                }
            });
        }

        {
            stable::clear();
            stable::grow(1).unwrap();
            init_allocator(0);

            let mut stable_hashtree_map = SCertifiedSet::new();

            measure!("Stable certified hashmap insert", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_hashtree_map.insert(i, i);
                }
            });

            measure!("Stable certified hashmap witnessing", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_hashtree_map.witness_key(&i).unwrap();
                }
            });

            measure!("Stable certified hashmap remove", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_hashtree_map.remove(&i).unwrap();
                }
            });
        }
    }
}
