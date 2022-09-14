#[cfg(test)]
mod vec_benchmark {
    use crate::collections::vec::SVec;
    use crate::measure;
    use crate::{init_allocator, stable};

    const ITERATIONS: usize = 1_000_000;

    #[test]
    #[ignore]
    fn body() {
        {
            let mut classic_vec = Vec::new();

            measure!("Classic vec push", ITERATIONS, {
                for _ in 0..ITERATIONS {
                    classic_vec.push(String::from("Some short string"));
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
            stable::grow(1).unwrap();
            init_allocator(0);

            let mut stable_vec = SVec::new();

            measure!("Stable vec push", ITERATIONS, {
                for _ in 0..ITERATIONS {
                    stable_vec.push(&String::from("Some short string"));
                }
            });

            measure!("Stable vec search", ITERATIONS, {
                for i in 0..ITERATIONS as u64 {
                    stable_vec.get_cloned(i).unwrap();
                }
            });

            measure!("Stable vec pop", ITERATIONS, {
                for _ in 0..ITERATIONS {
                    stable_vec.pop().unwrap();
                }
            });
        }
    }
}
