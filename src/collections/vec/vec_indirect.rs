use crate::collections::vec::vec_direct::SVecDirect;
use crate::primitive::s_unsafe_cell::SUnsafeCell;
use crate::utils::phantom_data::SPhantomData;
use speedy::{LittleEndian, Readable, Writable};

#[derive(Readable, Writable)]
pub struct SVec<T> {
    inner: SVecDirect<u64>,
    #[speedy(skip)]
    _data: SPhantomData<T>,
}

impl<'a, T: Readable<'a, LittleEndian> + Writable<LittleEndian>> SVec<T> {
    pub fn new() -> Self {
        Self {
            inner: SVecDirect::new(),
            _data: SPhantomData::new(),
        }
    }

    pub fn push(&mut self, element: &T) {
        let elem_cell = SUnsafeCell::new(element);
        let elem_ptr = unsafe { elem_cell.as_ptr() };

        self.inner.push(&elem_ptr);
    }

    pub fn pop(&mut self) -> Option<T> {
        let elem_ptr = self.inner.pop()?;

        let elem_cell = unsafe { SUnsafeCell::<T>::from_ptr(elem_ptr) };
        let elem = elem_cell.get_cloned();
        elem_cell.drop();

        Some(elem)
    }

    pub fn get_cloned(&mut self, idx: u64) -> Option<T> {
        let elem_ptr = self.inner.get_cloned(idx)?;
        let elem_cell = unsafe { SUnsafeCell::<T>::from_ptr(elem_ptr) };
        let elem = elem_cell.get_cloned();

        Some(elem)
    }

    pub fn replace(&mut self, idx: u64, element: &T) -> T {
        assert!(idx < self.len(), "Out of bounds");
        let new_elem_cell = SUnsafeCell::new(element);
        let new_elem_ptr = unsafe { new_elem_cell.as_ptr() };

        let prev_elem_ptr = self.inner.replace(idx, &new_elem_ptr);
        let prev_elem_cell = unsafe { SUnsafeCell::<T>::from_ptr(prev_elem_ptr) };
        let prev_elem = prev_elem_cell.get_cloned();

        prev_elem_cell.drop();

        prev_elem
    }

    pub fn swap(&mut self, idx1: u64, idx2: u64) {
        self.inner.swap(idx1, idx2);
    }

    pub fn drop(mut self) {
        loop {
            if self.pop().is_none() {
                break;
            }
        }

        self.inner.drop();
    }

    pub fn capacity(&self) -> u64 {
        self.inner.capacity()
    }

    pub fn len(&self) -> u64 {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn is_about_to_grow(&self) -> bool {
        self.len() == self.capacity()
    }

    pub fn recache_sectors(&mut self) {
        self.inner.recache_sectors();
    }
}

impl<'a, T: Readable<'a, LittleEndian> + Writable<LittleEndian>> Default for SVec<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::vec::vec_indirect::SVec;
    use crate::init_allocator;
    use crate::utils::mem_context::stable;
    use speedy::{Readable, Writable};

    #[derive(Readable, Writable, Debug)]
    struct Test {
        a: u64,
        b: String,
    }

    #[test]
    fn create_destroy_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut stable_vec = SVec::<u64>::new();
        assert_eq!(stable_vec.capacity(), 0);
        assert_eq!(stable_vec.len(), 0);

        stable_vec.push(&10);
        assert_eq!(stable_vec.capacity(), 4);
        assert_eq!(stable_vec.len(), 1);

        stable_vec.drop();
    }

    #[test]
    fn push_pop_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut stable_vec = SVec::new();
        let count = 10u64;

        for i in 0..count {
            let it = Test {
                a: i,
                b: format!("Str {}", i),
            };

            stable_vec.push(&it);
        }

        assert_eq!(stable_vec.len(), count, "Invalid len after push");

        for i in 0..count {
            let it = Test {
                a: i,
                b: format!("String of the element {}", i),
            };

            stable_vec.replace(i, &it);
        }

        assert_eq!(stable_vec.len(), count, "Invalid len after push");

        for i in 0..count {
            let it = stable_vec.pop().unwrap();

            assert_eq!(it.a, count - 1 - i);
            assert_eq!(it.b, format!("String of the element {}", count - 1 - i));
        }

        assert_eq!(stable_vec.len(), 0, "Invalid len after pop");

        for i in 0..count {
            let it = Test {
                a: i,
                b: format!("Str {}", i),
            };

            stable_vec.push(&it);
        }

        stable_vec.drop();
    }

    #[test]
    fn basic_flow_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut v = SVec::<u64>::default();
        assert!(v.get_cloned(100).is_none());

        v.push(&10);
        v.push(&20);

        assert_eq!(v.replace(0, &11), 10);

        v.drop();
    }
}
