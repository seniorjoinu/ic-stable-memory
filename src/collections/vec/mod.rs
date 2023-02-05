use crate::collections::vec::iter::SVecIter;
use crate::encoding::{AsFixedSizeBytes, Buffer};
use crate::mem::allocator::EMPTY_PTR;
use crate::mem::s_slice::{SSlice, Side};
use crate::mem::StablePtr;
use crate::primitive::s_ref::SRef;
use crate::primitive::s_ref_mut::SRefMut;
use crate::primitive::StableType;
use crate::utils::Anyway;
use crate::{allocate, deallocate, reallocate};
use std::cmp::Ordering;
use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;

pub mod iter;

const DEFAULT_CAPACITY: usize = 4;

pub struct SVec<T: StableType + AsFixedSizeBytes> {
    ptr: u64,
    len: usize,
    cap: usize,
    is_owned: bool,
    _marker_t: PhantomData<T>,
}

impl<T: StableType + AsFixedSizeBytes> SVec<T> {
    #[inline]
    pub fn new() -> Self {
        Self::new_with_capacity(DEFAULT_CAPACITY)
    }

    #[inline]
    pub fn new_with_capacity(capacity: usize) -> Self {
        Self {
            len: 0,
            cap: capacity,
            ptr: EMPTY_PTR,
            is_owned: false,
            _marker_t: PhantomData::default(),
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
}

impl<T: StableType + AsFixedSizeBytes> SVec<T> {
    fn maybe_reallocate(&mut self) {
        if self.ptr == EMPTY_PTR {
            self.ptr = allocate(self.capacity() * T::SIZE).as_ptr();
            return;
        }

        if self.len() == self.capacity() {
            self.cap *= 2;
            let slice = SSlice::from_ptr(self.ptr, Side::Start).unwrap();

            self.ptr = reallocate(slice, self.cap * T::SIZE).anyway().as_ptr();
        }
    }

    #[inline]
    pub fn push(&mut self, mut element: T) {
        self.maybe_reallocate();

        let elem_ptr = SSlice::_make_ptr_by_offset(self.ptr, self.len * T::SIZE);
        unsafe { crate::mem::write_and_own_fixed(elem_ptr, &mut element) };

        self.len += 1;
    }

    #[inline]
    pub fn pop(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }

        let elem_ptr = self.get_element_ptr(self.len - 1)?;
        self.len -= 1;

        Some(unsafe { crate::mem::read_and_disown_fixed(elem_ptr) })
    }

    #[inline]
    pub fn get(&self, idx: usize) -> Option<SRef<'_, T>> {
        let ptr = self.get_element_ptr(idx)?;

        Some(SRef::new(ptr))
    }

    #[inline]
    pub fn get_mut(&mut self, idx: usize) -> Option<SRefMut<'_, T>> {
        let ptr = self.get_element_ptr(idx)?;

        Some(SRefMut::new(ptr))
    }

    pub fn replace(&mut self, idx: usize, mut element: T) -> T {
        assert!(idx < self.len(), "Out of bounds");

        let elem_ptr = SSlice::_make_ptr_by_offset(self.ptr, idx * T::SIZE);

        let prev_element = unsafe { crate::mem::read_and_disown_fixed(elem_ptr) };
        unsafe { crate::mem::write_and_own_fixed(elem_ptr, &mut element) };

        prev_element
    }

    pub fn insert(&mut self, idx: usize, mut element: T) {
        if idx == self.len {
            self.push(element);
            return;
        }

        assert!(idx < self.len, "out of bounds");

        self.maybe_reallocate();

        let elem_ptr = SSlice::_make_ptr_by_offset(self.ptr, idx * T::SIZE);

        // moving elements after idx one slot to the right
        let mut buf = vec![0u8; (self.len - idx) * T::SIZE];
        unsafe { crate::mem::read_bytes(elem_ptr, &mut buf) };
        unsafe { crate::mem::write_bytes(elem_ptr + T::SIZE as u64, &buf) };

        // writing the element
        unsafe { crate::mem::write_and_own_fixed(elem_ptr, &mut element) };

        self.len += 1;
    }

    pub fn remove(&mut self, idx: usize) -> T {
        if idx == self.len - 1 {
            return unsafe { self.pop().unwrap_unchecked() };
        }

        assert!(idx < self.len - 1, "out of bounds");

        let elem_ptr = SSlice::_make_ptr_by_offset(self.ptr, idx * T::SIZE);
        let elem = unsafe { crate::mem::read_and_disown_fixed(elem_ptr) };

        let mut buf = vec![0u8; (self.len - idx - 1) * T::SIZE];
        unsafe { crate::mem::read_bytes(elem_ptr + T::SIZE as u64, &mut buf) };
        unsafe { crate::mem::write_bytes(elem_ptr, &buf) };

        self.len -= 1;

        elem
    }

    pub fn swap(&mut self, idx1: usize, idx2: usize) {
        assert!(
            idx1 < self.len() && idx2 < self.len() && idx1 != idx2,
            "invalid idx"
        );

        let ptr1 = SSlice::_make_ptr_by_offset(self.ptr, idx1 * T::SIZE);
        let ptr2 = SSlice::_make_ptr_by_offset(self.ptr, idx2 * T::SIZE);

        let mut buf_1 = T::Buf::new(T::SIZE);
        let mut buf_2 = T::Buf::new(T::SIZE);

        unsafe { crate::mem::read_bytes(ptr1, buf_1._deref_mut()) };
        unsafe { crate::mem::read_bytes(ptr2, buf_2._deref_mut()) };

        unsafe { crate::mem::write_bytes(ptr2, buf_1._deref()) };
        unsafe { crate::mem::write_bytes(ptr1, buf_2._deref()) };
    }

    #[inline]
    pub fn clear(&mut self) {
        while self.pop().is_some() {}
    }

    pub fn binary_search_by<FN>(&self, mut f: FN) -> Result<usize, usize>
    where
        FN: FnMut(&T) -> Ordering,
    {
        if self.is_empty() {
            return Err(0);
        }

        let mut min = 0;
        let mut max = self.len;
        let mut mid = (max - min) / 2;

        loop {
            let elem_ptr = SSlice::_make_ptr_by_offset(self.ptr, mid * T::SIZE);
            let elem = unsafe { crate::mem::read_fixed_for_reference(elem_ptr) };

            let res = f(&elem);

            match res {
                Ordering::Equal => return Ok(mid),
                Ordering::Greater => {
                    max = mid;
                    let new_mid = (max - min) / 2 + min;

                    if new_mid == mid {
                        return Err(mid);
                    }

                    mid = new_mid;
                    continue;
                }
                Ordering::Less => {
                    min = mid;
                    let new_mid = (max - min) / 2 + min;

                    if new_mid == mid {
                        return Err(mid + 1);
                    }

                    mid = new_mid;
                    continue;
                }
            }
        }
    }

    #[inline]
    pub fn iter(&self) -> SVecIter<T> {
        SVecIter::new(self)
    }

    pub fn debug_print(&self) {
        print!("SVec[");
        for i in 0..self.len {
            let mut b = T::Buf::new(T::SIZE);
            unsafe {
                crate::mem::read_bytes(
                    SSlice::_make_ptr_by_offset(self.ptr, i * T::SIZE),
                    b._deref_mut(),
                )
            };

            print!("{:?}", b._deref());

            if i < self.len - 1 {
                print!(", ");
            }
        }

        println!("]");
    }

    pub(crate) fn get_element_ptr(&self, idx: usize) -> Option<StablePtr> {
        if idx < self.len() {
            Some(SSlice::_make_ptr_by_offset(self.ptr, idx * T::SIZE))
        } else {
            None
        }
    }
}

impl<T: StableType + AsFixedSizeBytes> Default for SVec<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: StableType + AsFixedSizeBytes> From<SVec<T>> for Vec<T> {
    fn from(mut svec: SVec<T>) -> Self {
        let mut vec = Self::new();

        while let Some(it) = svec.pop() {
            vec.insert(0, it);
        }

        vec
    }
}

impl<T: StableType + AsFixedSizeBytes> From<Vec<T>> for SVec<T> {
    fn from(mut vec: Vec<T>) -> Self {
        let mut svec = Self::new();

        for _ in 0..vec.len() {
            svec.push(vec.remove(0));
        }

        svec
    }
}

impl<T: StableType + AsFixedSizeBytes + Debug> Debug for SVec<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("[")?;
        for (idx, item) in self.iter().enumerate() {
            item.fmt(f)?;

            if idx < self.len - 1 {
                f.write_str(", ")?;
            }
        }
        f.write_str("]")
    }
}

impl<T: StableType + AsFixedSizeBytes> AsFixedSizeBytes for SVec<T> {
    const SIZE: usize = u64::SIZE + usize::SIZE + usize::SIZE;
    type Buf = [u8; u64::SIZE + usize::SIZE + usize::SIZE];

    fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
        self.ptr.as_fixed_size_bytes(&mut buf[0..u64::SIZE]);
        self.len
            .as_fixed_size_bytes(&mut buf[u64::SIZE..(u64::SIZE + usize::SIZE)]);
        self.cap.as_fixed_size_bytes(
            &mut buf[(u64::SIZE + usize::SIZE)..(u64::SIZE + usize::SIZE * 2)],
        );
    }

    fn from_fixed_size_bytes(arr: &[u8]) -> Self {
        let ptr = u64::from_fixed_size_bytes(&arr[0..u64::SIZE]);
        let len = usize::from_fixed_size_bytes(&arr[u64::SIZE..(u64::SIZE + usize::SIZE)]);
        let cap = usize::from_fixed_size_bytes(
            &arr[(u64::SIZE + usize::SIZE)..(u64::SIZE + usize::SIZE * 2)],
        );

        Self {
            ptr,
            len,
            cap,
            is_owned: false,
            _marker_t: PhantomData::default(),
        }
    }
}

impl<T: StableType + AsFixedSizeBytes> StableType for SVec<T> {
    #[inline]
    unsafe fn assume_owned_by_stable_memory(&mut self) {
        self.is_owned = true;
    }

    #[inline]
    unsafe fn assume_not_owned_by_stable_memory(&mut self) {
        self.is_owned = false;
    }

    #[inline]
    fn is_owned_by_stable_memory(&self) -> bool {
        self.is_owned
    }

    unsafe fn stable_drop(&mut self) {
        if self.ptr != EMPTY_PTR {
            self.clear();

            let slice = SSlice::from_ptr(self.ptr, Side::Start).unwrap();

            deallocate(slice);
        }
    }
}

impl<T: StableType + AsFixedSizeBytes> Drop for SVec<T> {
    fn drop(&mut self) {
        if !self.is_owned_by_stable_memory() {
            unsafe {
                self.stable_drop();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::vec::{SVec, DEFAULT_CAPACITY};
    use crate::encoding::{AsFixedSizeBytes, Buffer};
    use crate::primitive::s_box::SBox;
    use crate::primitive::StableType;
    use crate::utils::mem_context::stable;
    use crate::{_debug_print_allocator, init_allocator, stable_memory_init};

    #[derive(Copy, Clone, Debug)]
    struct Test {
        a: usize,
        b: bool,
    }

    impl AsFixedSizeBytes for Test {
        const SIZE: usize = usize::SIZE + bool::SIZE;
        type Buf = [u8; Self::SIZE];

        fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
            self.a.as_fixed_size_bytes(&mut buf[0..usize::SIZE]);
            self.b
                .as_fixed_size_bytes(&mut buf[usize::SIZE..(usize::SIZE + bool::SIZE)]);
        }

        fn from_fixed_size_bytes(arr: &[u8]) -> Self {
            let a = usize::from_fixed_size_bytes(&arr[0..usize::SIZE]);
            let b = bool::from_fixed_size_bytes(&arr[usize::SIZE..(usize::SIZE + bool::SIZE)]);

            Self { a, b }
        }
    }

    impl StableType for Test {}

    #[test]
    fn create_destroy_work_fine() {
        stable::clear();
        stable_memory_init();

        let mut stable_vec = SVec::new();
        assert_eq!(stable_vec.capacity(), DEFAULT_CAPACITY);
        assert_eq!(stable_vec.len(), 0);

        stable_vec.push(10);
        assert_eq!(stable_vec.capacity(), DEFAULT_CAPACITY);
        assert_eq!(stable_vec.len(), 1);
    }

    #[test]
    fn push_pop_work_fine() {
        stable::clear();
        stable_memory_init();

        let mut stable_vec = SVec::new();
        let count = 10usize;

        for i in 0..count {
            let it = Test { a: i, b: true };

            stable_vec.push(it);
        }

        assert_eq!(stable_vec.len(), count, "Invalid len after push");

        for i in 0..count {
            let it = Test { a: i, b: false };

            stable_vec.replace(i, it);
        }

        stable_vec.debug_print();
        assert_eq!(stable_vec.len(), count, "Invalid len after push");

        for i in 0..count {
            println!("{} {}", i, stable_vec.len());

            let it = stable_vec.pop().unwrap();

            assert_eq!(stable_vec.len(), count - i - 1);
            assert_eq!(it.a, count - 1 - i);
            assert!(!it.b);
        }

        assert_eq!(stable_vec.len(), 0, "Invalid len after pop");

        for i in 0..count {
            let it = Test { a: i, b: true };

            stable_vec.push(it);
        }
    }

    #[test]
    fn basic_flow_works_fine() {
        stable::clear();
        stable_memory_init();

        let mut v = SVec::default();
        assert!(v.get(100).is_none());

        v.push(10);
        v.push(20);

        assert_eq!(*v.get(0).unwrap(), 10);
        assert_eq!(*v.get(1).unwrap(), 20);
        assert_eq!(v.replace(0, 11), 10);
    }

    #[test]
    fn insert_works_fine() {
        stable::clear();
        stable_memory_init();

        let mut array = SVec::default();
        let mut check = Vec::default();

        for i in 0..30 {
            array.insert(0, 29 - i);
            check.insert(0, 29 - i);
        }

        for i in 60..100 {
            array.insert(array.len(), i);
            check.insert(check.len(), i);
        }

        for i in 30..60 {
            array.insert(30 + (i - 30), i);
            check.insert(30 + (i - 30), i);
        }

        for i in 0..100 {
            assert_eq!(*array.get(i).unwrap(), i);
        }

        let actual: Vec<_> = Vec::from(array);
        assert_eq!(actual, check);
    }

    #[test]
    fn binary_search_work_fine() {
        stable::clear();
        stable_memory_init();

        // 0..100 randomly shuffled
        let initial = vec![
            3, 24, 46, 92, 2, 21, 34, 95, 82, 22, 88, 32, 59, 13, 73, 51, 12, 83, 28, 17, 9, 23, 5,
            63, 62, 38, 20, 40, 25, 98, 30, 43, 57, 86, 42, 4, 99, 33, 11, 74, 96, 94, 47, 31, 37,
            71, 80, 70, 14, 67, 93, 56, 27, 39, 58, 41, 29, 84, 8, 0, 45, 54, 7, 26, 97, 6, 81, 65,
            79, 10, 91, 68, 36, 60, 76, 75, 15, 87, 49, 35, 78, 64, 69, 52, 50, 61, 48, 53, 44, 19,
            55, 72, 90, 77, 89, 16, 85, 66, 18, 1,
        ];

        let mut array = SVec::<i32>::default();
        let mut check = Vec::<i32>::new();

        for i in 0..100 {
            match check.binary_search_by(|it| it.cmp(&initial[i])) {
                Err(idx) => check.insert(idx, initial[i]),
                _ => unreachable!(),
            }

            match array.binary_search_by(|it| it.cmp(&initial[i])) {
                Err(idx) => array.insert(idx, initial[i]),
                _ => unreachable!(),
            }
        }

        let actual = Vec::from(array);
        assert_eq!(actual, check);
    }

    #[test]
    fn remove_works_fine() {
        stable::clear();
        stable_memory_init();

        let initial = vec![
            3, 24, 46, 92, 2, 21, 34, 95, 82, 22, 88, 32, 59, 13, 73, 51, 12, 83, 28, 17, 9, 23, 5,
            63, 62, 38, 20, 40, 25, 98, 30, 43, 57, 86, 42, 4, 99, 33, 11, 74, 96, 94, 47, 31, 37,
            71, 80, 70, 14, 67, 93, 56, 27, 39, 58, 41, 29, 84, 8, 0, 45, 54, 7, 26, 97, 6, 81, 65,
            79, 10, 91, 68, 36, 60, 76, 75, 15, 87, 49, 35, 78, 64, 69, 52, 50, 61, 48, 53, 44, 19,
            55, 72, 90, 77, 89, 16, 85, 66, 18, 1,
        ];

        let mut array = SVec::from(initial.clone());

        for i in 0..initial.len() {
            assert_eq!(array.remove(0), initial[i]);
        }
    }

    #[test]
    fn serialization_works_fine() {
        stable::clear();
        stable_memory_init();

        let vec = SVec::<u32>::new_with_capacity(10);
        let mut buf = <SVec<u32> as AsFixedSizeBytes>::Buf::new(SVec::<u32>::SIZE);
        vec.as_fixed_size_bytes(buf._deref_mut());
        let vec1 = SVec::<u32>::from_fixed_size_bytes(buf._deref());

        assert_eq!(vec.ptr, vec1.ptr);
        assert_eq!(vec.len, vec1.len);
        assert_eq!(vec.cap, vec1.cap);

        let ptr = vec.ptr;
        let len = vec.len;
        let cap = vec.cap;

        let mut buf = <SVec<u32> as AsFixedSizeBytes>::Buf::new(SVec::<u32>::SIZE);
        vec.as_fixed_size_bytes(buf._deref_mut());
        let vec1 = SVec::<u32>::from_fixed_size_bytes(buf._deref());

        assert_eq!(ptr, vec1.ptr);
        assert_eq!(len, vec1.len);
        assert_eq!(cap, vec1.cap);
    }

    #[test]
    fn iter_works_fine() {
        stable::clear();
        stable_memory_init();

        let mut vec = SVec::new();
        for i in 0..100 {
            vec.push(i);
        }

        let mut c = 0;
        vec.debug_print();

        for (idx, mut i) in vec.iter().enumerate() {
            c += 1;

            assert_eq!(idx as i32, *i);
        }

        assert_eq!(c, 100);
    }

    #[test]
    fn sboxes_work_fine() {
        stable::clear();
        stable_memory_init();

        {
            let mut vec = SVec::new();

            for i in 0..100 {
                vec.debug_print();
                let b = SBox::new(10);

                println!("{}", b.as_ptr());

                vec.push(b);
            }

            vec.debug_print();
        }

        _debug_print_allocator();
    }
}
