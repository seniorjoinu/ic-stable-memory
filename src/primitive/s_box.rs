use crate::mem::s_slice::{SSlice, Side};
use crate::primitive::StableAllocated;
use crate::{allocate, deallocate};
use copy_as_bytes::traits::{AsBytes, SuperSized};
use speedy::{Context, LittleEndian, Readable, Reader, Writable, Writer};
use std::cmp::Ordering;
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::Deref;

pub struct SBox<T> {
    slice: Option<SSlice>,
    inner: T,
}

impl<T> SBox<T> {
    pub fn new(it: T) -> Self {
        Self {
            slice: None,
            inner: it,
        }
    }

    pub fn as_ptr(&self) -> u64 {
        self.slice.unwrap().get_ptr()
    }

    pub fn get(&self) -> &T {
        &self.inner
    }
}

impl<'a, T: Readable<'a, LittleEndian>> SBox<T> {
    pub unsafe fn from_ptr(ptr: u64) -> Self {
        let slice = SSlice::from_ptr(ptr, Side::Start).unwrap();

        let mut buf = vec![0u8; slice.get_size_bytes()];
        slice.read_bytes(0, &mut buf);

        let inner = T::read_from_buffer_copying_data(&buf).unwrap();

        Self {
            slice: Some(slice),
            inner,
        }
    }

    pub fn get_cloned(&self) -> T {
        if let Some(slice) = self.slice {
            let mut buf = vec![0u8; slice.get_size_bytes()];
            slice.read_bytes(0, &mut buf);

            T::read_from_buffer_copying_data(&buf).unwrap()
        } else {
            unreachable!()
        }
    }
}

impl<T> SuperSized for SBox<T> {
    const SIZE: usize = u64::SIZE;
}

impl<'a, T: Readable<'a, LittleEndian> + Writable<LittleEndian>> AsBytes for SBox<T> {
    fn to_bytes(self) -> [u8; Self::SIZE] {
        self.as_ptr().to_bytes()
    }

    fn from_bytes(arr: [u8; Self::SIZE]) -> Self {
        let ptr = u64::from_bytes(arr);

        unsafe { Self::from_ptr(ptr) }
    }
}

impl<'a, T: Readable<'a, LittleEndian> + Writable<LittleEndian>> StableAllocated for SBox<T> {
    fn move_to_stable(&mut self) {
        if self.slice.is_none() {
            let buf = self.inner.write_to_vec().unwrap();
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

    #[inline]
    unsafe fn stable_drop(mut self) {
        self.remove_from_stable();
    }
}

impl<'a, T: Readable<'a, LittleEndian> + Writable<LittleEndian>> Readable<'a, LittleEndian>
    for SBox<T>
{
    fn read_from<R: Reader<'a, LittleEndian>>(
        reader: &mut R,
    ) -> Result<Self, <speedy::LittleEndian as Context>::Error> {
        let ptr = reader.read_u64()?;

        Ok(unsafe { Self::from_ptr(ptr) })
    }
}

impl<T: Writable<LittleEndian>> Writable<LittleEndian> for SBox<T> {
    fn write_to<W: ?Sized + Writer<LittleEndian>>(
        &self,
        writer: &mut W,
    ) -> Result<(), <speedy::LittleEndian as Context>::Error> {
        writer.write_u64(self.as_ptr())
    }
}

impl<'a, T: Readable<'a, LittleEndian> + Writable<LittleEndian>> Deref for SBox<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a, T: PartialEq + Readable<'a, LittleEndian> + Writable<LittleEndian>> PartialEq for SBox<T> {
    fn eq(&self, other: &Self) -> bool {
        self.inner.eq(&other.inner)
    }
}

impl<'a, T: PartialOrd + Readable<'a, LittleEndian> + Writable<LittleEndian>> PartialOrd
    for SBox<T>
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.inner.partial_cmp(&other.inner)
    }
}

impl<'a, T: Eq + PartialEq + Readable<'a, LittleEndian> + Writable<LittleEndian>> Eq for SBox<T> {}

impl<'a, T: Ord + PartialOrd + Readable<'a, LittleEndian> + Writable<LittleEndian>> Ord
    for SBox<T>
{
    fn cmp(&self, other: &Self) -> Ordering {
        self.inner.cmp(&other.inner)
    }
}

impl<'a, T: Default + Readable<'a, LittleEndian> + Writable<LittleEndian>> Default for SBox<T> {
    fn default() -> Self {
        Self::new(Default::default())
    }
}

impl<T: Hash> Hash for SBox<T> {
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

#[cfg(test)]
mod tests {
    use crate::primitive::s_box::SBox;
    use std::cmp::Ordering;

    #[test]
    fn sboxes_work_fine() {
        let sbox1 = SBox::new(10);
        let sbox11 = SBox::new(10);
        let sbox2 = SBox::new(20);

        assert_eq!(sbox1.get(), &10);
        assert_eq!(*sbox1, 10);

        assert!(sbox1 < sbox2);
        assert!(sbox2 > sbox1);
        assert_eq!(sbox1, sbox11);

        println!("{:?}", sbox1);

        let sbox = SBox::<i32>::default();
        assert!(matches!(sbox1.cmp(&sbox), Ordering::Greater));
    }
}
