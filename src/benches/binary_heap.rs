#[cfg(test)]
mod binary_heap_benchmark {
    use crate::collections::binary_heap::SBinaryHeap;
    use crate::{measure, stable, stable_memory_init};
    use rand::seq::SliceRandom;
    use rand::thread_rng;
    use std::collections::BinaryHeap;

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
            let mut classic_binary_heap = BinaryHeap::new();

            measure!("Classic binary heap push", ITERATIONS, {
                for i in 0..ITERATIONS {
                    classic_binary_heap.push(example[i]);
                }
            });

            measure!("Classic binary heap peek", ITERATIONS, {
                for _ in 0..ITERATIONS {
                    classic_binary_heap.peek().unwrap();
                }
            });

            measure!("Classic binary heap pop", ITERATIONS, {
                for _ in 0..ITERATIONS {
                    classic_binary_heap.pop().unwrap();
                }
            });
        }

        {
            stable::clear();
            stable_memory_init();

            let mut stable_binary_heap = SBinaryHeap::new();

            measure!("Stable binary heap push", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_binary_heap.push(example[i]);
                }
            });

            measure!("Stable binary heap peek", ITERATIONS, {
                for _ in 0..ITERATIONS {
                    stable_binary_heap.peek().unwrap();
                }
            });

            measure!("Stable binary heap pop", ITERATIONS, {
                for _ in 0..ITERATIONS {
                    stable_binary_heap.pop().unwrap();
                }
            });
        }
    }
}
