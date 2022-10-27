use crate::mem::s_slice::{SSlice, Side};
use crate::primitive::StableAllocated;
use crate::{allocate, deallocate, reallocate};
use copy_as_bytes::traits::{AsBytes, SuperSized};
use speedy::{Context, LittleEndian, Readable, Reader, Writable, Writer};
use std::cmp::Ordering;
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::{Deref, DerefMut};

pub struct SBoxMut<T> {
    outer_slice: Option<SSlice>,
    inner: T,
}

impl<T> SBoxMut<T> {
    pub fn new(it: T) -> Self {
        Self {
            outer_slice: None,
            inner: it,
        }
    }

    pub fn get(&self) -> &T {
        &self.inner
    }

    pub fn as_ptr(&self) -> u64 {
        self.outer_slice.unwrap().get_ptr()
    }
}

impl<'a, T: Readable<'a, LittleEndian> + Writable<LittleEndian>> SBoxMut<T> {
    pub fn get_mut(&mut self) -> SMutRef<T> {
        SMutRef::new(self)
    }
}

impl<'a, T: Readable<'a, LittleEndian>> SBoxMut<T> {
    pub unsafe fn from_ptr(ptr: u64) -> Self {
        let outer_slice = SSlice::from_ptr(ptr, Side::Start).unwrap();

        let inner_slice_ptr = outer_slice.as_bytes_read(0);
        let inner_slice = SSlice::from_ptr(inner_slice_ptr, Side::Start).unwrap();

        let mut buf = vec![0u8; inner_slice.get_size_bytes()];
        let it = T::read_from_buffer_copying_data(&buf).unwrap();

        Self {
            outer_slice: Some(outer_slice),
            inner: it,
        }
    }

    pub fn get_cloned(&self) -> T {
        let inner_slice_ptr = self.outer_slice.unwrap().as_bytes_read(0);
        let inner_slice = SSlice::from_ptr(inner_slice_ptr, Side::Start).unwrap();

        let mut buf = vec![0u8; inner_slice.get_size_bytes()];
        inner_slice.read_bytes(0, &mut buf);

        T::read_from_buffer_copying_data(&buf).unwrap()
    }
}

impl<T: Writable<LittleEndian>> SBoxMut<T> {
    fn repersist(&mut self) {
        if let Some(outer_slice) = self.outer_slice {
            let inner_slice_ptr = outer_slice.as_bytes_read(0);
            let inner_slice = SSlice::from_ptr(inner_slice_ptr, Side::Start).unwrap();

            let buf = self.inner.write_to_vec().unwrap();

            let (inner_slice, should_rewrite_outer) = if buf.len() > inner_slice.get_size_bytes() {
                match reallocate(inner_slice, buf.len()) {
                    Ok(slice) => (slice, false),
                    Err(slice) => (slice, true),
                }
            } else {
                (inner_slice, false)
            };

            inner_slice.write_bytes(0, &buf);

            if should_rewrite_outer {
                outer_slice.as_bytes_write(0, inner_slice.get_ptr());
            }
        }
    }
}

pub struct SMutRef<'a, T: Writable<LittleEndian>> {
    sbox: &'a mut SBoxMut<T>,
}

impl<'a, T: Writable<LittleEndian>> SMutRef<'a, T> {
    pub fn new(sbox: &'a mut SBoxMut<T>) -> Self {
        Self { sbox }
    }
}

impl<'a, T: Writable<LittleEndian>> Deref for SMutRef<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.sbox.inner
    }
}

impl<'a, T: Writable<LittleEndian>> DerefMut for SMutRef<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.sbox.inner
    }
}

impl<'a, T: Writable<LittleEndian>> Drop for SMutRef<'a, T> {
    fn drop(&mut self) {
        self.sbox.repersist();
    }
}

impl<T> SuperSized for SBoxMut<T> {
    const SIZE: usize = u64::SIZE;
}

impl<'a, T: Readable<'a, LittleEndian>> AsBytes for SBoxMut<T> {
    fn to_bytes(self) -> [u8; Self::SIZE] {
        self.as_ptr().to_bytes()
    }

    fn from_bytes(arr: [u8; Self::SIZE]) -> Self {
        let ptr = u64::from_bytes(arr);

        unsafe { Self::from_ptr(ptr) }
    }
}

impl<'a, T: Readable<'a, LittleEndian> + Writable<LittleEndian>> StableAllocated for SBoxMut<T> {
    fn move_to_stable(&mut self) {
        if self.outer_slice.is_none() {
            let buf = self.inner.write_to_vec().unwrap();
            let inner_slice = allocate(buf.len());

            inner_slice.write_bytes(0, &buf);

            let outer_slice = allocate(u64::SIZE);
            outer_slice.as_bytes_write(0, inner_slice.get_ptr());

            self.outer_slice = Some(outer_slice);
        }
    }

    fn remove_from_stable(&mut self) {
        if let Some(outer_slice) = self.outer_slice {
            let inner_slice_ptr = outer_slice.as_bytes_read(0);
            let inner_slice = SSlice::from_ptr(inner_slice_ptr, Side::Start).unwrap();

            deallocate(outer_slice);
            deallocate(inner_slice);

            self.outer_slice = None;
        }
    }

    #[inline]
    unsafe fn stable_drop(mut self) {
        self.remove_from_stable();
    }
}

impl<'a, T: Readable<'a, LittleEndian>> Readable<'a, LittleEndian> for SBoxMut<T> {
    fn read_from<R: Reader<'a, LittleEndian>>(
        reader: &mut R,
    ) -> Result<Self, <speedy::LittleEndian as Context>::Error> {
        let ptr = reader.read_u64()?;

        Ok(unsafe { Self::from_ptr(ptr) })
    }
}

impl<T: Writable<LittleEndian>> Writable<LittleEndian> for SBoxMut<T> {
    fn write_to<W: ?Sized + Writer<LittleEndian>>(
        &self,
        writer: &mut W,
    ) -> Result<(), <speedy::LittleEndian as Context>::Error> {
        writer.write_u64(self.as_ptr())
    }
}

impl<'a, T: PartialEq + Readable<'a, LittleEndian> + Writable<LittleEndian>> PartialEq
    for SBoxMut<T>
{
    fn eq(&self, other: &Self) -> bool {
        self.inner.eq(&other.inner)
    }
}

impl<'a, T: PartialOrd + Readable<'a, LittleEndian> + Writable<LittleEndian>> PartialOrd
    for SBoxMut<T>
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.inner.partial_cmp(&other.inner)
    }
}

impl<'a, T: Eq + PartialEq + Readable<'a, LittleEndian> + Writable<LittleEndian>> Eq
    for SBoxMut<T>
{
}

impl<'a, T: Ord + PartialOrd + Readable<'a, LittleEndian> + Writable<LittleEndian>> Ord
    for SBoxMut<T>
{
    fn cmp(&self, other: &Self) -> Ordering {
        self.inner.cmp(&other.inner)
    }
}

impl<'a, T: Default + Readable<'a, LittleEndian> + Writable<LittleEndian>> Default for SBoxMut<T> {
    fn default() -> Self {
        Self::new(Default::default())
    }
}

impl<'a, T: Hash + Readable<'a, LittleEndian> + Writable<LittleEndian>> Hash for SBoxMut<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.inner.hash(state);
    }
}

impl<'a, T: Debug + Readable<'a, LittleEndian> + Writable<LittleEndian>> Debug for SBoxMut<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("SBoxMut(")?;

        self.inner.fmt(f)?;

        f.write_str(")")
    }
}

impl<T> Deref for SBoxMut<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use crate::primitive::s_box_mut::SBoxMut;
    use crate::primitive::StableAllocated;
    use crate::{init_allocator, stable};
    use speedy::{Readable, Writable};
    use std::cmp::Ordering;

    #[test]
    fn sboxes_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let sbox1 = SBoxMut::new(10);
        let sbox11 = SBoxMut::new(10);
        let sbox2 = SBoxMut::new(20);

        assert_eq!(sbox1.get(), &10);
        assert_eq!(*sbox1, 10);

        assert!(sbox1 < sbox2);
        assert!(sbox2 > sbox1);
        assert_eq!(sbox1, sbox11);

        println!("{:?}", sbox1);

        let mut sbox = SBoxMut::<i32>::default();
        assert!(matches!(sbox1.cmp(&sbox), Ordering::Greater));

        sbox.move_to_stable();

        *sbox.get_mut() = 100;
        assert_eq!(*sbox, 100);
        assert_eq!(*sbox.get_mut(), 100);
        assert_eq!(sbox.get_cloned(), 100);

        sbox.remove_from_stable();

        let mut sbox = SBoxMut::<Vec<u8>>::default();
        sbox.move_to_stable();
        (*sbox.get_mut()).extend(vec![0u8; 100]);
        assert_eq!(sbox.get_cloned(), vec![0u8; 100]);

        let buf = sbox.write_to_vec().unwrap();
        let mut sbox = SBoxMut::<Vec<u8>>::read_from_buffer_copying_data(&buf).unwrap();
        assert_eq!(sbox.get_cloned(), vec![0u8; 100]);

        sbox.remove_from_stable();
    }
}
