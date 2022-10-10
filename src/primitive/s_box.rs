use crate::mem::s_slice::{SSlice, Side};
use crate::primitive::StackAllocated;
use crate::utils::uninit_u8_vec_of_size;
use crate::{allocate, deallocate};
use speedy::{Context, LittleEndian, Readable, Reader, Writable, Writer};
use std::cmp::Ordering;
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::mem::size_of;
use std::ops::Deref;

pub struct SBox<T> {
    slice: SSlice,
    inner: T,
}

impl<T> SBox<T> {
    pub fn as_ptr(&self) -> u64 {
        self.slice.ptr
    }

    pub unsafe fn drop(self) -> T {
        deallocate(self.slice);

        self.inner
    }
}

impl<'a, T: Readable<'a, LittleEndian> + Writable<LittleEndian>>
    StackAllocated<SBox<T>, [u8; size_of::<u64>()]> for SBox<T>
{
    #[inline]
    fn size_of_u8_array() -> usize {
        size_of::<u64>()
    }

    #[inline]
    fn fixed_size_u8_array() -> [u8; size_of::<u64>()] {
        [0u8; size_of::<u64>()]
    }

    #[inline]
    fn as_u8_slice(it: &Self) -> &[u8] {
        u64::as_u8_slice(&it.slice.ptr)
    }

    fn from_u8_fixed_size_array(arr: [u8; size_of::<u64>()]) -> Self {
        let ptr = u64::from_u8_fixed_size_array(arr);

        unsafe { Self::from_ptr(ptr) }
    }
}

impl<'a, T: Readable<'a, LittleEndian> + Writable<LittleEndian>> SBox<T> {
    pub fn new(it: T) -> Self {
        let buf = it.write_to_vec().unwrap();
        let slice = allocate(buf.len());

        slice.write_bytes(0, &buf);

        Self { slice, inner: it }
    }

    pub unsafe fn from_ptr(ptr: u64) -> Self {
        let slice = SSlice::from_ptr(ptr, Side::Start).unwrap();

        let mut buf = unsafe { uninit_u8_vec_of_size(slice.get_size_bytes()) };
        slice.read_bytes(0, &mut buf);

        let inner = T::read_from_buffer_copying_data(&buf).unwrap();

        Self { slice, inner }
    }

    pub fn get_cloned(&self) -> T {
        let mut buf = unsafe { uninit_u8_vec_of_size(self.slice.get_size_bytes()) };
        self.slice.read_bytes(0, &mut buf);

        T::read_from_buffer_copying_data(&buf).unwrap()
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
        writer.write_u64(self.slice.ptr)
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
