#[cfg(test)]
mod log_benchmark {
    use crate::collections::log::SLog;
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
        }

        {
            stable::clear();
            stable_memory_init();

            let mut stable_log = SLog::new();

            measure!("Stable vec push", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_log.push(i as u64);
                }
            });

            measure!("Stable vec search", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_log.get(i as u64).unwrap();
                }
            });

            measure!("Stable vec pop", ITERATIONS, {
                for _ in 0..ITERATIONS {
                    stable_log.pop().unwrap();
                }
            });
        }
    }
}
