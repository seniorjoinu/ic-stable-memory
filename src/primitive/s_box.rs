use crate::encoding::{AsDynSizeBytes, AsFixedSizeBytes};
use crate::mem::s_slice::SSlice;
use crate::primitive::StableType;
use crate::utils::certification::{AsHashTree, AsHashableBytes};
use crate::{allocate, deallocate, reallocate, OutOfMemory};
use std::borrow::Borrow;
use std::cmp::Ordering;
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::Deref;

pub struct SBox<T: AsDynSizeBytes + StableType> {
    slice: Option<SSlice>,
    inner: Option<T>,
    is_owned: bool,
}

impl<T: AsDynSizeBytes + StableType> SBox<T> {
    /// DONT PUT REFERENCES INSIDE
    #[inline]
    pub fn new(it: T) -> Result<Self, T> {
        let buf = it.as_dyn_size_bytes();
        if let Ok(slice) = allocate(buf.len() as u64) {
            unsafe { crate::mem::write_bytes(slice.offset(0), &buf) };

            Ok(Self {
                slice: Some(slice),
                inner: Some(it),
                is_owned: false,
            })
        } else {
            Err(it)
        }
    }

    #[inline]
    pub fn as_ptr(&self) -> u64 {
        self.slice.unwrap().as_ptr()
    }

    #[inline]
    pub fn into_inner(mut self) -> T {
        self.inner.take().unwrap()
    }

    pub unsafe fn from_ptr(ptr: u64) -> Self {
        let slice = SSlice::from_ptr(ptr).unwrap();

        let mut buf = vec![0u8; slice.get_size_bytes() as usize];
        unsafe { crate::mem::read_bytes(slice.offset(0), &mut buf) };

        let inner = Some(T::from_dyn_size_bytes(&buf));

        Self {
            is_owned: false,
            slice: Some(slice),
            inner,
        }
    }

    #[inline]
    pub fn get(&self) -> SBoxRef<'_, T> {
        SBoxRef(self)
    }

    #[inline]
    pub fn with<R, F: FnOnce(&mut T) -> R>(&mut self, func: F) -> Result<R, OutOfMemory> {
        let it = self.inner.as_mut().unwrap();
        let res = func(it);

        self.repersist().map(|_| res)
    }

    fn repersist(&mut self) -> Result<(), OutOfMemory> {
        if let Some(mut slice) = self.slice.take() {
            let buf = self.inner.as_ref().unwrap().as_dyn_size_bytes();

            if slice.get_size_bytes() < buf.len() as u64 {
                // safe, since buf.len() is always less or equal to usize::MAX
                unsafe {
                    match reallocate(slice, buf.len() as u64) {
                        Ok(s) => {
                            slice = s;
                        }
                        Err(e) => {
                            self.slice = Some(slice);
                            return Err(e);
                        }
                    }
                }
            }

            unsafe { crate::mem::write_bytes(slice.offset(0), &buf) };
            self.slice = Some(slice);
        }

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
    fn is_owned_by_stable_memory(&self) -> bool {
        self.is_owned
    }

    #[inline]
    unsafe fn assume_owned_by_stable_memory(&mut self) {
        self.is_owned = true;

        if let Some(it) = self.inner.as_mut() {
            it.assume_owned_by_stable_memory();
        }
    }

    #[inline]
    unsafe fn assume_not_owned_by_stable_memory(&mut self) {
        self.is_owned = false;

        if let Some(it) = self.inner.as_mut() {
            it.assume_not_owned_by_stable_memory();
        }
    }

    #[inline]
    unsafe fn stable_drop(&mut self) {
        if let Some(slice) = self.slice.take() {
            deallocate(slice);
        }
    }
}

impl<T: AsDynSizeBytes + StableType> Drop for SBox<T> {
    fn drop(&mut self) {
        if !self.is_owned_by_stable_memory() {
            unsafe {
                self.stable_drop();
            }
        }
    }
}

impl<T: AsHashableBytes + AsDynSizeBytes + StableType> AsHashableBytes for SBox<T> {
    #[inline]
    fn as_hashable_bytes(&self) -> Vec<u8> {
        self.inner.as_ref().unwrap().as_hashable_bytes()
    }
}

impl<T: AsHashTree + AsDynSizeBytes + StableType> AsHashTree for SBox<T> {
    #[inline]
    fn root_hash(&self) -> crate::utils::certification::Hash {
        self.inner.as_ref().unwrap().root_hash()
    }
}

impl<T: PartialEq + AsDynSizeBytes + StableType> PartialEq for SBox<T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.inner.eq(&other.inner)
    }
}

impl<T: PartialOrd + AsDynSizeBytes + StableType> PartialOrd for SBox<T> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.inner.partial_cmp(&other.inner)
    }
}

impl<T: Eq + PartialEq + AsDynSizeBytes + StableType> Eq for SBox<T> {}

impl<T: Ord + PartialOrd + AsDynSizeBytes + StableType> Ord for SBox<T> {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.inner.cmp(&other.inner)
    }
}

impl<T: Hash + AsDynSizeBytes + StableType> Hash for SBox<T> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.inner.as_ref().unwrap().hash(state);
    }
}

impl<T: Debug + AsDynSizeBytes + StableType> Debug for SBox<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("SBox(")?;

        self.inner.as_ref().unwrap().fmt(f)?;

        f.write_str(")")
    }
}

impl<T: Clone + AsDynSizeBytes + StableType> Clone for SBox<T> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            is_owned: false,
            slice: None,
            inner: self.inner.clone(),
        }
    }
}

impl<T: AsDynSizeBytes + StableType> Borrow<T> for SBox<T> {
    #[inline]
    fn borrow(&self) -> &T {
        self.inner.as_ref().unwrap()
    }
}

pub struct SBoxRef<'a, T: AsDynSizeBytes + StableType>(&'a SBox<T>);

impl<'a, T: AsDynSizeBytes + StableType> Deref for SBoxRef<'a, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.0.inner.as_ref().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use crate::primitive::s_box::SBox;
    use crate::{_debug_validate_allocator, get_allocated_size, stable, stable_memory_init};
    use std::cmp::Ordering;
    use std::ops::Deref;

    #[test]
    fn sboxes_work_fine() {
        stable::clear();
        stable_memory_init();

        {
            let mut sbox1 = SBox::new(10).unwrap();
            let mut sbox11 = SBox::new(10).unwrap();
            let mut sbox2 = SBox::new(20).unwrap();

            assert_eq!(sbox1.get().deref(), &10);
            assert_eq!(*sbox1.get(), 10);

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
}
