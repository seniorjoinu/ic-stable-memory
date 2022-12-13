#[cfg(test)]
mod vec_benchmark {
    use crate::collections::vec::SVec;
    use crate::measure;
    use crate::primitive::s_box::SBox;
    use crate::{init_allocator, stable};

    const ITERATIONS: usize = 1_000_000;

    #[test]
    #[ignore]
    fn body_indirect() {
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
                    stable_vec.push(SBox::new(String::from("Some short string")));
                }
            });

            measure!("Stable vec search", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_vec.get_copy(i).unwrap();
                }
            });

            measure!("Stable vec pop", ITERATIONS, {
                for _ in 0..ITERATIONS {
                    stable_vec.pop().unwrap();
                }
            });
        }
    }

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
            stable::grow(1).unwrap();
            init_allocator(0);

            let mut stable_vec = SVec::new();

            measure!("Stable vec push", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_vec.push(i as u64);
                }
            });

            measure!("Stable vec search", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_vec.get_copy(i).unwrap();
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
