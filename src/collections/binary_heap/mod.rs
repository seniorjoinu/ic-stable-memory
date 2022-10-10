use crate::collections::vec::SVec;
use crate::primitive::{NotReference, StackAllocated};
use speedy::{Readable, Writable};

#[derive(Readable, Writable)]
pub enum SHeapType {
    Min,
    Max,
}

#[derive(Readable, Writable)]
pub struct SBinaryHeap<T, A> {
    ty: SHeapType,
    arr: SVec<T, A>,
}

impl<T, A> SBinaryHeap<T, A> {
    #[inline]
    pub fn new() -> Self {
        Self::new_with_type(SHeapType::Max)
    }

    pub fn new_with_type(ty: SHeapType) -> Self {
        Self {
            ty,
            arr: SVec::new(),
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.arr.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.arr.is_empty()
    }
}

impl<A: AsRef<[u8]> + AsMut<[u8]>, T: StackAllocated<T, A> + NotReference + Ord> SBinaryHeap<T, A> {
    pub fn push(&mut self, elem: &T) {
        self.arr.push(elem);
        let len = self.len();
        if len == 1 {
            return;
        }

        let mut idx = len - 1;

        loop {
            let parent_idx = idx / 2;
            let parent = self.arr.get_copy(parent_idx).unwrap();

            let mut flag = false;

            match self.ty {
                SHeapType::Min => {
                    if elem < &parent {
                        flag = true;
                    }
                }
                SHeapType::Max => {
                    if elem > &parent {
                        flag = true;
                    }
                }
            };

            if flag {
                self.arr.swap(idx, parent_idx);
                idx = parent_idx;

                if idx > 0 {
                    continue;
                }
            }

            break;
        }
    }

    #[inline]
    pub fn peek(&mut self) -> Option<T> {
        self.arr.get_copy(0)
    }

    pub fn pop(&mut self) -> Option<T> {
        let len = self.len();

        if len <= 1 {
            return self.arr.pop();
        }

        self.arr.swap(0, len - 1);
        let elem = self.arr.pop().unwrap();

        let last_idx = len - 2;

        let mut idx = 0;

        loop {
            let parent = self.arr.get_copy(idx).unwrap();

            let left_child_idx = (idx + 1) * 2 - 1;
            let right_child_idx = (idx + 1) * 2;

            if left_child_idx > last_idx {
                return Some(elem);
            }

            let left_child = self.arr.get_copy(left_child_idx).unwrap();

            if right_child_idx > last_idx {
                let mut flag = false;

                match self.ty {
                    SHeapType::Min => {
                        if parent > left_child {
                            flag = true;
                        }
                    }
                    SHeapType::Max => {
                        if parent < left_child {
                            flag = true;
                        }
                    }
                };

                if flag {
                    self.arr.swap(idx, left_child_idx);
                }

                // this is the last iteration, we can return here
                // because our binary tree is always complete
                return Some(elem);
            }

            let right_child = self.arr.get_copy(right_child_idx).unwrap();

            match self.ty {
                SHeapType::Min => {
                    if left_child <= right_child && left_child < parent {
                        self.arr.swap(idx, left_child_idx);
                        idx = left_child_idx;

                        continue;
                    }

                    if right_child <= left_child && right_child < parent {
                        self.arr.swap(idx, right_child_idx);
                        idx = right_child_idx;

                        continue;
                    }
                }
                SHeapType::Max => {
                    if left_child >= right_child && left_child > parent {
                        self.arr.swap(idx, left_child_idx);
                        idx = left_child_idx;

                        continue;
                    }

                    if right_child >= left_child && right_child > parent {
                        self.arr.swap(idx, right_child_idx);
                        idx = right_child_idx;

                        continue;
                    }
                }
            }

            return Some(elem);
        }
    }

    #[inline]
    pub unsafe fn drop(self) {
        self.arr.drop();
    }
}

impl<T, A> Default for SBinaryHeap<T, A> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::binary_heap::{SBinaryHeap, SHeapType};
    use crate::{stable, stable_memory_init};

    #[test]
    fn heap_sort_works_fine() {
        stable::clear();
        stable_memory_init(true, 0);

        let example = vec![10u32, 20, 30, 40, 50, 60, 70, 80, 90, 100];
        let mut max_heap = SBinaryHeap::default();

        assert!(max_heap.is_empty());

        // insert example values in random order
        max_heap.push(&80);
        max_heap.push(&100);
        max_heap.push(&50);
        max_heap.push(&10);
        max_heap.push(&90);
        max_heap.push(&60);
        max_heap.push(&70);
        max_heap.push(&20);
        max_heap.push(&40);
        max_heap.push(&30);

        assert_eq!(max_heap.peek().unwrap(), 100);

        let mut probe = vec![];

        // pop all elements, push them to probe
        probe.insert(0, max_heap.pop().unwrap());
        probe.insert(0, max_heap.pop().unwrap());
        probe.insert(0, max_heap.pop().unwrap());
        probe.insert(0, max_heap.pop().unwrap());
        probe.insert(0, max_heap.pop().unwrap());
        probe.insert(0, max_heap.pop().unwrap());
        probe.insert(0, max_heap.pop().unwrap());
        probe.insert(0, max_heap.pop().unwrap());
        probe.insert(0, max_heap.pop().unwrap());
        probe.insert(0, max_heap.pop().unwrap());

        // probe should be the same as example
        assert_eq!(probe, example, "Invalid elements order (max)");

        unsafe { max_heap.drop() };

        // it should also work for the min heap
        let example = vec![100u32, 90, 90, 80, 70, 50, 40, 30, 20, 10];
        let mut min_heap = SBinaryHeap::new_with_type(SHeapType::Min);

        // insert example values in random order
        min_heap.push(&80);
        min_heap.push(&100);
        min_heap.push(&50);
        min_heap.push(&10);
        min_heap.push(&90);
        min_heap.push(&90);
        min_heap.push(&70);
        min_heap.push(&20);
        min_heap.push(&40);
        min_heap.push(&30);

        assert_eq!(min_heap.peek().unwrap(), 10);

        let mut probe = vec![];

        // pop all elements, push them to probe
        probe.insert(0, min_heap.pop().unwrap());
        probe.insert(0, min_heap.pop().unwrap());
        probe.insert(0, min_heap.pop().unwrap());
        probe.insert(0, min_heap.pop().unwrap());
        probe.insert(0, min_heap.pop().unwrap());
        probe.insert(0, min_heap.pop().unwrap());
        probe.insert(0, min_heap.pop().unwrap());
        probe.insert(0, min_heap.pop().unwrap());
        probe.insert(0, min_heap.pop().unwrap());
        probe.insert(0, min_heap.pop().unwrap());

        // probe should be the same as example
        assert_eq!(probe, example, "Invalid elements order (min)");

        unsafe { min_heap.drop() };
    }
}
