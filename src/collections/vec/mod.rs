use crate::collections::vec::iter::SVecIter;
use crate::encoding::{AsFixedSizeBytes, Buffer};
use crate::mem::allocator::EMPTY_PTR;
use crate::mem::s_slice::SSlice;
use crate::mem::StablePtr;
use crate::primitive::s_ref::SRef;
use crate::primitive::s_ref_mut::SRefMut;
use crate::primitive::StableType;
use crate::{allocate, deallocate, reallocate, OutOfMemory};
use std::cmp::Ordering;
use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;

#[doc(hidden)]
pub mod iter;

const DEFAULT_CAPACITY: usize = 4;

/// Stable analog of [Vec]
///
/// May reallocate on inserts. In this case will copy the underlying data to a new location.
///
/// This is a "finite" data structure, it can only holp up to [u32::MAX] / `T::SIZE` elements. Putting
/// more elements inside will panic.
///
/// `T` has to implement both [StableType] and [AsFixedSizeBytes]. [SVec] itself implements these
/// traits and can be nested inside other stable data structures.
///
/// When [SVec] is stable-dropped, its elements are also stable-dropped but in reverse order.
pub struct SVec<T: StableType + AsFixedSizeBytes> {
    ptr: u64,
    len: usize,
    cap: usize,
    stable_drop_flag: bool,
    _marker_t: PhantomData<T>,
}

impl<T: StableType + AsFixedSizeBytes> SVec<T> {
    /// Creates a [SVec] of capacity equal to 4 elements.
    ///
    /// Does not allocate any heap or stable memory.
    ///
    /// # Example
    /// ```rust
    /// // won't allocate until you insert something in it
    /// # use ic_stable_memory::collections::SVec;
    /// # use ic_stable_memory::stable_memory_init;
    /// # unsafe { ic_stable_memory::mem::clear(); }
    /// # stable_memory_init();
    /// let mut numbers = SVec::<u64>::new();
    /// ```
    #[inline]
    pub fn new() -> Self {
        Self {
            len: 0,
            cap: DEFAULT_CAPACITY,
            ptr: EMPTY_PTR,
            stable_drop_flag: true,
            _marker_t: PhantomData::default(),
        }
    }

    /// Creates a [SVec] of requested capacity.
    ///
    /// Does allocate stable memory, returning [OutOfMemory] if there is not enough of it.
    /// If this function returns [Ok], you are guaranteed to have enough stable memory to store at
    /// least `capacity` elements in it.
    ///
    /// # Example
    /// ```rust
    /// # use ic_stable_memory::collections::SVec;
    /// # use ic_stable_memory::stable_memory_init;
    /// # unsafe { ic_stable_memory::mem::clear(); }
    /// # stable_memory_init();
    /// let mut at_least_10_numbers = SVec::<u64>::new_with_capacity(10)
    ///     .expect("Out of memory");
    /// ```
    #[inline]
    pub fn new_with_capacity(capacity: usize) -> Result<Self, OutOfMemory> {
        assert!(capacity <= Self::max_capacity());

        Ok(Self {
            len: 0,
            cap: capacity,
            ptr: unsafe { allocate((capacity * T::SIZE) as u64)?.as_ptr() },
            stable_drop_flag: true,
            _marker_t: PhantomData::default(),
        })
    }

    /// Returns the capacity of this [SVec]
    #[inline]
    pub fn capacity(&self) -> usize {
        self.cap
    }

    /// Returns the length of this [SVec]
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns [true] if the length of this [SVec] is `0`
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the maximum possible capacity of this [SVec]
    #[inline]
    pub const fn max_capacity() -> usize {
        u32::MAX as usize / T::SIZE
    }

    /// Inserts a new element at the end of this [SVec]
    ///
    /// Will try to reallocate if `capacity == length`. If the canister is out of stable memory,
    /// will return [Err] with the element that was about to get inserted.
    #[inline]
    pub fn push(&mut self, mut element: T) -> Result<(), T> {
        if self.maybe_reallocate().is_ok() {
            let elem_ptr = SSlice::_offset(self.ptr, (self.len * T::SIZE) as u64);
            unsafe { crate::mem::write_fixed(elem_ptr, &mut element) };

            self.len += 1;

            Ok(())
        } else {
            Err(element)
        }
    }

    /// Removes the last element of the [SVec]
    ///
    /// If the [SVec] is empty, returns [None].
    #[inline]
    pub fn pop(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }

        let elem_ptr = self.get_element_ptr(self.len - 1)?;
        self.len -= 1;

        Some(unsafe { crate::mem::read_fixed_for_move(elem_ptr) })
    }

    /// Returns a [SRef] pointing to the element at requested index
    ///
    /// See also [SVec::get_mut].
    ///
    /// If out of bounds, returns [None]
    ///
    /// # Example
    /// ```rust
    /// # use ic_stable_memory::collections::SVec;
    /// # use ic_stable_memory::stable_memory_init;
    /// # unsafe { ic_stable_memory::mem::clear(); }
    /// # stable_memory_init();
    /// let mut vec = SVec::<u64>::new();
    /// vec.push(20).expect("Out of memory");
    ///
    /// let elem = vec.get(0).unwrap();
    ///
    /// assert_eq!(*elem, 20);
    /// ```
    #[inline]
    pub fn get(&self, idx: usize) -> Option<SRef<'_, T>> {
        let ptr = self.get_element_ptr(idx)?;

        unsafe { Some(SRef::new(ptr)) }
    }

    /// Returns [SRefMut] pointing to the element at requested index
    ///
    /// See also [SVec::get].
    ///
    /// If out of bounds, returns [None]
    ///
    /// # Example
    /// ```rust
    /// # use ic_stable_memory::collections::SVec;
    /// # use ic_stable_memory::stable_memory_init;
    /// # unsafe { ic_stable_memory::mem::clear(); }
    /// # stable_memory_init();
    /// let mut vec = SVec::<u64>::new();
    /// vec.push(20).expect("Out of memory");
    ///
    /// let mut elem = vec.get_mut(0).unwrap();
    /// *elem = 100;
    /// ```
    #[inline]
    pub fn get_mut(&mut self, idx: usize) -> Option<SRefMut<'_, T>> {
        let ptr = self.get_element_ptr(idx)?;

        unsafe { Some(SRefMut::new(ptr)) }
    }

    /// Replaces an element at requested index with a provided value
    ///
    /// # Panics
    /// Panics if out of bounds.
    ///
    /// # Example
    /// ```rust
    /// # use ic_stable_memory::collections::SVec;
    /// # use ic_stable_memory::stable_memory_init;
    /// # unsafe { ic_stable_memory::mem::clear(); }
    /// # stable_memory_init();
    /// let mut vec = SVec::<u64>::new();
    /// vec.push(10).expect("Out of memory");
    ///
    /// let prev = vec.replace(0, 20);
    ///
    /// assert_eq!(prev, 10);
    /// assert_eq!(*vec.get(0).unwrap(), 20);
    /// ```
    pub fn replace(&mut self, idx: usize, mut element: T) -> T {
        assert!(idx < self.len(), "Out of bounds");

        let elem_ptr = SSlice::_offset(self.ptr, (idx * T::SIZE) as u64);

        let prev_element = unsafe { crate::mem::read_fixed_for_move(elem_ptr) };
        unsafe { crate::mem::write_fixed(elem_ptr, &mut element) };

        prev_element
    }

    /// Inserts a new element at the requested index, forward-shifting all elements after it
    ///
    /// Will try to reallocate, if `capacity == length`. If the canister is out of stable memory,
    /// will return [Err] with the element that was about to get inserted.
    ///
    /// # Panics
    /// Panics if out of bounds.
    pub fn insert(&mut self, idx: usize, mut element: T) -> Result<(), T> {
        if idx == self.len {
            return self.push(element);
        }

        assert!(idx < self.len, "out of bounds");

        if self.maybe_reallocate().is_ok() {
            let elem_ptr = SSlice::_offset(self.ptr, (idx * T::SIZE) as u64);

            // moving elements after idx one slot to the right
            let mut buf = vec![0u8; (self.len - idx) * T::SIZE];
            unsafe { crate::mem::read_bytes(elem_ptr, &mut buf) };
            unsafe { crate::mem::write_bytes(elem_ptr + T::SIZE as u64, &buf) };

            // writing the element
            unsafe { crate::mem::write_fixed(elem_ptr, &mut element) };

            self.len += 1;

            Ok(())
        } else {
            Err(element)
        }
    }

    /// Removes element at the requested index, back-shifting all elements after it
    ///
    /// # Panics
    /// Panics if out of bounds.
    pub fn remove(&mut self, idx: usize) -> T {
        assert!(idx < self.len, "out of bounds");

        if idx == self.len - 1 {
            return unsafe { self.pop().unwrap_unchecked() };
        }

        let elem_ptr = SSlice::_offset(self.ptr, (idx * T::SIZE) as u64);
        let elem = unsafe { crate::mem::read_fixed_for_move(elem_ptr) };

        let mut buf = vec![0u8; (self.len - idx - 1) * T::SIZE];
        unsafe { crate::mem::read_bytes(elem_ptr + T::SIZE as u64, &mut buf) };
        unsafe { crate::mem::write_bytes(elem_ptr, &buf) };

        self.len -= 1;

        elem
    }

    /// Swaps elements at requested indices with each other
    ///
    /// # Panics
    /// Panics if `idx1` == `idx2` or if any of indices are out of bounds.
    pub fn swap(&mut self, idx1: usize, idx2: usize) {
        assert!(
            idx1 < self.len() && idx2 < self.len() && idx1 != idx2,
            "invalid idx"
        );

        let ptr1 = SSlice::_offset(self.ptr, (idx1 * T::SIZE) as u64);
        let ptr2 = SSlice::_offset(self.ptr, (idx2 * T::SIZE) as u64);

        let mut buf_1 = T::Buf::new(T::SIZE);
        let mut buf_2 = T::Buf::new(T::SIZE);

        unsafe { crate::mem::read_bytes(ptr1, buf_1._deref_mut()) };
        unsafe { crate::mem::read_bytes(ptr2, buf_2._deref_mut()) };

        unsafe { crate::mem::write_bytes(ptr2, buf_1._deref()) };
        unsafe { crate::mem::write_bytes(ptr1, buf_2._deref()) };
    }

    /// Clears the [SVec] from elements
    ///
    /// Does not reallocate or shrink the underlying memory block.
    #[inline]
    pub fn clear(&mut self) {
        while self.pop().is_some() {}
    }

    /// Performs binary search on a sorted [SVec], using the provided lambda
    ///
    /// Works the same way as in [Vec].
    ///
    /// # Example
    /// ```rust
    /// # use ic_stable_memory::collections::SVec;
    /// # use ic_stable_memory::stable_memory_init;
    /// # unsafe { ic_stable_memory::mem::clear(); }
    /// # stable_memory_init();
    /// let mut vec = SVec::new_with_capacity(100).expect("Out of memory");
    ///
    /// for i in 0..100 {
    ///     vec.push(i);
    /// }
    ///
    /// let idx = vec.binary_search_by(|it| it.cmp(&10)).unwrap();
    ///
    /// assert_eq!(idx, 10);
    /// ```
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
            let elem_ptr = SSlice::_offset(self.ptr, (mid * T::SIZE) as u64);
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

    /// Returns an immutable iterator over this collection
    ///
    /// # Example
    /// ```rust
    /// # use ic_stable_memory::collections::SVec;
    /// # use ic_stable_memory::stable_memory_init;
    /// # unsafe { ic_stable_memory::mem::clear(); }
    /// # stable_memory_init();
    /// let mut vec = SVec::new_with_capacity(100).expect("Out of memory");
    ///
    /// for i in 0..100 {
    ///     vec.push(i);
    /// }
    ///
    /// for elem in vec.iter() {
    ///     println!("{}", *elem); // will print '0, 1, 2, 3, 4, ...'
    /// }
    /// ```
    #[inline]
    pub fn iter(&self) -> SVecIter<T> {
        SVecIter::new(self)
    }

    /// Prints byte representation of this collection
    ///
    /// Useful for tests
    pub fn debug_print(&self) {
        print!("SVec[");
        for i in 0..self.len {
            let mut b = T::Buf::new(T::SIZE);
            unsafe {
                crate::mem::read_bytes(
                    SSlice::_offset(self.ptr, (i * T::SIZE) as u64),
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

    fn maybe_reallocate(&mut self) -> Result<(), OutOfMemory> {
        if self.ptr == EMPTY_PTR {
            self.ptr = unsafe { allocate((self.capacity() * T::SIZE) as u64)?.as_ptr() };
            return Ok(());
        }

        if self.len() == self.capacity() {
            self.cap = self.cap.checked_mul(2).unwrap();
            assert!(self.cap <= Self::max_capacity());

            let slice = unsafe { SSlice::from_ptr(self.ptr).unwrap() };

            self.ptr = unsafe { reallocate(slice, (self.cap * T::SIZE) as u64)?.as_ptr() };
        }

        Ok(())
    }

    fn grow(&mut self, new_capacity: usize) -> Result<bool, OutOfMemory> {
        if new_capacity <= self.cap {
            return Ok(false);
        }

        self.cap = new_capacity;
        assert!(self.cap <= Self::max_capacity());

        let slice = unsafe { SSlice::from_ptr(self.ptr).unwrap() };

        self.ptr = unsafe { reallocate(slice, (self.cap * T::SIZE) as u64)?.as_ptr() };

        Ok(true)
    }

    pub(crate) fn get_element_ptr(&self, idx: usize) -> Option<StablePtr> {
        if idx < self.len() {
            Some(SSlice::_offset(self.ptr, (idx * T::SIZE) as u64))
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
    const SIZE: usize = u64::SIZE + usize::SIZE * 2;
    type Buf = [u8; u64::SIZE + usize::SIZE * 2];

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
            stable_drop_flag: false,
            _marker_t: PhantomData::default(),
        }
    }
}

impl<T: StableType + AsFixedSizeBytes> StableType for SVec<T> {
    #[inline]
    unsafe fn stable_drop_flag_off(&mut self) {
        self.stable_drop_flag = false;
    }

    #[inline]
    unsafe fn stable_drop_flag_on(&mut self) {
        self.stable_drop_flag = true;
    }

    #[inline]
    fn should_stable_drop(&self) -> bool {
        self.stable_drop_flag
    }

    unsafe fn stable_drop(&mut self) {
        if self.ptr != EMPTY_PTR {
            self.clear();

            let slice = SSlice::from_ptr(self.ptr).unwrap();

            deallocate(slice);
        }
    }
}

impl<T: StableType + AsFixedSizeBytes> Drop for SVec<T> {
    fn drop(&mut self) {
        if self.should_stable_drop() {
            unsafe {
                self.stable_drop();
            }
        }
    }
}

impl SVec<u8> {
    pub fn from_std_blob<T: AsRef<[u8]>>(blob: T) -> Result<Self, OutOfMemory> {
        let b = blob.as_ref();
        let mut svec = SVec::<u8>::new_with_capacity(b.len())?;
        svec.len = b.len();

        let ptr = SSlice::_offset(svec.ptr, 0);
        unsafe { crate::mem::write_bytes(ptr, b) };

        Ok(svec)
    }

    pub fn as_std_blob(&self) -> Vec<u8> {
        let mut blob = vec![0; self.len()];

        let ptr = SSlice::_offset(self.ptr, 0);
        unsafe { crate::mem::read_bytes(ptr, &mut blob) };

        return blob;
    }

    pub fn extend_from_slice(&mut self, slice: &[u8]) {
        self.grow(new_capacity)
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::vec::{SVec, DEFAULT_CAPACITY};
    use crate::encoding::{AsFixedSizeBytes, Buffer};
    use crate::primitive::s_box::SBox;
    use crate::primitive::StableType;
    use crate::utils::mem_context::stable;
    use crate::utils::test::generate_random_string;
    use crate::utils::DebuglessUnwrap;
    use crate::{
        _debug_print_allocator, _debug_validate_allocator, deinit_allocator, get_allocated_size,
        init_allocator, retrieve_custom_data, stable_memory_init, stable_memory_post_upgrade,
        stable_memory_pre_upgrade, store_custom_data,
    };
    use rand::rngs::ThreadRng;
    use rand::seq::SliceRandom;
    use rand::{thread_rng, Rng};
    use std::fmt::Debug;
    use std::ops::Deref;

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

        {
            let mut stable_vec = SVec::new();
            assert_eq!(stable_vec.capacity(), DEFAULT_CAPACITY);
            assert_eq!(stable_vec.len(), 0);

            stable_vec.push(10).unwrap();
            assert_eq!(stable_vec.capacity(), DEFAULT_CAPACITY);
            assert_eq!(stable_vec.len(), 1);
        }

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
    }

    #[test]
    fn push_pop_work_fine() {
        stable::clear();
        stable_memory_init();

        {
            let mut stable_vec = SVec::new();
            let count = 10usize;

            for i in 0..count {
                let it = Test { a: i, b: true };

                stable_vec.push(it).unwrap();
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

                stable_vec.push(it).unwrap();
            }
        }

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
    }

    #[test]
    fn basic_flow_works_fine() {
        stable::clear();
        stable_memory_init();

        {
            let mut v = SVec::default();
            assert!(v.get(100).is_none());

            v.push(10).unwrap();
            v.push(20).unwrap();

            assert_eq!(*v.get(0).unwrap(), 10);
            assert_eq!(*v.get(1).unwrap(), 20);
            assert_eq!(v.replace(0, 11), 10);
        }

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
    }

    #[test]
    fn insert_works_fine() {
        stable::clear();
        stable_memory_init();

        {
            let mut array = SVec::default();
            let mut check = Vec::default();

            for i in 0..30 {
                array.insert(0, 29 - i).unwrap();
                check.insert(0, 29 - i);
            }

            for i in 60..100 {
                array.insert(array.len(), i).unwrap();
                check.insert(check.len(), i);
            }

            for i in 30..60 {
                array.insert(30 + (i - 30), i).unwrap();
                check.insert(30 + (i - 30), i);
            }

            for i in 0..100 {
                assert_eq!(*array.get(i).unwrap(), i);
            }

            let mut actual = Vec::new();
            while let Some(elem) = array.pop() {
                actual.insert(0, elem);
            }

            assert_eq!(actual, check);
        }

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
    }

    #[test]
    fn binary_search_work_fine() {
        stable::clear();
        stable_memory_init();

        {
            // 0..100 randomly shuffled
            let initial = vec![
                3, 24, 46, 92, 2, 21, 34, 95, 82, 22, 88, 32, 59, 13, 73, 51, 12, 83, 28, 17, 9,
                23, 5, 63, 62, 38, 20, 40, 25, 98, 30, 43, 57, 86, 42, 4, 99, 33, 11, 74, 96, 94,
                47, 31, 37, 71, 80, 70, 14, 67, 93, 56, 27, 39, 58, 41, 29, 84, 8, 0, 45, 54, 7,
                26, 97, 6, 81, 65, 79, 10, 91, 68, 36, 60, 76, 75, 15, 87, 49, 35, 78, 64, 69, 52,
                50, 61, 48, 53, 44, 19, 55, 72, 90, 77, 89, 16, 85, 66, 18, 1,
            ];

            let mut array = SVec::<i32>::default();
            let mut check = Vec::<i32>::new();

            for i in 0..100 {
                match check.binary_search_by(|it| it.cmp(&initial[i])) {
                    Err(idx) => check.insert(idx, initial[i]),
                    _ => unreachable!(),
                }

                match array.binary_search_by(|it| it.cmp(&initial[i])) {
                    Err(idx) => array.insert(idx, initial[i]).unwrap(),
                    _ => unreachable!(),
                }

                _debug_print_allocator();
            }

            let mut actual = Vec::new();
            while let Some(elem) = array.pop() {
                actual.insert(0, elem);
            }

            assert_eq!(actual, check);
        }

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
    }

    #[test]
    fn remove_works_fine() {
        stable::clear();
        stable_memory_init();

        {
            let initial = vec![
                3, 24, 46, 92, 2, 21, 34, 95, 82, 22, 88, 32, 59, 13, 73, 51, 12, 83, 28, 17, 9,
                23, 5, 63, 62, 38, 20, 40, 25, 98, 30, 43, 57, 86, 42, 4, 99, 33, 11, 74, 96, 94,
                47, 31, 37, 71, 80, 70, 14, 67, 93, 56, 27, 39, 58, 41, 29, 84, 8, 0, 45, 54, 7,
                26, 97, 6, 81, 65, 79, 10, 91, 68, 36, 60, 76, 75, 15, 87, 49, 35, 78, 64, 69, 52,
                50, 61, 48, 53, 44, 19, 55, 72, 90, 77, 89, 16, 85, 66, 18, 1,
            ];

            let mut array = SVec::new();
            for i in 0..initial.len() {
                array.push(initial[i]);
            }

            for i in 0..initial.len() {
                assert_eq!(array.remove(0), initial[i]);
            }
        }

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
    }

    #[test]
    fn serialization_works_fine() {
        stable::clear();
        stable_memory_init();

        {
            let mut vec = SVec::<u32>::new_with_capacity(10).debugless_unwrap();
            vec.push(1);
            vec.push(2);
            vec.push(3);

            let mut buf = <SVec<u32> as AsFixedSizeBytes>::Buf::new(SVec::<u32>::SIZE);
            vec.as_fixed_size_bytes(buf._deref_mut());
            let mut vec1 = SVec::<u32>::from_fixed_size_bytes(buf._deref());
            unsafe { vec1.stable_drop_flag_off() };

            assert_eq!(vec.ptr, vec1.ptr);
            assert_eq!(vec.len, vec1.len);
            assert_eq!(vec.cap, vec1.cap);

            let ptr = vec.ptr;
            let len = vec.len;
            let cap = vec.cap;

            let mut buf = <SVec<u32> as AsFixedSizeBytes>::Buf::new(SVec::<u32>::SIZE);
            vec.as_fixed_size_bytes(buf._deref_mut());

            let mut vec1 = SVec::<u32>::from_fixed_size_bytes(buf._deref());
            unsafe { vec1.stable_drop_flag_off() };

            assert_eq!(ptr, vec1.ptr);
            assert_eq!(len, vec1.len);
            assert_eq!(cap, vec1.cap);
        }

        _debug_print_allocator();
        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
    }

    #[test]
    fn iter_works_fine() {
        stable::clear();
        stable_memory_init();

        {
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

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
    }

    #[test]
    fn random_works_fine() {
        stable::clear();
        stable_memory_init();

        {
            let mut svec = SVec::new();

            let mut rng = thread_rng();
            let iterations = 10_000;

            let mut example = Vec::new();
            for i in 0..iterations {
                example.push(i);
            }
            example.shuffle(&mut rng);

            for i in 0..iterations {
                svec.push(example[i]);
            }

            for i in 0..iterations {
                svec.insert(rng.gen_range(0..svec.len()), example[i]);
            }

            for _ in 0..iterations {
                svec.pop().unwrap();
            }

            for i in 0..iterations {
                if svec.len() == 1 {
                    svec.remove(0);
                } else {
                    let range = rng.gen_range(0..(svec.len() - 1));
                    svec.remove(range);
                }
            }

            assert!(svec.is_empty());
        }

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
    }

    #[test]
    fn sboxes_work_fine() {
        stable::clear();
        stable_memory_init();

        {
            let mut vec = SVec::new();

            for _ in 0..100 {
                let b = SBox::new(10).unwrap();

                vec.push(b);
            }

            vec.debug_print();
        }

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
    }

    #[derive(Debug)]
    enum Action {
        Push(usize),
        Pop(usize),
        Insert(usize, usize),
        Remove(usize, usize),
        Swap(usize, usize, usize),
        Replace(usize, usize),
        CanisterUpgrade,
        GetMut(usize, usize),
        Clear(usize),
    }

    struct Fuzzer {
        vec: Option<SVec<SVec<SBox<String>>>>,
        example: Vec<Vec<String>>,
        rng: ThreadRng,
        log: Vec<Action>,
    }

    impl Fuzzer {
        fn new() -> Self {
            let mut svec = SVec::default();
            let mut vec = Vec::default();

            for i in 0..10 {
                svec.push(SVec::default());
                vec.push(Vec::default());
            }

            Self {
                vec: Some(svec),
                example: vec,
                rng: thread_rng(),
                log: Vec::default(),
            }
        }

        fn vec(&mut self) -> &mut SVec<SVec<SBox<String>>> {
            self.vec.as_mut().unwrap()
        }

        fn next(&mut self) {
            let action = self.rng.gen_range(0..1100);

            match action {
                // PUSH ~25%
                0..=250 => {
                    let str = generate_random_string(&mut self.rng);

                    if let Ok(data) = SBox::new(str.clone()) {
                        let outer_idx = self.rng.gen_range(0..10);

                        if self.vec().get_mut(outer_idx).unwrap().push(data).is_err() {
                            return;
                        };

                        self.example.get_mut(outer_idx).unwrap().push(str.clone());

                        self.log.push(Action::Push(outer_idx));
                    }
                }
                // INSERT ~25%
                251..=500 => {
                    let str = generate_random_string(&mut self.rng);

                    if let Ok(data) = SBox::new(str.clone()) {
                        let outer_idx = self.rng.gen_range(0..10);

                        let len = self.vec().get(outer_idx).unwrap().len();
                        let idx = if len == 0 {
                            0
                        } else {
                            self.rng.gen_range(0..len + 1)
                        };

                        if self
                            .vec()
                            .get_mut(outer_idx)
                            .unwrap()
                            .insert(idx, data)
                            .is_err()
                        {
                            return;
                        }

                        self.example
                            .get_mut(outer_idx)
                            .unwrap()
                            .insert(idx, str.clone());

                        self.log.push(Action::Insert(outer_idx, idx));
                    }
                }
                // POP ~10%
                501..=600 => {
                    let outer_idx = self.rng.gen_range(0..10);

                    self.vec().get_mut(outer_idx).unwrap().pop();
                    self.example.get_mut(outer_idx).unwrap().pop();

                    self.log.push(Action::Pop(outer_idx));
                }
                // REMOVE ~10%
                601..=700 => {
                    let outer_idx = self.rng.gen_range(0..10);

                    let len = self.vec().get(outer_idx).unwrap().len();

                    if len == 0 {
                        return self.next();
                    }

                    let idx = if len == 1 {
                        0
                    } else {
                        self.rng.gen_range(0..len)
                    };

                    self.vec().get_mut(outer_idx).unwrap().remove(idx);
                    self.example.get_mut(outer_idx).unwrap().remove(idx);

                    self.log.push(Action::Remove(outer_idx, idx));
                }
                // SWAP ~10%
                701..=800 => {
                    let outer_idx = self.rng.gen_range(0..10);

                    let len = self.vec().get(outer_idx).unwrap().len();

                    if len < 2 {
                        return self.next();
                    }

                    let mut idx1 = self.rng.gen_range(0..len);
                    let mut idx2 = self.rng.gen_range(0..len);

                    if idx1 == idx2 {
                        if idx2 < len - 1 {
                            idx2 += 1;
                        } else {
                            idx2 -= 1;
                        }
                    }

                    self.vec().get_mut(outer_idx).unwrap().swap(idx1, idx2);
                    self.example.get_mut(outer_idx).unwrap().swap(idx1, idx2);

                    self.log.push(Action::Swap(outer_idx, idx1, idx2));
                }
                // REPLACE ~10%
                801..=900 => {
                    let outer_idx = self.rng.gen_range(0..10);

                    let len = self.vec().get(outer_idx).unwrap().len();

                    if len == 0 {
                        return self.next();
                    }

                    let idx = self.rng.gen_range(0..len);
                    let str = generate_random_string(&mut self.rng);

                    if let Ok(data) = SBox::new(str.clone()) {
                        self.vec().get_mut(outer_idx).unwrap().replace(idx, data);

                        std::mem::replace(
                            self.example
                                .get_mut(outer_idx)
                                .unwrap()
                                .get_mut(idx)
                                .unwrap(),
                            str.clone(),
                        );

                        self.log.push(Action::Replace(outer_idx, idx));
                    }
                }
                // GET MUT ~10%
                901..=1000 => {
                    let outer_idx = self.rng.gen_range(0..10);

                    let len = self.vec().get_mut(outer_idx).unwrap().len();

                    if len == 0 {
                        return self.next();
                    }

                    let idx = self.rng.gen_range(0..len);
                    let str = generate_random_string(&mut self.rng);

                    if self
                        .vec()
                        .get_mut(outer_idx)
                        .unwrap()
                        .get_mut(idx)
                        .unwrap()
                        .with(|s: &mut String| {
                            *s = str.clone();
                        })
                        .is_err()
                    {
                        return;
                    }

                    *self
                        .example
                        .get_mut(outer_idx)
                        .unwrap()
                        .get_mut(idx)
                        .unwrap() = str;

                    self.log.push(Action::GetMut(outer_idx, idx));
                }
                // CLEAR
                1001..=1010 => {
                    let outer_idx = self.rng.gen_range(0..10);

                    self.vec().get_mut(outer_idx).unwrap().clear();
                    self.example.get_mut(outer_idx).unwrap().clear();

                    self.log.push(Action::Clear(outer_idx));
                }
                // CANISTER UPGRADE
                _ => match SBox::new(self.vec.take().unwrap()) {
                    Ok(data) => {
                        store_custom_data(1, data);

                        if stable_memory_pre_upgrade().is_ok() {
                            stable_memory_post_upgrade();
                        }

                        self.vec = retrieve_custom_data::<SVec<SVec<SBox<String>>>>(1)
                            .map(|it| it.into_inner());
                        self.log.push(Action::CanisterUpgrade);
                    }
                    Err(vec) => {
                        self.vec = Some(vec);
                    }
                },
            }

            _debug_validate_allocator();
            assert_eq!(self.vec().len(), self.example.len());

            for i in 0..self.vec().len() {
                let svec = self.vec.as_ref().unwrap().get(i).unwrap();
                let example = self.example.get(i).unwrap();

                assert_eq!(svec.len(), example.len());

                for j in 0..svec.len() {
                    assert_eq!(
                        svec.get(j).unwrap().clone(),
                        example.get(j).unwrap().clone()
                    );
                }
            }
        }
    }

    #[test]
    fn fuzzer_works_fine() {
        stable::clear();
        init_allocator(0);

        {
            let mut fuzzer = Fuzzer::new();

            for _ in 0..10_000 {
                fuzzer.next();
            }
        }

        assert_eq!(get_allocated_size(), 0);
    }

    #[test]
    fn fuzzer_works_fine_limited_memory() {
        stable::clear();
        init_allocator(10);

        {
            let mut fuzzer = Fuzzer::new();

            for _ in 0..10_000 {
                fuzzer.next();
            }
        }

        assert_eq!(get_allocated_size(), 0);
    }
}
