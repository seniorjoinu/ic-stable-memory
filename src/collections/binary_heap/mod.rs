use crate::collections::vec::SVec;
use crate::primitive::StackAllocated;
use copy_as_bytes::traits::AsBytes;
use speedy::{Context, LittleEndian, Readable, Reader, Writable, Writer};
use std::mem::size_of;

pub struct SBinaryHeap<T> {
    arr: SVec<T>,
}

// Max heap
impl<T> SBinaryHeap<T> {
    #[inline]
    pub fn new() -> Self {
        Self { arr: SVec::new() }
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

// https://stackoverflow.com/questions/6531543/efficient-implementation-of-binary-heaps

impl<'a, T: AsBytes + Ord> SBinaryHeap<T>
where
    [(); T::SIZE]: Sized,
{
    #[inline]
    pub fn peek(&self) -> Option<T> {
        self.arr.get_copy(0)
    }

    #[inline]
    pub unsafe fn drop(self) {
        self.arr.drop();
    }

    pub fn push(&mut self, elem: T) {
        self.arr.push(elem);
        let len = self.len();
        if len == 1 {
            return;
        }

        let mut idx = len - 1;
        let elem = self.arr.get_copy(idx).unwrap();

        loop {
            let parent_idx = idx / 2;
            let parent = self.arr.get_copy(parent_idx).unwrap();

            if elem > parent {
                self.arr.swap(idx, parent_idx);
                idx = parent_idx;

                if idx > 0 {
                    continue;
                }
            }

            break;
        }
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
                if parent < left_child {
                    self.arr.swap(idx, left_child_idx);
                }

                // this is the last iteration, we can return here
                // because our binary tree is always complete
                return Some(elem);
            }

            let right_child = self.arr.get_copy(right_child_idx).unwrap();

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

            return Some(elem);
        }
    }
}

impl<T> Default for SBinaryHeap<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, T> Readable<'a, LittleEndian> for SBinaryHeap<T> {
    fn read_from<R: Reader<'a, LittleEndian>>(
        reader: &mut R,
    ) -> Result<Self, <speedy::LittleEndian as Context>::Error> {
        let arr = SVec::read_from(reader)?;

        Ok(Self { arr })
    }
}

impl<T> Writable<LittleEndian> for SBinaryHeap<T> {
    fn write_to<W: ?Sized + Writer<LittleEndian>>(
        &self,
        writer: &mut W,
    ) -> Result<(), <speedy::LittleEndian as Context>::Error> {
        self.arr.write_to(writer)
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::binary_heap::SBinaryHeap;
    use crate::{stable, stable_memory_init};

    #[test]
    fn heap_sort_works_fine() {
        stable::clear();
        stable_memory_init(true, 0);

        let example = vec![10u32, 20, 30, 40, 50, 60, 70, 80, 90, 100];
        let mut max_heap = SBinaryHeap::default();

        assert!(max_heap.is_empty());

        // insert example values in random order
        max_heap.push(80);
        max_heap.push(100);
        max_heap.push(50);
        max_heap.push(10);
        max_heap.push(90);
        max_heap.push(60);
        max_heap.push(70);
        max_heap.push(20);
        max_heap.push(40);
        max_heap.push(30);

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
    }
}
