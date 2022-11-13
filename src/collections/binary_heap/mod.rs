use crate::collections::binary_heap::iter::SBinaryHeapIter;
use crate::collections::vec::SVec;
use crate::primitive::StableAllocated;
use copy_as_bytes::traits::{AsBytes, SuperSized};
use speedy::{Context, LittleEndian, Readable, Reader, Writable, Writer};
use std::fmt::{Debug, Formatter};

pub mod iter;

pub struct SBinaryHeap<T> {
    inner: SVec<T>,
}

// Max heap
impl<T> SBinaryHeap<T> {
    #[inline]
    pub fn new() -> Self {
        Self { inner: SVec::new() }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

// TODO: apply https://stackoverflow.com/questions/6531543/efficient-implementation-of-binary-heaps

impl<'a, T: StableAllocated + Ord> SBinaryHeap<T>
where
    [(); T::SIZE]: Sized,
{
    #[inline]
    pub fn peek(&self) -> Option<T> {
        self.inner.get_copy(0)
    }

    #[inline]
    pub unsafe fn stable_drop_collection(mut self) {
        self.inner.stable_drop_collection();
    }

    pub fn push(&mut self, elem: T) {
        self.inner.push(elem);
        let len = self.len();
        if len == 1 {
            return;
        }

        let mut idx = len - 1;
        let elem = self.inner.get_copy(idx).unwrap();

        loop {
            let parent_idx = idx / 2;
            let parent = self.inner.get_copy(parent_idx).unwrap();

            if elem > parent {
                self.inner.swap(idx, parent_idx);
                idx = parent_idx;

                if idx != 0 {
                    continue;
                }
            }

            break;
        }
    }

    pub fn pop(&mut self) -> Option<T> {
        let len = self.len();

        if len <= 1 {
            return self.inner.pop();
        }

        self.inner.swap(0, len - 1);
        let elem = self.inner.pop().unwrap();

        let last_idx = len - 2;

        let mut idx = 0;

        loop {
            let parent = self.inner.get_copy(idx).unwrap();

            // FIXME: check overflow
            let left_child_idx = (idx + 1) * 2 - 1;
            let right_child_idx = (idx + 1) * 2;

            if left_child_idx > last_idx {
                return Some(elem);
            }

            let left_child = self.inner.get_copy(left_child_idx).unwrap();

            if right_child_idx > last_idx {
                if parent < left_child {
                    self.inner.swap(idx, left_child_idx);
                }

                // this is the last iteration, we can return here
                // because our binary tree is always complete
                return Some(elem);
            }

            let right_child = self.inner.get_copy(right_child_idx).unwrap();

            if left_child >= right_child && left_child > parent {
                self.inner.swap(idx, left_child_idx);
                idx = left_child_idx;

                continue;
            }

            if right_child >= left_child && right_child > parent {
                self.inner.swap(idx, right_child_idx);
                idx = right_child_idx;

                continue;
            }

            return Some(elem);
        }
    }

    pub fn iter(&self) -> SBinaryHeapIter<T> {
        SBinaryHeapIter::new(self)
    }
}

impl<T: StableAllocated + Debug> Debug for SBinaryHeap<T>
where
    [(); T::SIZE]: Sized,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.inner.fmt(f)
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
        let inner = SVec::read_from(reader)?;

        Ok(Self { inner })
    }
}

impl<T> Writable<LittleEndian> for SBinaryHeap<T> {
    fn write_to<W: ?Sized + Writer<LittleEndian>>(
        &self,
        writer: &mut W,
    ) -> Result<(), <speedy::LittleEndian as Context>::Error> {
        self.inner.write_to(writer)
    }
}

impl<T> SuperSized for SBinaryHeap<T> {
    const SIZE: usize = SVec::<T>::SIZE;
}

impl<T: StableAllocated> AsBytes for SBinaryHeap<T> {
    #[inline]
    fn to_bytes(self) -> [u8; Self::SIZE] {
        self.inner.to_bytes()
    }

    #[inline]
    fn from_bytes(arr: [u8; Self::SIZE]) -> Self {
        let inner = SVec::<T>::from_bytes(arr);
        Self { inner }
    }
}

impl<T: StableAllocated> StableAllocated for SBinaryHeap<T>
where
    [u8; T::SIZE]: Sized,
{
    #[inline]
    fn move_to_stable(&mut self) {
        self.inner.move_to_stable()
    }

    #[inline]
    fn remove_from_stable(&mut self) {
        self.inner.remove_from_stable()
    }

    #[inline]
    unsafe fn stable_drop(self) {
        self.inner.stable_drop();
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::binary_heap::SBinaryHeap;
    use crate::primitive::StableAllocated;
    use crate::{init_allocator, stable, stable_memory_init};
    use copy_as_bytes::traits::AsBytes;
    use speedy::{Readable, Writable};

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

        println!("{:?}", max_heap);

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

        unsafe { max_heap.stable_drop() };
    }

    #[test]
    fn iter_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut heap = SBinaryHeap::default();

        for i in 0..100 {
            heap.push(i);
        }

        let mut c = 0;
        for i in heap.iter() {
            c += 1;

            assert!(i < 100);
        }

        assert_eq!(c, 100);
    }

    #[test]
    fn serialization_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let heap = SBinaryHeap::<u32>::default();
        let buf = heap.write_to_vec().unwrap();
        let heap1 = SBinaryHeap::<u32>::read_from_buffer_copying_data(&buf).unwrap();

        assert_eq!(heap.inner.ptr, heap1.inner.ptr);
        assert_eq!(heap.inner.len, heap1.inner.len);
        assert_eq!(heap.inner.cap, heap1.inner.cap);

        let ptr = heap.inner.ptr;
        let len = heap.inner.len;
        let cap = heap.inner.cap;

        let buf = heap.to_bytes();
        let heap1 = SBinaryHeap::<u32>::from_bytes(buf);

        assert_eq!(ptr, heap1.inner.ptr);
        assert_eq!(len, heap1.inner.len);
        assert_eq!(cap, heap1.inner.cap);
    }

    #[test]
    fn helpers_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut heap = SBinaryHeap::<u32>::default();
        heap.move_to_stable(); // does nothing
        heap.remove_from_stable(); // does nothing
        unsafe { heap.stable_drop_collection() };
    }
}