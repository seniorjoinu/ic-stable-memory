use crate::encoding::{AsDynSizeBytes, AsFixedSizeBytes};
use crate::mem::s_slice::SSlice;
use crate::primitive::StableType;
use crate::utils::certification::{AsHashTree, AsHashableBytes, HashTree};
use crate::{allocate, deallocate, reallocate, OutOfMemory};
use std::borrow::Borrow;
use std::cell::UnsafeCell;
use std::cmp::Ordering;
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::mem::ManuallyDrop;
use std::ops::Deref;

/// Smart-pointer that allows storing any dynamic sized data on stable memory.
///
/// `T` should implement both [StableType] and [AsDynSizeBytes]. [SBox] itself implements [StableType],
/// so it will be stable-dropped automatically, when it is no longer needed. [SBox] also implements [AsFixedSizeBytes],
/// so you can put it in any other stable structure (e.g. [SVec](crate::collections::SVec)).
///
/// It is eager on writes, but lazy on reads. When you create or update it, the underlying value gets
/// immediately serialized and written to stable memory, but when you read it after it was stored in
/// some other stable data structure, it's underlying value gets read and deserialized only when you
/// access it.
///
/// You can access the underlying data by dereferencing it, for immutable access. For mutable access
/// you have to use [SBox::with] method (similar to `thread_local!`'s `with()` method).
///
/// # Examples
/// ```rust
/// # use ic_stable_memory::{stable_memory_init, SBox};
/// # unsafe { ic_stable_memory::mem::clear(); }
/// # stable_memory_init();
/// {
///     let mut boxed_string = SBox::new(String::from("Test string"))
///         .expect("Out of memory");
///
///     assert_eq!(&*boxed_string, "Test string");
///
///     boxed_string.with(|it| {
///         *it = String::from("Much much longer test string");
///     }).expect("Out of memory");
///
///     assert_eq!(&*boxed_string, "Much much longer test string");
/// } // <- gets stable-dropped here automatically
/// ```
pub struct SBox<T: AsDynSizeBytes + StableType> {
    slice: Option<SSlice>,
    inner: UnsafeCell<Option<T>>,
    stable_drop_flag: bool,
}

impl<T: AsDynSizeBytes + StableType> SBox<T> {
    /// Stores dynamic sized data on stable memory, immediately serializing and allocating.
    ///
    /// Returns `Err` and the data, if the canister is `OutOfMemory`.
    #[inline]
    pub fn new(mut it: T) -> Result<Self, T> {
        let buf = it.as_dyn_size_bytes();
        if let Ok(slice) = unsafe { allocate(buf.len() as u64) } {
            unsafe {
                crate::mem::write_bytes(slice.offset(0), &buf);
                it.stable_drop_flag_off();
            }

            Ok(Self {
                slice: Some(slice),
                inner: UnsafeCell::new(Some(it)),
                stable_drop_flag: true,
            })
        } else {
            Err(it)
        }
    }

    /// Returns a pointer to the underlying [SSlice] of stable memory.
    ///
    /// See also [SBox::from_ptr].
    #[inline]
    pub fn as_ptr(&self) -> u64 {
        self.slice.unwrap().as_ptr()
    }

    /// Returns the underlying data, releasing occupied stable memory.
    #[inline]
    pub fn into_inner(mut self) -> T {
        unsafe {
            self.lazy_read(true);
        };

        let res = self.inner.get_mut().take().unwrap();

        unsafe {
            self.stable_drop();
            self.stable_drop_flag_off();
        }

        res
    }

    /// Creates [SBox] from a pointer to the underlying [SSlice] of stable memory.
    ///
    /// See also [SBox::as_ptr].
    ///
    /// # Panics
    /// Panics if the pointer points to an invalid (or free) block of stable memory.
    ///
    /// # Safety
    /// This method basically allows you to clone the smart-pointer, which breaks ownership and
    /// stable-drop rules. Always make sure you restore stable-drop rules manually. Always destroy
    /// other copies of the same [SBox] before mutating it or the underlying data.
    ///
    /// # Example
    /// ```rust
    /// # use ic_stable_memory::{SBox, stable_memory_init, StableType};
    /// # unsafe { ic_stable_memory::mem::clear(); }
    /// # stable_memory_init();
    /// let mut b = SBox::new(10u64).expect("Out of memory");
    /// unsafe { b.stable_drop_flag_off() };
    ///
    /// b = unsafe { SBox::from_ptr(b.as_ptr()) };
    /// ```
    pub unsafe fn from_ptr(ptr: u64) -> Self {
        let slice = SSlice::from_ptr(ptr).unwrap();

        Self {
            stable_drop_flag: false,
            slice: Some(slice),
            inner: UnsafeCell::default(),
        }
    }

    /// Provides mutable access to the underlying data, by accepting a lambda function.
    ///
    /// Returns [OutOfMemory] error if it was impossible to reallocate the underlying [SSlice] to
    /// make it bugger.
    ///
    /// # Example
    /// ```rust
    /// # use ic_stable_memory::{SBox, stable_memory_init};
    /// # use ic_stable_memory::derive::{StableType, CandidAsDynSizeBytes};
    /// # use candid::{CandidType, Deserialize};
    /// # unsafe { ic_stable_memory::mem::clear(); }
    /// # stable_memory_init();
    ///
    /// #[derive(CandidType, Deserialize, CandidAsDynSizeBytes, StableType, Debug)]
    /// struct A {
    ///     name: String,
    ///     id: u64,
    /// }
    ///
    /// let it = A { name: String::from("Sasha"), id: 1 };
    /// let mut b = SBox::new(it).expect("Out of memory");
    ///
    /// b.with(|it| it.id += 1).unwrap();
    /// ```
    #[inline]
    pub fn with<R, F: FnOnce(&mut T) -> R>(&mut self, func: F) -> Result<R, OutOfMemory> {
        unsafe {
            self.lazy_read(true);

            let it = self.inner.get_mut().as_mut().unwrap();
            let res = func(it);

            self.repersist().map(|_| res)
        }
    }

    unsafe fn lazy_read(&self, drop_flag: bool) {
        if let Some(it) = (*self.inner.get()).as_mut() {
            if drop_flag {
                it.stable_drop_flag_on();
            } else {
                it.stable_drop_flag_off();
            }

            return;
        }

        let slice = self.slice.as_ref().unwrap();
        let mut buf = vec![0u8; slice.get_size_bytes() as usize];
        unsafe { crate::mem::read_bytes(slice.offset(0), &mut buf) };

        let mut inner = T::from_dyn_size_bytes(&buf);
        if drop_flag {
            inner.stable_drop_flag_on();
        } else {
            inner.stable_drop_flag_off();
        }

        *self.inner.get() = Some(inner);
    }

    fn repersist(&mut self) -> Result<(), OutOfMemory> {
        let mut slice = self.slice.take().unwrap();
        let buf = self.inner.get_mut().as_ref().unwrap().as_dyn_size_bytes();

        unsafe { self.inner.get_mut().stable_drop_flag_off() };

        if slice.get_size_bytes() < buf.len() as u64 {
            // won't panic, because buf.len() is always less or equal to u32::MAX
            match unsafe { reallocate(slice, buf.len() as u64) } {
                Ok(s) => {
                    slice = s;
                }
                Err(e) => {
                    self.slice = Some(slice);
                    return Err(e);
                }
            }
        }

        unsafe { crate::mem::write_bytes(slice.offset(0), &buf) };
        self.slice = Some(slice);

        Ok(())
    }
}

impl<T: AsDynSizeBytes + StableType> AsFixedSizeBytes for SBox<T> {
    const SIZE: usize = u64::SIZE;
    type Buf = [u8; u64::SIZE];

    #[inline]
    fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
        self.as_ptr().as_fixed_size_bytes(buf)
    }

    #[inline]
    fn from_fixed_size_bytes(arr: &[u8]) -> Self {
        let ptr = u64::from_fixed_size_bytes(arr);

        unsafe { Self::from_ptr(ptr) }
    }
}

impl<T: AsDynSizeBytes + StableType> StableType for SBox<T> {
    #[inline]
    fn should_stable_drop(&self) -> bool {
        self.stable_drop_flag
    }

    #[inline]
    unsafe fn stable_drop_flag_off(&mut self) {
        self.stable_drop_flag = false;
    }

    #[inline]
    unsafe fn stable_drop_flag_on(&mut self) {
        self.stable_drop_flag = true;
    }

    #[inline]
    unsafe fn stable_drop(&mut self) {
        deallocate(self.slice.take().unwrap());
    }
}

impl<T: AsDynSizeBytes + StableType> Drop for SBox<T> {
    fn drop(&mut self) {
        unsafe {
            if self.should_stable_drop() {
                self.lazy_read(true);
                self.stable_drop();
            }
        }
    }
}

impl<T: AsHashableBytes + AsDynSizeBytes + StableType> AsHashableBytes for SBox<T> {
    #[inline]
    fn as_hashable_bytes(&self) -> Vec<u8> {
        unsafe {
            self.lazy_read(false);

            (*self.inner.get()).as_ref().unwrap().as_hashable_bytes()
        }
    }
}

impl<T: AsHashTree + AsDynSizeBytes + StableType> AsHashTree for SBox<T> {
    #[inline]
    fn root_hash(&self) -> crate::utils::certification::Hash {
        unsafe {
            self.lazy_read(false);

            (*self.inner.get()).as_ref().unwrap().root_hash()
        }
    }

    #[inline]
    fn hash_tree(&self) -> HashTree {
        unsafe {
            self.lazy_read(false);

            (*self.inner.get()).as_ref().unwrap().hash_tree()
        }
    }
}

impl<T: PartialEq + AsDynSizeBytes + StableType> PartialEq for SBox<T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        unsafe {
            self.lazy_read(false);
            other.lazy_read(false);

            (*self.inner.get()).eq(&(*other.inner.get()))
        }
    }
}

impl<T: PartialOrd + AsDynSizeBytes + StableType> PartialOrd for SBox<T> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        unsafe {
            self.lazy_read(false);
            other.lazy_read(false);

            (*self.inner.get()).partial_cmp(&(*other.inner.get()))
        }
    }
}

impl<T: Eq + PartialEq + AsDynSizeBytes + StableType> Eq for SBox<T> {}

impl<T: Ord + PartialOrd + AsDynSizeBytes + StableType> Ord for SBox<T> {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        unsafe {
            self.lazy_read(false);
            other.lazy_read(false);

            (*self.inner.get()).cmp(&(*other.inner.get()))
        }
    }
}

impl<T: Hash + AsDynSizeBytes + StableType> Hash for SBox<T> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        unsafe {
            self.lazy_read(false);

            (*self.inner.get()).as_ref().unwrap().hash(state);
        }
    }
}

impl<T: Debug + AsDynSizeBytes + StableType> Debug for SBox<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("SBox(")?;

        unsafe {
            self.lazy_read(false);

            (*self.inner.get()).as_ref().unwrap().fmt(f)?;
        }

        f.write_str(")")
    }
}

impl<T: AsDynSizeBytes + StableType> Borrow<T> for SBox<T> {
    #[inline]
    fn borrow(&self) -> &T {
        unsafe {
            self.lazy_read(false);

            (*self.inner.get()).as_ref().unwrap()
        }
    }
}

impl<T: AsDynSizeBytes + StableType> Deref for SBox<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe {
            self.lazy_read(false);

            (*self.inner.get()).as_ref().unwrap()
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::SVec;
    use crate::primitive::s_box::SBox;
    use crate::{
        _debug_validate_allocator, get_allocated_size, retrieve_custom_data, stable,
        stable_memory_init, store_custom_data,
    };
    use std::cmp::Ordering;
    use std::ops::Deref;

    #[test]
    fn sboxes_work_fine() {
        stable::clear();
        stable_memory_init();

        {
            let sbox = SBox::new(100).unwrap();
        }

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);

        {
            let mut sbox = SBox::new(100).unwrap();
            let mut o_sbox = SBox::new(sbox).unwrap();
            let mut oo_sbox = SBox::new(o_sbox).unwrap();

            store_custom_data(0, oo_sbox);
            oo_sbox = retrieve_custom_data::<SBox<SBox<i32>>>(0).unwrap();
        }

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);

        {
            let mut sbox = SBox::new(100).unwrap();
            let mut o_sbox = SBox::new(sbox).unwrap();
            let mut oo_sbox = SBox::new(o_sbox).unwrap();

            store_custom_data(0, oo_sbox);
            o_sbox = retrieve_custom_data::<SBox<SBox<i32>>>(0)
                .unwrap()
                .into_inner();

            o_sbox.with(|sbox| *sbox = SBox::new(200).unwrap()).unwrap();

            sbox = o_sbox.into_inner();

            assert_eq!(*sbox, 200);
        }

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);

        {
            let mut sbox1 = SBox::new(10).unwrap();
            let mut sbox11 = SBox::new(10).unwrap();
            let mut sbox2 = SBox::new(20).unwrap();

            assert_eq!(sbox1.deref(), &10);
            assert_eq!(*sbox1, 10);

            assert!(sbox1 < sbox2);
            assert!(sbox2 > sbox1);
            assert_eq!(sbox1, sbox11);

            println!("{:?}", sbox1);

            let sbox = SBox::<i32>::new(i32::default()).unwrap();
            assert!(matches!(sbox1.cmp(&sbox), Ordering::Greater));
        }

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
    }

    #[test]
    fn complex_nested_structures_work_fine() {
        stable::clear();
        stable_memory_init();

        {
            let mut b = SBox::new(Some(SVec::new())).unwrap();

            b.with(|it: &mut Option<SVec<u64>>| {
                if let Some(v) = it.as_mut() {
                    v.push(10);
                }
            });

            assert_eq!(*b.as_ref().unwrap().get(0).unwrap(), 10);

            store_custom_data(0, b);

            b = retrieve_custom_data(0).unwrap();

            assert_eq!(*b.as_ref().unwrap().get(0).unwrap(), 10);

            b.with(|it: &mut Option<SVec<u64>>| {
                *it = None;
            });
        }

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
    }
}
