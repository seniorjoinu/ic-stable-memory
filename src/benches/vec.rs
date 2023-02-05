#[cfg(test)]
mod vec_benchmark {
    use crate::collections::vec::SVec;
    use crate::{init_allocator, stable};
    use crate::{measure, stable_memory_init};

    const ITERATIONS: usize = 1_000_000;

    #[test]
    #[ignore]
    fn body_direct() {
        {
            let mut classic_vec = Vec::new();

            measure!("Classic vec push", ITERATIONS, {
                for i in 0..ITERATIONS {
                    classic_vec.push(i as u64);
                }
            });

            measure!("Classic vec search", ITERATIONS, {
                for i in 0..ITERATIONS {
                    classic_vec.get(i).unwrap();
                }
            });

            measure!("Classic vec pop", ITERATIONS, {
                for _ in 0..ITERATIONS {
                    classic_vec.pop().unwrap();
                }
            });

            measure!("Classic vec insert", ITERATIONS / 10, {
                for i in 0..(ITERATIONS / 10) {
                    classic_vec.insert(0, i as u64);
                }
            });

            measure!("Classic vec remove", ITERATIONS / 10, {
                for _ in 0..(ITERATIONS / 10) {
                    classic_vec.remove(0);
                }
            });
        }

        {
            stable::clear();
            stable_memory_init();

            let mut stable_vec = SVec::new();

            measure!("Stable vec push", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_vec.push(i as u64);
                }
            });

            measure!("Stable vec search", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_vec.get(i).unwrap();
                }
            });

            measure!("Stable vec pop", ITERATIONS, {
                for _ in 0..ITERATIONS {
                    stable_vec.pop().unwrap();
                }
            });

            measure!("Stable vec insert", ITERATIONS / 10, {
                for i in 0..(ITERATIONS / 10) {
                    stable_vec.insert(0, i as u64);
                }
            });

            measure!("Stable vec remove", ITERATIONS / 10, {
                for _ in 0..(ITERATIONS / 10) {
                    stable_vec.remove(0);
                }
            });
        }
    }
}
