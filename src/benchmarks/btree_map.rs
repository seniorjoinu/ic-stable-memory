#[cfg(test)]
mod btree_map_benchmark {
    use crate::collections::btree_map::SBTreeMap;
    use crate::{init_allocator, measure, stable};
    use std::collections::BTreeMap;

    const ITERATIONS: usize = 10_000;

    #[test]
    #[ignore]
    fn body() {
        {
            let mut classic_btree_map = BTreeMap::new();

            measure!("Classic btree map insert", ITERATIONS, {
                for i in 0..ITERATIONS {
                    classic_btree_map.insert(i, String::from("Some short string"));
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

            let mut stable_btree_map = SBTreeMap::new();

            measure!("Stable btree map insert", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_btree_map.insert(i, &String::from("Some short string"));
                }
            });

            measure!("Stable btree map search", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_btree_map.get_cloned(&i).unwrap();
                }
            });

            measure!("Stable btree map remove", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_btree_map.remove(&i).unwrap();
                }
            });
        }
    }
}
