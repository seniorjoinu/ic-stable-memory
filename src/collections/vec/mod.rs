use crate::collections::vec::iter::SVecIter;
use crate::mem::allocator::EMPTY_PTR;
use crate::mem::s_slice::{SSlice, Side};
use crate::mem::Anyway;
use crate::primitive::StableAllocated;
use crate::utils::encoding::{AsFixedSizeBytes, FixedSize};
use crate::{allocate, deallocate, reallocate};
use std::cmp::Ordering;
use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;

pub mod iter;

const DEFAULT_CAPACITY: usize = 4;

pub struct SVec<T> {
    pub(crate) ptr: u64,
    pub(crate) len: usize,
    pub(crate) cap: usize,
    pub(crate) _marker_t: PhantomData<T>,
}

impl<T> SVec<T> {
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

impl<T: StableAllocated> SVec<T>
where
    [u8; T::SIZE]: Sized,
{
    fn maybe_reallocate(&mut self) {
        if self.ptr == EMPTY_PTR {
            self.ptr = allocate(self.capacity() * T::SIZE).get_ptr();
            return;
        }

        if self.len() == self.capacity() {
            self.cap *= 2;
            let slice = SSlice::from_ptr(self.ptr, Side::Start).unwrap();

            self.ptr = reallocate(slice, self.cap * T::SIZE).anyway().get_ptr();
        }
    }

    pub fn push(&mut self, mut element: T) {
        self.maybe_reallocate();

        element.move_to_stable();

        let buf = element.as_fixed_size_bytes();
        SSlice::_write_bytes(self.ptr, self.len * T::SIZE, &buf);

        self.len += 1;
    }

    pub fn pop(&mut self) -> Option<T> {
        if !self.is_empty() {
            self.len -= 1;

            let mut buf = T::_u8_arr_of_size();
            SSlice::_read_bytes(self.ptr, self.len * T::SIZE, &mut buf);

            let mut it = T::from_fixed_size_bytes(&buf);
            it.remove_from_stable();

            Some(it)
        } else {
            None
        }
    }

    pub fn get_copy(&self, idx: usize) -> Option<T> {
        if idx < self.len() {
            let mut buf = T::_u8_arr_of_size();
            SSlice::_read_bytes(self.ptr, idx * T::SIZE, &mut buf);

            Some(T::from_fixed_size_bytes(&buf))
        } else {
            None
        }
    }

    pub fn replace(&mut self, idx: usize, mut element: T) -> T {
        assert!(idx < self.len(), "Out of bounds");

        let mut buf = T::_u8_arr_of_size();
        SSlice::_read_bytes(self.ptr, idx * T::SIZE, &mut buf);
        let mut prev_element = T::from_fixed_size_bytes(&buf);

        prev_element.remove_from_stable();
        element.move_to_stable();

        let buf = element.as_fixed_size_bytes();
        SSlice::_write_bytes(self.ptr, idx * T::SIZE, &buf);

        prev_element
    }

    pub fn insert(&mut self, idx: usize, mut element: T) {
        assert!(idx <= self.len, "out of bounds");

        if idx == self.len {
            self.push(element);
            return;
        }

        self.maybe_reallocate();

        let mut buf = vec![0u8; (self.len - idx) * T::SIZE];
        SSlice::_read_bytes(self.ptr, idx * T::SIZE, &mut buf);
        SSlice::_write_bytes(self.ptr, (idx + 1) * T::SIZE, &buf);

        element.move_to_stable();

        let buf = element.as_fixed_size_bytes();
        SSlice::_write_bytes(self.ptr, idx * T::SIZE, &buf);

        self.len += 1;
    }

    pub fn remove(&mut self, idx: usize) -> T {
        assert!(idx < self.len, "out of bounds");

        if idx == self.len - 1 {
            return unsafe { self.pop().unwrap_unchecked() };
        }

        let mut buf = T::_u8_arr_of_size();
        SSlice::_read_bytes(self.ptr, idx * T::SIZE, &mut buf);
        let mut it = T::from_fixed_size_bytes(&buf);

        it.remove_from_stable();

        let mut buf = vec![0u8; (self.len - idx - 1) * T::SIZE];
        SSlice::_read_bytes(self.ptr, (idx + 1) * T::SIZE, &mut buf);
        SSlice::_write_bytes(self.ptr, idx * T::SIZE, &buf);

        self.len -= 1;

        it
    }

    pub fn swap(&mut self, idx1: usize, idx2: usize) {
        assert!(
            idx1 < self.len() && idx2 < self.len() && idx1 != idx2,
            "invalid idx"
        );

        let mut buf_1 = T::_u8_arr_of_size();
        let mut buf_2 = T::_u8_arr_of_size();

        SSlice::_read_bytes(self.ptr, idx1 * T::SIZE, &mut buf_1);
        SSlice::_read_bytes(self.ptr, idx2 * T::SIZE, &mut buf_2);

        SSlice::_write_bytes(self.ptr, idx1 * T::SIZE, &buf_2);
        SSlice::_write_bytes(self.ptr, idx2 * T::SIZE, &buf_1);
    }

    pub fn extend_from(&mut self, mut other: Self) {
        if other.is_empty() {
            return;
        }

        if self.capacity() < self.len() + other.len() {
            self.cap = self.len() + other.len();

            let slice = unsafe { SSlice::from_ptr(self.ptr, Side::Start).unwrap_unchecked() };
            self.ptr = reallocate(slice, self.cap * T::SIZE).anyway().get_ptr();
        }

        let mut buf = vec![0u8; other.len() * T::SIZE];
        SSlice::_read_bytes(other.ptr, 0, &mut buf);
        SSlice::_write_bytes(self.ptr, self.len() * T::SIZE, &buf);

        self.len += other.len();
        other.len = 0;

        unsafe { other.stable_drop() };
    }

    #[inline]
    pub fn clear(&mut self) {
        for i in 0..self.len {
            let mut v = unsafe { self.get_copy(i).unwrap_unchecked() };
            v.remove_from_stable();
        }
        self.len = 0;
    }

    pub fn binary_search_by<FN>(&self, mut f: FN) -> Result<usize, usize>
    where
        FN: FnMut(T) -> Ordering,
    {
        if self.is_empty() {
            return Err(0);
        }

        let mut min = 0;
        let mut max = self.len;
        let mut mid = (max - min) / 2;

        let mut buf = T::_u8_arr_of_size();

        loop {
            SSlice::_read_bytes(self.ptr, mid * T::SIZE, &mut buf);
            let res = f(T::from_fixed_size_bytes(&buf));

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
}

impl<T> Default for SVec<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: StableAllocated> From<SVec<T>> for Vec<T>
where
    [u8; T::SIZE]: Sized,
{
    fn from(mut svec: SVec<T>) -> Self {
        let mut vec = Self::new();

        for i in svec.iter() {
            vec.push(i);
        }

        svec.len = 0;
        unsafe { svec.stable_drop() };

        vec
    }
}

impl<T: StableAllocated> From<Vec<T>> for SVec<T>
where
    [u8; T::SIZE]: Sized,
{
    fn from(mut vec: Vec<T>) -> Self {
        let mut svec = Self::new();

        for _ in 0..vec.len() {
            svec.push(vec.remove(0));
        }

        svec
    }
}

impl<T: StableAllocated + Debug> Debug for SVec<T>
where
    [u8; T::SIZE]: Sized,
{
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

impl<T> FixedSize for SVec<T> {
    const SIZE: usize = u64::SIZE + usize::SIZE + usize::SIZE;
}

impl<T: StableAllocated> AsFixedSizeBytes for SVec<T> {
    fn as_fixed_size_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        let (ptr_buf, rest_buf) = buf.split_at_mut(u64::SIZE);
        let (len_buf, cap_buf) = rest_buf.split_at_mut(usize::SIZE);

        ptr_buf.copy_from_slice(&self.ptr.as_fixed_size_bytes());
        len_buf.copy_from_slice(&self.len.as_fixed_size_bytes());
        cap_buf.copy_from_slice(&self.cap.as_fixed_size_bytes());

        buf
    }

    fn from_fixed_size_bytes(arr: &[u8; Self::SIZE]) -> Self {
        let (ptr_buf, rest_buf) = arr.split_at(u64::SIZE);
        let (len_buf, cap_buf) = rest_buf.split_at(usize::SIZE);

        let mut ptr_arr = [0u8; u64::SIZE];
        let mut len_arr = [0u8; usize::SIZE];
        let mut cap_arr = [0u8; usize::SIZE];

        ptr_arr[..].copy_from_slice(ptr_buf);
        len_arr[..].copy_from_slice(len_buf);
        cap_arr[..].copy_from_slice(cap_buf);

        Self {
            ptr: u64::from_fixed_size_bytes(&ptr_arr),
            len: usize::from_fixed_size_bytes(&len_arr),
            cap: usize::from_fixed_size_bytes(&cap_arr),
            _marker_t: PhantomData::default(),
        }
    }
}

impl<T: StableAllocated> StableAllocated for SVec<T>
where
    [(); T::SIZE]: Sized,
{
    #[inline]
    fn move_to_stable(&mut self) {}

    #[inline]
    fn remove_from_stable(&mut self) {}

    unsafe fn stable_drop(mut self) {
        if self.ptr != EMPTY_PTR {
            for elem in self.iter() {
                elem.stable_drop();
            }

            let slice = SSlice::from_ptr(self.ptr, Side::Start).unwrap();

            deallocate(slice);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::vec::{SVec, DEFAULT_CAPACITY};
    use crate::init_allocator;
    use crate::primitive::s_box::SBox;
    use crate::primitive::s_box_mut::SBoxMut;
    use crate::primitive::StableAllocated;
    use crate::utils::encoding::{AsFixedSizeBytes, FixedSize};
    use crate::utils::mem_context::stable;

    #[derive(Copy, Clone, Debug)]
    struct Test {
        a: usize,
        b: bool,
    }

    impl FixedSize for Test {
        const SIZE: usize = usize::SIZE + bool::SIZE;
    }

    impl AsFixedSizeBytes for Test {
        fn as_fixed_size_bytes(&self) -> [u8; Self::SIZE] {
            let mut whole = [0u8; Self::SIZE];
            let (part1, part2) = whole.split_at_mut(usize::SIZE);

            part1.copy_from_slice(&self.a.as_fixed_size_bytes());
            part2.copy_from_slice(&self.b.as_fixed_size_bytes());

            whole
        }

        fn from_fixed_size_bytes(arr: &[u8; Self::SIZE]) -> Self {
            let (part1, part2) = arr.split_at(usize::SIZE);
            let mut a_arr = [0u8; usize::SIZE];
            let mut b_arr = [0u8; bool::SIZE];

            a_arr[..].copy_from_slice(part1);
            b_arr[..].copy_from_slice(part2);

            Self {
                a: usize::from_fixed_size_bytes(&a_arr),
                b: bool::from_fixed_size_bytes(&b_arr),
            }
        }
    }

    impl StableAllocated for Test {
        fn move_to_stable(&mut self) {}

        fn remove_from_stable(&mut self) {}

        unsafe fn stable_drop(self) {}
    }

    #[test]
    fn create_destroy_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut stable_vec = SVec::new();
        assert_eq!(stable_vec.capacity(), DEFAULT_CAPACITY);
        assert_eq!(stable_vec.len(), 0);

        stable_vec.push(10);
        assert_eq!(stable_vec.capacity(), DEFAULT_CAPACITY);
        assert_eq!(stable_vec.len(), 1);

        unsafe { stable_vec.stable_drop() };
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

            stable_vec.push(it);
        }

        assert_eq!(stable_vec.len(), count, "Invalid len after push");

        for i in 0..count {
            let it = Test { a: i, b: false };

            stable_vec.replace(i, it);
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

            stable_vec.push(it);
        }

        unsafe { stable_vec.stable_drop() };
    }

    #[test]
    fn basic_flow_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut v = SVec::default();
        assert!(v.get_copy(100).is_none());

        v.push(10);
        v.push(20);

        assert_eq!(v.get_copy(0).unwrap(), 10);
        assert_eq!(v.get_copy(1).unwrap(), 20);
        assert_eq!(v.replace(0, 11), 10);

        unsafe { v.stable_drop() };
    }

    #[test]
    fn insert_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

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
            assert_eq!(array.get_copy(i).unwrap(), i);
        }

        let actual: Vec<_> = Vec::from(array);
        assert_eq!(actual, check);
    }

    #[test]
    fn binary_search_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

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
        stable::grow(1).unwrap();
        init_allocator(0);

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
        stable::grow(1).unwrap();
        init_allocator(0);

        let vec = SVec::<u32>::new_with_capacity(10);
        let buf = vec.as_fixed_size_bytes();
        let vec1 = SVec::<u32>::from_fixed_size_bytes(&buf);

        assert_eq!(vec.ptr, vec1.ptr);
        assert_eq!(vec.len, vec1.len);
        assert_eq!(vec.cap, vec1.cap);

        let ptr = vec.ptr;
        let len = vec.len;
        let cap = vec.cap;

        let buf = vec.as_fixed_size_bytes();
        let vec1 = SVec::<u32>::from_fixed_size_bytes(&buf);

        assert_eq!(ptr, vec1.ptr);
        assert_eq!(len, vec1.len);
        assert_eq!(cap, vec1.cap);
    }

    #[test]
    fn iter_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut vec = SVec::new();
        for i in 0..100 {
            vec.push(i);
        }

        let mut c = 0;
        for (idx, i) in vec.iter().enumerate() {
            c += 1;

            assert_eq!(idx as i32, i);
        }

        assert_eq!(c, 100);
    }

    #[test]
    fn sboxes_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut vec = SVec::new();

        for i in 0..100 {
            vec.push(SBox::new(10));
        }

        unsafe { vec.stable_drop() };

        let mut vec = SVec::new();

        for i in 0..100 {
            vec.push(SBoxMut::new(10));
        }

        unsafe { vec.stable_drop() };
    }
}
