use crate::mem::s_slice::{SSlice, Side};
use crate::primitive::StackAllocated;
use crate::utils::phantom_data::SPhantomData;
use crate::{allocate, deallocate, reallocate};
use speedy::{Context, LittleEndian, Readable, Reader, Writable, Writer};
use std::cmp::Ordering;
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::mem::size_of;

pub struct SBoxMut<T> {
    slice: SSlice,
    _marker: SPhantomData<T>,
    _null_ptr: *const u8,
}

impl<T> SBoxMut<T> {
    pub fn as_ptr(&self) -> u64 {
        self.slice.get_ptr()
    }

    pub unsafe fn from_ptr(ptr: u64) -> Self {
        let slice = SSlice::from_ptr(ptr, Side::Start).unwrap();

        Self {
            slice,
            _marker: SPhantomData::default(),
            _null_ptr: std::ptr::null(),
        }
    }
}

impl<'a, T: Readable<'a, LittleEndian> + Writable<LittleEndian>> SBoxMut<T> {
    pub fn new(it: &T) -> Self {
        let buf = it.write_to_vec().unwrap();
        let inner_slice = allocate(buf.len());

        inner_slice.write_bytes(0, &buf);

        let slice = allocate(size_of::<u64>());
        slice.write_word(0, inner_slice.get_ptr());

        Self {
            slice,
            _marker: SPhantomData::default(),
            _null_ptr: std::ptr::null(),
        }
    }

    pub unsafe fn drop(self) -> T {
        let inner_slice_ptr = self.slice.read_word(0);
        let inner_slice = SSlice::from_ptr(inner_slice_ptr, Side::Start).unwrap();

        let mut buf = vec![0u8; inner_slice.get_size_bytes()];
        let it = T::read_from_buffer_copying_data(&buf).unwrap();

        deallocate(self.slice);
        deallocate(inner_slice);

        it
    }

    pub fn get_cloned(&self) -> T {
        let inner_slice_ptr = self.slice.read_word(0);
        let inner_slice = SSlice::from_ptr(inner_slice_ptr, Side::Start).unwrap();

        let mut buf = vec![0u8; inner_slice.get_size_bytes()];
        inner_slice.read_bytes(0, &mut buf);

        T::read_from_buffer_copying_data(&buf).unwrap()
    }

    pub fn set(&mut self, it: &T) {
        let inner_slice_ptr = self.slice.read_word(0);
        let inner_slice = SSlice::from_ptr(inner_slice_ptr, Side::Start).unwrap();

        let buf = it.write_to_vec().unwrap();

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
            self.slice.write_word(0, inner_slice.get_ptr());
        }
    }
}

impl<'a, T: Readable<'a, LittleEndian> + Writable<LittleEndian>> StackAllocated<SBoxMut<T>, u64>
    for SBoxMut<T>
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
    fn to_u8_fixed_size_array(it: SBoxMut<T>) -> [u8; size_of::<u64>()] {
        u64::to_u8_fixed_size_array(it.slice.get_ptr())
    }

    fn from_u8_fixed_size_array(arr: [u8; size_of::<u64>()]) -> Self {
        let ptr = u64::from_u8_fixed_size_array(arr);

        unsafe { Self::from_ptr(ptr) }
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
        self.get_cloned().eq(&other.get_cloned())
    }
}

impl<'a, T: PartialOrd + Readable<'a, LittleEndian> + Writable<LittleEndian>> PartialOrd
    for SBoxMut<T>
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.get_cloned().partial_cmp(&other.get_cloned())
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
        self.get_cloned().cmp(&other.get_cloned())
    }
}

impl<'a, T: Default + Readable<'a, LittleEndian> + Writable<LittleEndian>> Default for SBoxMut<T> {
    fn default() -> Self {
        Self::new(&Default::default())
    }
}

impl<'a, T: Hash + Readable<'a, LittleEndian> + Writable<LittleEndian>> Hash for SBoxMut<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.get_cloned().hash(state);
    }
}

impl<'a, T: Debug + Readable<'a, LittleEndian> + Writable<LittleEndian>> Debug for SBoxMut<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("SBoxMut(")?;

        self.get_cloned().fmt(f)?;

        f.write_str(")")
    }
}
