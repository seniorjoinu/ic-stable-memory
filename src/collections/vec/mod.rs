use crate::mem::allocator::EMPTY_PTR;
use crate::mem::s_slice::{SSlice, Side};
use crate::mem::Anyway;
use crate::primitive::StackAllocated;
use crate::utils::phantom_data::SPhantomData;
use crate::utils::u8_smallvec;
use crate::{allocate, deallocate, reallocate};
use speedy::{Context, LittleEndian, Readable, Reader, Writable, Writer};
use std::cmp::Ordering;
use std::fmt::{Debug, Formatter};
use std::mem::size_of;

const DEFAULT_CAPACITY: usize = 4;

pub struct SVec<T, A> {
    pub(crate) ptr: u64,
    pub(crate) len: usize,
    pub(crate) cap: usize,
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
        }
    }

    #[inline]
    fn to_offset_or_size(idx: usize, item_size: usize) -> usize {
        idx * item_size
    }
}

impl<A: AsMut<[u8]> + AsRef<[u8]>, T: StackAllocated<T, A>> SVec<T, A> {
    pub fn push(&mut self, element: T) {
        self.maybe_reallocate(T::size_of_u8_array());

        let offset = Self::to_offset_or_size(self.len, T::size_of_u8_array());
        let elem_bytes = T::to_u8_fixed_size_array(element);

        SSlice::_write_bytes(self.ptr, offset, elem_bytes.as_ref());

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
        if idx >= self.len() {
            return None;
        }

        let offset = Self::to_offset_or_size(idx, T::size_of_u8_array());

        let mut elem_bytes = T::fixed_size_u8_array();
        SSlice::_read_bytes(self.ptr, offset, elem_bytes.as_mut());

        Some(T::from_u8_fixed_size_array(elem_bytes))
    }

    pub fn replace(&mut self, idx: usize, element: T) -> T {
        assert!(idx < self.len(), "Out of bounds");

        let offset = Self::to_offset_or_size(idx, T::size_of_u8_array());

        let mut old_elem_bytes = T::fixed_size_u8_array();
        SSlice::_read_bytes(self.ptr, offset, old_elem_bytes.as_mut());

        let new_elem_bytes = T::to_u8_fixed_size_array(element);
        SSlice::_write_bytes(self.ptr, offset, new_elem_bytes.as_ref());

        T::from_u8_fixed_size_array(old_elem_bytes)
    }

    pub fn insert(&mut self, idx: usize, element: T) {
        assert!(idx <= self.len, "out of bounds");

        if idx == self.len {
            self.push(element);
            return;
        }

        self.maybe_reallocate(T::size_of_u8_array());

        let size = Self::to_offset_or_size(self.len - idx, T::size_of_u8_array());
        let offset = Self::to_offset_or_size(idx, T::size_of_u8_array());

        let mut buf = u8_smallvec(size);

        SSlice::_read_bytes(self.ptr, offset, &mut buf);
        SSlice::_write_bytes(self.ptr, offset + T::size_of_u8_array(), &buf);

        let elem_bytes = T::to_u8_fixed_size_array(element);
        SSlice::_write_bytes(self.ptr, offset, elem_bytes.as_ref());

        self.len += 1;
    }

    pub fn remove(&mut self, idx: usize) -> T {
        assert!(idx < self.len, "out of bounds");

        if idx == self.len - 1 {
            return unsafe { self.pop().unwrap_unchecked() };
        }

        let size = Self::to_offset_or_size(self.len - (idx + 1), T::size_of_u8_array());
        let mut buf = u8_smallvec(size);
        let offset = Self::to_offset_or_size(idx + 1, T::size_of_u8_array());
        SSlice::_read_bytes(self.ptr, offset, &mut buf);

        let mut elem_bytes = T::fixed_size_u8_array();
        SSlice::_read_bytes(
            self.ptr,
            offset - T::size_of_u8_array(),
            elem_bytes.as_mut(),
        );

        SSlice::_write_bytes(self.ptr, offset - T::size_of_u8_array(), &buf);

        self.len -= 1;

        T::from_u8_fixed_size_array(elem_bytes)
    }

    // TODO: make more efficient by simply copying bits
    pub fn extend_from(&mut self, other: &Self) {
        for i in 0..other.len() {
            self.push(other.get_copy(i).unwrap());
        }
    }

    pub fn binary_search_by<FN>(&self, f: FN) -> Result<usize, usize>
    where
        FN: Fn(T) -> Ordering,
    {
        if self.is_empty() {
            return Err(0);
        }

        let mut min = 0;
        let mut max = self.len;
        let mut mid = (max - min) / 2;

        loop {
            match f(unsafe { self.get_copy(mid).unwrap_unchecked() }) {
                Ordering::Equal => return Ok(mid),
                // actually LESS
                Ordering::Greater => {
                    max = mid;
                    let new_mid = (max - min) / 2 + min;

                    if new_mid == mid {
                        return Err(mid);
                    }

                    mid = new_mid;
                    continue;
                }
                // actually GREATER
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
}

impl<A: AsRef<[u8]> + AsMut<[u8]>, T: StackAllocated<T, A>> SVec<T, A> {
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

impl<A: AsMut<[u8]> + AsRef<[u8]>, T: StackAllocated<T, A>> Default for SVec<T, A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A: AsMut<[u8]> + AsRef<[u8]>, T: StackAllocated<T, A>> From<&SVec<T, A>> for Vec<T> {
    fn from(svec: &SVec<T, A>) -> Self {
        let mut vec = Self::new();

        for i in 0..svec.len() {
            vec.push(unsafe { svec.get_copy(i).unwrap_unchecked() });
        }

        vec
    }
}

impl<A: AsMut<[u8]> + AsRef<[u8]>, T: StackAllocated<T, A>> From<Vec<T>> for SVec<T, A> {
    fn from(mut vec: Vec<T>) -> Self {
        let mut svec = Self::new();

        for _ in 0..vec.len() {
            svec.push(unsafe { vec.remove(0) });
        }

        svec
    }
}

impl<A: AsMut<[u8]> + AsRef<[u8]>, T: StackAllocated<T, A> + Debug> Debug for SVec<T, A> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("[")?;
        for i in 0..self.len {
            let elem = unsafe { self.get_copy(i).unwrap_unchecked() };
            elem.fmt(f)?;

            if i < self.len - 1 {
                f.write_str(", ")?;
            }
        }
        f.write_str("]")
    }
}

impl<'a, A, T> Readable<'a, LittleEndian> for SVec<T, A> {
    fn read_from<R: Reader<'a, LittleEndian>>(
        reader: &mut R,
    ) -> Result<Self, <speedy::LittleEndian as Context>::Error> {
        let ptr = reader.read_u64()?;
        let len = reader.read_u32()? as usize;
        let cap = reader.read_u32()? as usize;

        Ok(Self {
            ptr,
            len,
            cap,
            _marker_t: SPhantomData::new(),
            _marker_a: SPhantomData::new(),
        })
    }
}

impl<A, T> Writable<LittleEndian> for SVec<T, A> {
    fn write_to<W: ?Sized + Writer<LittleEndian>>(
        &self,
        writer: &mut W,
    ) -> Result<(), <speedy::LittleEndian as Context>::Error> {
        writer.write_u64(self.ptr)?;
        writer.write_u32(self.len as u32)?;
        writer.write_u32(self.cap as u32)
    }
}

impl<A, T> StackAllocated<SVec<T, A>, [u8; size_of::<SVec<T, A>>()]> for SVec<T, A> {
    fn size_of_u8_array() -> usize {
        size_of::<Self>()
    }

    fn fixed_size_u8_array() -> [u8; size_of::<Self>()] {
        [0u8; size_of::<Self>()]
    }

    fn to_u8_fixed_size_array(it: Self) -> [u8; size_of::<Self>()] {
        unsafe { std::mem::transmute(it) }
    }

    fn from_u8_fixed_size_array(arr: [u8; size_of::<Self>()]) -> Self {
        unsafe { std::mem::transmute(arr) }
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::vec::SVec;
    use crate::init_allocator;
    use crate::utils::mem_context::stable;
    use std::mem::size_of;

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

        stable_vec.push(10);
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

        unsafe { stable_vec.drop() };
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

        unsafe { v.drop() };
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

        let actual: Vec<_> = Vec::from(&array);
        assert_eq!(actual, check);

        unsafe { array.drop() };
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

        let mut array = SVec::<i32, [u8; size_of::<i32>()]>::default();
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

        let actual = Vec::from(&array);
        assert_eq!(actual, check);

        unsafe { array.drop() };
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
}
