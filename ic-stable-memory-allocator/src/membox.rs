use crate::mem_context::stable;
use crate::types::PAGE_SIZE_BYTES;
use candid::parser::value::{IDLValue, IDLValueVisitor};
use candid::types::{Serializer, Type};
use candid::{decode_one, encode_one, CandidType};
use serde::de::{DeserializeOwned, Error};
use serde::{Deserialize, Deserializer};
use std::marker::PhantomData;
use std::mem::size_of;

pub(crate) type Word = u64;
pub(crate) type Size = usize;

pub(crate) const ALLOCATED: Size = 2usize.pow(Size::BITS - 1); // first biggest bit set to 1, other set to 0
pub(crate) const FREE: Size = 2usize.pow(Size::BITS - 1) - 1; // first biggest bit set to 0, other set to 1
pub(crate) const MEM_BOX_META_SIZE: Size = size_of::<Size>() as Size;
pub(crate) const MEM_BOX_MIN_SIZE: Size = size_of::<Word>() as Size * 2;

pub(crate) enum Side {
    Start,
    End,
}

/// A smart-pointer for stable memory.
#[derive(Debug, Clone, Copy)]
pub struct MemBox<T> {
    ptr: Word,
    data: PhantomData<T>,
}

impl<T> CandidType for MemBox<T> {
    fn _ty() -> Type {
        Type::Nat64
    }

    fn idl_serialize<S>(&self, serializer: S) -> Result<(), S::Error>
    where
        S: Serializer,
    {
        self.get_ptr().idl_serialize(serializer)
    }
}

impl<'de, T> Deserialize<'de> for MemBox<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let idl_value = deserializer.deserialize_u64(IDLValueVisitor)?;
        match idl_value {
            IDLValue::Nat64(ptr) => Ok(MemBox {
                ptr,
                data: PhantomData::default(),
            }),
            _ => Err(D::Error::custom("Unable to deserialize a Membox")),
        }
    }
}

#[derive(Debug)]
pub enum CandidMemBoxError {
    CandidError(candid::Error),
    MemBoxOverflow(Vec<u8>),
}

impl<T: DeserializeOwned + CandidType> MemBox<T> {
    pub fn get_cloned(&self) -> Result<T, CandidMemBoxError> {
        let mut bytes = vec![0u8; self.get_size_bytes()];
        self._read_bytes(0, &mut bytes);

        decode_one(&bytes).map_err(CandidMemBoxError::CandidError)
    }

    pub fn set(&mut self, it: T) -> Result<(), CandidMemBoxError> {
        let bytes = encode_one(it).map_err(CandidMemBoxError::CandidError)?;
        if self.get_size_bytes() < bytes.len() {
            return Err(CandidMemBoxError::MemBoxOverflow(bytes));
        }

        self._write_bytes(0, &bytes);

        Ok(())
    }
}

impl<T> MemBox<T> {
    pub fn get_size_bytes(&self) -> usize {
        self.get_meta().0
    }

    pub fn _write_bytes(&mut self, offset: usize, data: &[u8]) {
        let (size, _) = self.get_meta();

        assert!(
            offset + data.len() as Size <= size,
            "MemBox overflow (max {}, provided {})",
            size,
            offset + data.len() as Size
        );

        stable::write(self.get_ptr() + (MEM_BOX_META_SIZE + offset) as Word, data);
    }

    pub fn _write_word(&mut self, offset: usize, word: u64) {
        let num = word.to_le_bytes();
        self._write_bytes(offset, &num);
    }

    pub fn _read_bytes(&self, offset: usize, data: &mut [u8]) {
        let (size, _) = self.get_meta();

        assert!(
            data.len() as Size + offset <= size,
            "MemBox overflow (max {}, provided {})",
            size,
            data.len() as Size + offset
        );

        stable::read(self.get_ptr() + (MEM_BOX_META_SIZE + offset) as Word, data);
    }

    pub fn _read_word(&self, offset: usize) -> u64 {
        let mut num = [0u8; size_of::<Word>()];
        self._read_bytes(offset, &mut num);

        Word::from_le_bytes(num)
    }

    /// # Safety
    /// Make sure there are no duplicates of this `MemBox`, before creating.
    pub(crate) unsafe fn new(ptr: Word, size: Size, allocated: bool) -> Self {
        assert!(
            size >= MEM_BOX_MIN_SIZE,
            "Size lesser than {} ({})",
            MEM_BOX_MIN_SIZE,
            size
        );
        assert!(size < ALLOCATED, "Size is bigger than {} ({})", FREE, size);
        assert!(ptr < stable::size_pages() * PAGE_SIZE_BYTES as Word);

        Self::write_meta(ptr, size, allocated);

        Self {
            ptr,
            data: PhantomData::default(),
        }
    }

    /// # Safety
    /// Make sure there no diplicates of this `MemBox`, before creation.
    pub(crate) unsafe fn new_total_size(ptr: Word, total_size: Size, allocated: bool) -> Self {
        Self::new(ptr, total_size - MEM_BOX_META_SIZE * 2, allocated)
    }

    /// # Safety
    /// This method may create a duplicate of the same unredlying memory slice. Make sure, your logic
    /// doesn't do that.
    pub(crate) unsafe fn from_ptr(mut ptr: Word, side: Side) -> Option<Self> {
        if ptr >= stable::size_pages() * PAGE_SIZE_BYTES as Word {
            return None;
        }

        let (size_1, size_2) = match side {
            Side::Start => {
                let (size_1, _) = Self::read_meta(ptr);
                if size_1 < MEM_BOX_MIN_SIZE {
                    return None;
                }

                let (size_2, _) = Self::read_meta(ptr + (MEM_BOX_META_SIZE + size_1) as Word);

                (size_1, size_2)
            }
            Side::End => {
                ptr -= MEM_BOX_META_SIZE as Word;
                let (size_2, _) = Self::read_meta(ptr);
                if size_2 < MEM_BOX_MIN_SIZE {
                    return None;
                }

                if ptr < (size_2 + MEM_BOX_META_SIZE) as Word {
                    return None;
                }

                ptr -= (size_2 + MEM_BOX_META_SIZE) as Word;
                let (size_1, _) = Self::read_meta(ptr);

                (size_1, size_2)
            }
        };

        if size_1 != size_2 {
            None
        } else {
            Some(Self {
                ptr,
                data: PhantomData::default(),
            })
        }
    }

    pub(crate) fn get_ptr(&self) -> Word {
        self.ptr
    }

    pub(crate) fn get_meta(&self) -> (Size, bool) {
        Self::read_meta(self.get_ptr())
    }

    pub(crate) fn set_allocated(&mut self, allocated: bool) {
        let (size, _) = self.get_meta();
        Self::write_meta(self.get_ptr(), size, allocated);
    }

    /// Splits this free `MemBox` into two new ones, if possible. The first one will have the provided size, the second
    /// one will have the rest (but not less than `min_size_second`. If size is not enough, returns
    /// `Err(self)`. Both new `MemBox`-es are free.
    ///
    /// # Safety
    /// Make sure there are no duplicates of this `MemBox` left before splitting.
    pub(crate) unsafe fn split(self, size_first: Size) -> Result<(Self, Self), Self> {
        assert!(
            size_first >= MEM_BOX_MIN_SIZE,
            "Size lesser than {} ({})",
            MEM_BOX_MIN_SIZE,
            size_first
        );

        let (size, allocated) = self.get_meta();
        self.assert_allocated(false, Some(allocated));

        if size < size_first + MEM_BOX_MIN_SIZE + MEM_BOX_META_SIZE * 2 {
            return Err(self);
        }

        let first = Self::new(self.get_ptr(), size_first, false);

        let size_second = size - size_first - MEM_BOX_META_SIZE * 2;

        let second = Self::new(first.get_next_neighbor_ptr(), size_second, false);

        Ok((first, second))
    }

    /// # Safety
    /// Make sure this MemBox and its neighbor are both have no duplicates, before merging.
    pub(crate) unsafe fn merge_with_neighbor(self, neighbor: Self) -> Self {
        let (self_size, self_allocated) = self.get_meta();
        self.assert_allocated(false, Some(self_allocated));

        let (neighbor_size, neighbor_allocated) = neighbor.get_meta();
        neighbor.assert_allocated(false, Some(neighbor_allocated));

        let self_ptr = self.get_ptr();
        let neighbor_ptr = neighbor.get_ptr();

        let n = if self_ptr > neighbor_ptr {
            self.get_neighbor(Side::Start).unwrap()
        } else {
            self.get_neighbor(Side::End).unwrap()
        };
        assert_eq!(n.get_ptr(), neighbor_ptr, "Not a neighbor");

        let ptr = if self_ptr > neighbor_ptr {
            neighbor_ptr
        } else {
            self_ptr
        };

        let size = self_size + neighbor_size + MEM_BOX_META_SIZE * 2;

        Self::new(ptr, size, false)
    }

    /// # Safety
    /// This method uses `MemBox::from_ptr()` under the hood. Follow its safety directions in order
    /// to do this right.
    pub(crate) unsafe fn get_neighbor(&self, side: Side) -> Option<Self> {
        match side {
            Side::Start => Self::from_ptr(self.get_ptr(), Side::End),
            Side::End => Self::from_ptr(self.get_next_neighbor_ptr(), Side::Start),
        }
    }

    pub(crate) fn get_next_neighbor_ptr(&self) -> Word {
        self.get_ptr() + (MEM_BOX_META_SIZE * 2 + self.get_meta().0) as Word
    }

    pub(crate) fn assert_allocated(&self, expected: bool, val: Option<bool>) {
        let actual = match val {
            Some(v) => v,
            None => {
                let (_, is_allocated) = self.get_meta();
                is_allocated
            }
        };

        assert_eq!(
            actual, expected,
            "Allocated assertion (expected {}, actual {})",
            expected, actual
        );
    }

    fn read_meta(ptr: Word) -> (Size, bool) {
        let mut meta = [0u8; MEM_BOX_META_SIZE as usize];
        stable::read(ptr, &mut meta);

        let encoded_size = Size::from_le_bytes(meta);
        let mut size = encoded_size;

        let allocated = if encoded_size & ALLOCATED == ALLOCATED {
            size &= FREE;
            true
        } else {
            false
        };

        (size, allocated)
    }

    fn write_meta(ptr: Word, size: Size, allocated: bool) {
        let encoded_size = if allocated {
            size | ALLOCATED
        } else {
            size & FREE
        };

        let meta = encoded_size.to_le_bytes();

        stable::write(ptr, &meta);
        stable::write(ptr + (MEM_BOX_META_SIZE + size) as Word, &meta);
    }
}

/// Only run these tests with `-- --test-threads=1`. It fails otherwise.
#[cfg(test)]
mod tests {
    use crate::mem_context::stable;
    use crate::membox::{CandidMemBoxError, MemBox, Side, Size, Word, MEM_BOX_META_SIZE};
    use candid::Nat;

    #[test]
    fn creation_works_fine() {
        unsafe {
            stable::clear();
            stable::grow(10).expect("Unable to grow");

            let m1_size: Size = 100;
            let m2_size: Size = 200;
            let m3_size: Size = 300;

            let m1 = MemBox::<()>::new(0, m1_size, false);
            assert_eq!(m1.get_meta(), (m1_size, false));
            assert_eq!(
                m1.get_next_neighbor_ptr(),
                (0 + m1_size + MEM_BOX_META_SIZE * 2) as Word
            );

            let m2 = MemBox::<()>::new(m1.get_next_neighbor_ptr(), m2_size, true);
            assert_eq!(m2.get_meta(), (m2_size, true));
            assert_eq!(
                m2.get_next_neighbor_ptr(),
                m1.get_next_neighbor_ptr() + (m2_size + MEM_BOX_META_SIZE * 2) as Word
            );

            let m3 = MemBox::<()>::new(m2.get_next_neighbor_ptr(), m3_size, false);
            assert_eq!(m3.get_meta(), (m3_size, false));
            assert_eq!(
                m3.get_next_neighbor_ptr(),
                m2.get_next_neighbor_ptr() + (m3_size + MEM_BOX_META_SIZE * 2) as Word
            );

            let m1 = MemBox::<()>::from_ptr(0, Side::Start).unwrap();
            assert_eq!(m1.get_meta(), (m1_size, false));
            assert_eq!(
                m1.get_next_neighbor_ptr(),
                0 + (m1_size + MEM_BOX_META_SIZE * 2) as Word
            );

            let m1 = MemBox::<()>::from_ptr(m1.get_next_neighbor_ptr(), Side::End).unwrap();
            assert_eq!(m1.get_meta(), (m1_size, false));
            assert_eq!(
                m1.get_next_neighbor_ptr(),
                0 + (m1_size + MEM_BOX_META_SIZE * 2) as Word
            );

            let m2 = MemBox::<()>::from_ptr(m1.get_next_neighbor_ptr(), Side::Start).unwrap();
            assert_eq!(m2.get_meta(), (m2_size, true));
            assert_eq!(
                m2.get_next_neighbor_ptr(),
                m1.get_next_neighbor_ptr() + (m2_size + MEM_BOX_META_SIZE * 2) as Word
            );

            let m2 = MemBox::<()>::from_ptr(m2.get_next_neighbor_ptr(), Side::End).unwrap();
            assert_eq!(m2.get_meta(), (m2_size, true));
            assert_eq!(
                m2.get_next_neighbor_ptr(),
                m1.get_next_neighbor_ptr() + (m2_size + MEM_BOX_META_SIZE * 2) as Word
            );

            let m3 = MemBox::<()>::from_ptr(m2.get_next_neighbor_ptr(), Side::Start).unwrap();
            assert_eq!(m3.get_meta(), (m3_size, false));
            assert_eq!(
                m3.get_next_neighbor_ptr(),
                m2.get_next_neighbor_ptr() + (m3_size + MEM_BOX_META_SIZE * 2) as Word
            );

            let m3 = MemBox::<()>::from_ptr(m3.get_next_neighbor_ptr(), Side::End).unwrap();
            assert_eq!(m3.get_meta(), (m3_size, false));
            assert_eq!(
                m3.get_next_neighbor_ptr(),
                m2.get_next_neighbor_ptr() + (m3_size + MEM_BOX_META_SIZE * 2) as Word
            );
        }
    }

    #[test]
    fn split_merge_work_fine() {
        unsafe {
            stable::clear();
            stable::grow(10).expect("Unable to grow");

            let m1_size: Size = 100;
            let m2_size: Size = 200;
            let m3_size: Size = 300;

            let m1 = MemBox::<()>::new(0, m1_size, false);
            let m2 = MemBox::<()>::new(m1.get_next_neighbor_ptr(), m2_size, false);
            let m3 = MemBox::<()>::new(m2.get_next_neighbor_ptr(), m3_size, false);

            let initial_m3_next_ptr = m3.get_next_neighbor_ptr();

            let (m3, m4) = m3.split(100).expect("Unable to split m3");
            assert_eq!(m3.get_meta(), (100, false));
            assert_eq!(m3.get_next_neighbor_ptr(), m4.get_ptr());

            assert_eq!(
                m4.get_meta(),
                (m3_size - 100 - 2 * MEM_BOX_META_SIZE, false)
            );
            assert_eq!(m4.get_next_neighbor_ptr(), initial_m3_next_ptr);

            let m3 = m4.merge_with_neighbor(m3);
            assert_eq!(m3.get_meta(), (m3_size, false));
            assert_eq!(m3.get_next_neighbor_ptr(), initial_m3_next_ptr);

            let m2 = m2.merge_with_neighbor(m3);
            assert_eq!(
                m2.get_meta(),
                (m2_size + m3_size + 2 * MEM_BOX_META_SIZE, false)
            );
            assert_eq!(m2.get_next_neighbor_ptr(), initial_m3_next_ptr);

            let m1 = m2.merge_with_neighbor(m1);
            assert_eq!(
                m1.get_meta(),
                (m1_size + m2_size + m3_size + 4 * MEM_BOX_META_SIZE, false)
            );
            assert_eq!(m1.get_next_neighbor_ptr(), initial_m3_next_ptr);

            let (m1, m2) = m1.split(m1_size).expect("Unable to split m1");
            assert_eq!(m1.get_meta(), (m1_size, false));
            assert_eq!(
                m2.get_meta(),
                (m2_size + m3_size + 2 * MEM_BOX_META_SIZE, false)
            );
            assert_eq!(m1.get_next_neighbor_ptr(), m2.get_ptr());
            assert_eq!(m2.get_next_neighbor_ptr(), initial_m3_next_ptr);

            let (m2, m3) = m2.split(m2_size).expect("Unable to split m2");
            assert_eq!(m2.get_meta(), (m2_size, false));
            assert_eq!(m3.get_meta(), (m3_size, false));
            assert_eq!(m2.get_next_neighbor_ptr(), m3.get_ptr());
            assert_eq!(m3.get_next_neighbor_ptr(), initial_m3_next_ptr);
        }
    }

    #[test]
    fn read_write_work_fine() {
        unsafe {
            stable::clear();
            stable::grow(10).expect("Unable to grow");

            let mut m1 = MemBox::<()>::new(0, 100, true);

            let a = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
            let b = vec![1u8, 3, 3, 7];
            let c = vec![9u8, 8, 7, 6, 5, 4, 3, 2, 1];

            m1._write_bytes(0, &a);
            m1._write_bytes(8, &b);
            m1._write_bytes(90, &c);

            let mut a1 = [0u8; 8];
            let mut b1 = [0u8; 4];
            let mut c1 = [0u8; 9];

            m1._read_bytes(0, &mut a1);
            m1._read_bytes(8, &mut b1);
            m1._read_bytes(90, &mut c1);

            assert_eq!(&a, &a1);
            assert_eq!(&b, &b1);
            assert_eq!(&c, &c1);
        }
    }

    use ic_cdk::export::candid::{CandidType, Deserialize};

    #[derive(CandidType, Deserialize, Debug, PartialEq, Eq, Clone)]
    struct Test {
        pub a: Nat,
        pub b: String,
    }

    #[test]
    fn candid_membox_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();

        let mut tiny_membox = unsafe { MemBox::<Test>::new(0, 20, true) };
        let obj = Test {
            a: Nat::from(12341231231u64),
            b: String::from("The string that sure never fits into 20 bytes"),
        };

        let res = tiny_membox.set(obj.clone()).expect_err("It should fail");
        match res {
            CandidMemBoxError::MemBoxOverflow(encoded_obj) => {
                let mut membox = unsafe { MemBox::<Test>::new(0, encoded_obj.len(), true) };
                membox._write_bytes(0, &encoded_obj);

                let obj1 = membox.get_cloned().unwrap();

                assert_eq!(obj, obj1);
            }
            _ => unreachable!("It should encode just fine"),
        };
    }
}
