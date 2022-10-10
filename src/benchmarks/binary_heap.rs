#[cfg(test)]
mod binary_heap_benchmark {
    use crate::collections::binary_heap::SBinaryHeap;
    use crate::primitive::s_box::SBox;
    use crate::{init_allocator, measure, stable};
    use std::collections::BinaryHeap;

    const ITERATIONS: usize = 1_000_000;

    #[test]
    #[ignore]
    fn body_indirect() {
        {
            let mut classic_binary_heap = BinaryHeap::new();

            measure!("Classic binary heap push", ITERATIONS, {
                for _ in 0..ITERATIONS {
                    classic_binary_heap.push(String::from("Some short string"));
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
            stable::grow(1).unwrap();
            init_allocator(0);

            let mut stable_binary_heap = SBinaryHeap::new();

            measure!("Stable binary heap push", ITERATIONS, {
                for _ in 0..ITERATIONS {
                    let val = SBox::new(&String::from("Some short string"));
                    stable_binary_heap.push(&val);
                }
            });

            measure!("Stable binary heap peek", ITERATIONS, {
                for _ in 0..ITERATIONS {
                    stable_binary_heap.peek().unwrap();
                }
            });

            measure!("Stable binary heap pop", ITERATIONS, {
                for _ in 0..ITERATIONS {
                    unsafe { stable_binary_heap.pop().unwrap().drop() };
                }
            });
        }
    }

    #[test]
    #[ignore]
    fn body_direct() {
        {
            let mut classic_binary_heap = BinaryHeap::new();

            measure!("Classic binary heap push", ITERATIONS, {
                for i in 0..ITERATIONS {
                    classic_binary_heap.push(i);
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
            stable::grow(1).unwrap();
            init_allocator(0);

            let mut stable_binary_heap = SBinaryHeap::new();

            measure!("Stable binary heap push", ITERATIONS, {
                for i in 0..ITERATIONS {
                    stable_binary_heap.push(&i);
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
