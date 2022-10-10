use crate::mem::allocator::EMPTY_PTR;
use crate::mem::s_slice::{SSlice, Side};
use crate::mem::Anyway;
use crate::primitive::StackAllocated;
use crate::utils::phantom_data::SPhantomData;
use crate::{allocate, deallocate, reallocate};
use speedy::{Readable, Writable};

const DEFAULT_CAPACITY: usize = 4;

#[derive(Readable, Writable)]
pub struct SVec<T, A> {
    ptr: u64,
    len: usize,
    cap: usize,
    _marker_t: SPhantomData<T>,
    _marker_a: SPhantomData<A>,
}

impl<T, A> SVec<T, A> {
    pub fn new() -> Self {
        Self::new_with_capacity(DEFAULT_CAPACITY)
    }

    pub fn new_with_capacity(capacity: usize) -> Self {
        Self {
            len: 0,
            cap: capacity,
            ptr: EMPTY_PTR,
            _marker_t: SPhantomData::new(),
            _marker_a: SPhantomData::new(),
        }
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.cap
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub unsafe fn drop(self) {
        if self.ptr != EMPTY_PTR {
            let slice = SSlice::from_ptr(self.ptr, Side::Start).unwrap();

            deallocate(slice);
        }
    }

    fn maybe_reallocate(&mut self, item_size: usize) {
        if self.ptr == EMPTY_PTR {
            self.ptr = allocate(self.cap * item_size).ptr;

            return;
        }

        if self.len() == self.capacity() {
            self.cap *= 2;
            let slice = SSlice::from_ptr(self.ptr, Side::Start).unwrap();

            self.ptr = reallocate(slice, self.cap * item_size).anyway().ptr;

            return;
        }
    }

    #[inline]
    fn to_offset_or_size(idx: usize, item_size: usize) -> usize {
        idx * item_size
    }
}

impl<A: AsRef<[u8]> + AsMut<[u8]>, T: StackAllocated<T, A>> SVec<T, A> {
    pub fn push(&mut self, element: &T) {
        self.maybe_reallocate(T::size_of_u8_array());

        let offset = Self::to_offset_or_size(self.len, T::size_of_u8_array());
        let elem_bytes = T::as_u8_slice(&element);

        SSlice::_write_bytes(self.ptr, offset, elem_bytes);

        self.len += 1;
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }

        self.len -= 1;

        let offset = Self::to_offset_or_size(self.len, T::size_of_u8_array());

        let mut elem_bytes = T::fixed_size_u8_array();
        SSlice::_read_bytes(self.ptr, offset, elem_bytes.as_mut());

        Some(T::from_u8_fixed_size_array(elem_bytes))
    }

    pub fn get_copy(&self, idx: usize) -> Option<T> {
        if idx >= self.len() || self.is_empty() {
            return None;
        }

        let offset = Self::to_offset_or_size(idx, T::size_of_u8_array());

        let mut elem_bytes = T::fixed_size_u8_array();
        SSlice::_read_bytes(self.ptr, offset, elem_bytes.as_mut());

        Some(T::from_u8_fixed_size_array(elem_bytes))
    }

    pub fn replace(&mut self, idx: usize, element: &T) -> T {
        assert!(idx < self.len(), "Out of bounds");

        let offset = Self::to_offset_or_size(idx, T::size_of_u8_array());

        let mut old_elem_bytes = T::fixed_size_u8_array();
        SSlice::_read_bytes(self.ptr, offset, old_elem_bytes.as_mut());

        let new_elem_bytes = T::as_u8_slice(element);
        SSlice::_write_bytes(self.ptr, offset, new_elem_bytes);

        T::from_u8_fixed_size_array(old_elem_bytes)
    }

    pub fn swap(&mut self, idx1: usize, idx2: usize) {
        assert!(idx1 < self.len(), "idx1 out of bounds");
        assert!(idx2 < self.len(), "idx2 out of bounds");
        assert!(idx1 != idx2, "Indices should differ");

        let offset1 = Self::to_offset_or_size(idx1, T::size_of_u8_array());
        let offset2 = Self::to_offset_or_size(idx2, T::size_of_u8_array());

        let mut elem_bytes_1 = T::fixed_size_u8_array();
        let mut elem_bytes_2 = T::fixed_size_u8_array();

        SSlice::_read_bytes(self.ptr, offset1, elem_bytes_1.as_mut());
        SSlice::_read_bytes(self.ptr, offset2, elem_bytes_2.as_mut());

        SSlice::_write_bytes(self.ptr, offset1, elem_bytes_2.as_ref());
        SSlice::_write_bytes(self.ptr, offset2, elem_bytes_1.as_ref());
    }
}

impl<A: AsRef<[u8]> + AsMut<[u8]>, T: StackAllocated<T, A>> Default for SVec<T, A> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::vec::SVec;
    use crate::init_allocator;
    use crate::utils::mem_context::stable;

    #[derive(Copy, Clone, Debug)]
    struct Test {
        a: usize,
        b: bool,
    }

    #[test]
    fn create_destroy_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut stable_vec = SVec::new();
        assert_eq!(stable_vec.capacity(), 4);
        assert_eq!(stable_vec.len(), 0);

        stable_vec.push(&10);
        assert_eq!(stable_vec.capacity(), 4);
        assert_eq!(stable_vec.len(), 1);

        unsafe { stable_vec.drop() };
    }

    #[test]
    fn push_pop_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut stable_vec = SVec::new();
        let count = 10usize;

        for i in 0..count {
            let it = Test { a: i, b: true };

            stable_vec.push(&it);
        }

        assert_eq!(stable_vec.len(), count, "Invalid len after push");

        for i in 0..count {
            let it = Test { a: i, b: false };

            stable_vec.replace(i, &it);
        }

        assert_eq!(stable_vec.len(), count, "Invalid len after push");

        for i in 0..count {
            let it = stable_vec.pop().unwrap();

            assert_eq!(it.a, count - 1 - i);
            assert!(!it.b);
        }

        assert_eq!(stable_vec.len(), 0, "Invalid len after pop");

        for i in 0..count {
            let it = Test { a: i, b: true };

            stable_vec.push(&it);
        }

        unsafe { stable_vec.drop() };
    }

    #[test]
    fn basic_flow_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut v = SVec::default();
        assert!(v.get_copy(100).is_none());

        v.push(&10);
        v.push(&20);

        assert_eq!(v.get_copy(0).unwrap(), 10);
        assert_eq!(v.get_copy(1).unwrap(), 20);
        assert_eq!(v.replace(0, &11), 10);

        unsafe { v.drop() };
    }
}
