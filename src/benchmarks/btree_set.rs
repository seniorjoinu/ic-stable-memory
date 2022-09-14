#[cfg(test)]
mod btree_map_benchmark {
    use crate::collections::btree_set::SBTreeSet;
    use crate::{init_allocator, measure, stable};
    use std::collections::BTreeSet;

    const ITERATIONS: usize = 10_000;

    #[test]
    #[ignore]
    fn body() {
        {
            let mut classic_btree_set = BTreeSet::new();

            measure!("Classic btree set insert", ITERATIONS, {
                for i in 0..ITERATIONS {
                    classic_btree_set.insert(i);
                }
            });

            measure!("Classic btree set search", ITERATIONS, {
                for i in 0..ITERATIONS {
                    classic_btree_set.contains(&i);
                }
            });

            measure!("Classic btree set remove", ITERATIONS, {
                for i in 0..ITERATIONS {
                    classic_btree_set.remove(&i);
                }
            });
        }

        {
            stable::clear();
            stable::grow(1).unwrap();
            init_allocator(0);

            let mut stable_btree_map = SBTreeSet::new();

            measure!("Stable btree set insert", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_btree_map.insert(i);
                }
            });

            measure!("Stable btree set search", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_btree_map.contains(&i);
                }
            });

            measure!("Stable btree set remove", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_btree_map.remove(&i);
                }
            });
        }
    }
}
