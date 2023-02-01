use crate::encoding::{AsDynSizeBytes, AsFixedSizeBytes, Buffer};
use crate::mem::s_slice::{SSlice, Side};
use crate::primitive::{StableAllocated, StableDrop};
use crate::utils::certification::{AsHashTree, AsHashableBytes};
use crate::utils::Anyway;
use crate::{allocate, deallocate, reallocate};
use std::borrow::Borrow;
use std::cmp::Ordering;
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::{Deref, DerefMut};

pub struct SBox<T> {
    slice: Option<SSlice>,
    inner: T,
}

impl<T> SBox<T> {
    #[inline]
    pub fn new(it: T) -> Self {
        Self {
            slice: None,
            inner: it,
        }
    }

    #[inline]
    pub fn as_ptr(&self) -> u64 {
        self.slice.unwrap().as_ptr()
    }

    #[inline]
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T: AsDynSizeBytes> SBox<T> {
    pub unsafe fn from_ptr(ptr: u64) -> Self {
        let slice = SSlice::from_ptr(ptr, Side::Start).unwrap();

        let mut buf = vec![0u8; slice.get_size_bytes()];
        slice.read_bytes(0, &mut buf);

        let inner = T::from_dyn_size_bytes(&buf);

        Self {
            slice: Some(slice),
            inner,
        }
    }

    pub unsafe fn get_cloned(&self) -> T {
        if let Some(slice) = self.slice {
            let mut buf = vec![0u8; slice.get_size_bytes()];
            slice.read_bytes(0, &mut buf);

            T::from_dyn_size_bytes(&buf)
        } else {
            unreachable!()
        }
    }

    #[inline]
    pub fn get(&self) -> SBoxRef<'_, T> {
        SBoxRef(self)
    }

    #[inline]
    pub fn get_mut(&mut self) -> SBoxRefMut<'_, T> {
        SBoxRefMut(self)
    }

    fn repersist(&mut self) {
        if let Some(mut slice) = self.slice.take() {
            let buf = self.inner.as_dyn_size_bytes();

            if slice.get_size_bytes() < buf.len() {
                slice = reallocate(slice, buf.len()).anyway();
            }

            slice.write_bytes(0, &buf);
            self.slice = Some(slice);
        }
    }
}

impl<T: AsDynSizeBytes> AsFixedSizeBytes for SBox<T> {
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

impl<T: AsDynSizeBytes> StableAllocated for SBox<T> {
    fn move_to_stable(&mut self) {
        if self.slice.is_none() {
            let buf = self.inner.as_dyn_size_bytes();
            let slice = allocate(buf.len());

            slice.write_bytes(0, &buf);

            self.slice = Some(slice);
        }
    }

    fn remove_from_stable(&mut self) {
        if let Some(slice) = self.slice {
            deallocate(slice);

            self.slice = None;
        }
    }
}

impl<T: StableDrop> StableDrop for SBox<T> {
    type Output = ();

    #[inline]
    unsafe fn stable_drop(mut self) -> Self::Output {
        if let Some(slice) = self.slice {
            deallocate(slice);

            self.slice = None;
        }

        self.inner.stable_drop();
    }
}

impl<T: AsHashableBytes> AsHashableBytes for SBox<T> {
    #[inline]
    fn as_hashable_bytes(&self) -> Vec<u8> {
        self.inner.as_hashable_bytes()
    }
}

impl<T: AsHashTree> AsHashTree for SBox<T> {
    #[inline]
    fn root_hash(&self) -> crate::utils::certification::Hash {
        self.inner.root_hash()
    }
}

impl<T: PartialEq> PartialEq for SBox<T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.inner.eq(&other.inner)
    }
}

impl<T: PartialOrd> PartialOrd for SBox<T> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.inner.partial_cmp(&other.inner)
    }
}

impl<T: Eq + PartialEq> Eq for SBox<T> {}

impl<T: Ord + PartialOrd> Ord for SBox<T> {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.inner.cmp(&other.inner)
    }
}

impl<T: Default> Default for SBox<T> {
    #[inline]
    fn default() -> Self {
        Self::new(Default::default())
    }
}

impl<T: Hash> Hash for SBox<T> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.inner.hash(state);
    }
}

impl<T: Debug> Debug for SBox<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("SBox(")?;

        self.inner.fmt(f)?;

        f.write_str(")")
    }
}

impl<T: Clone> Clone for SBox<T> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            slice: None,
            inner: self.inner.clone(),
        }
    }
}

impl<T> Borrow<T> for SBox<T> {
    #[inline]
    fn borrow(&self) -> &T {
        &self.inner
    }
}

pub struct SBoxRefMut<'a, T: AsDynSizeBytes>(&'a mut SBox<T>);

impl<'a, T: AsDynSizeBytes> Deref for SBoxRefMut<'a, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0.inner
    }
}

impl<'a, T: AsDynSizeBytes> DerefMut for SBoxRefMut<'a, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0.inner
    }
}

impl<'a, T: AsDynSizeBytes> Drop for SBoxRefMut<'a, T> {
    fn drop(&mut self) {
        self.0.repersist();
    }
}

pub struct SBoxRef<'a, T: AsDynSizeBytes>(&'a SBox<T>);

impl<'a, T: AsDynSizeBytes> Deref for SBoxRef<'a, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0.inner
    }
}

#[cfg(test)]
mod tests {
    use crate::primitive::s_box::SBox;
    use std::cmp::Ordering;
    use std::ops::Deref;

    #[test]
    fn sboxes_work_fine() {
        let mut sbox1 = SBox::new(10);
        let mut sbox11 = SBox::new(10);
        let mut sbox2 = SBox::new(20);

        assert_eq!(sbox1.get().deref(), &10);
        assert_eq!(*sbox1.get(), 10);

        assert!(sbox1 < sbox2);
        assert!(sbox2 > sbox1);
        assert_eq!(sbox1, sbox11);

        println!("{:?}", sbox1);

        let sbox = SBox::<i32>::default();
        assert!(matches!(sbox1.cmp(&sbox), Ordering::Greater));
    }
}
