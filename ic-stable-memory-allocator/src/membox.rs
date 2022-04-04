use crate::mem_context::stable;
use std::mem::size_of;

pub type Word = u64;
pub type Size = u32;

pub const ALLOCATED: Size = 2u32.pow(Size::BITS - 1); // first biggest bit set to 1, other set to 0
pub const FREE: Size = 2u32.pow(Size::BITS - 1) - 1; // first biggest bit set to 0, other set to 1
pub const MEM_BOX_META_SIZE: Size = size_of::<Size>() as Size;

pub enum Side {
    Start,
    End,
}

#[derive(Debug, Clone, Copy)]
pub struct MemBox(Word);

impl MemBox {
    /// # Safety
    /// Make sure there are no duplicates of this `MemBox`, before creating.
    pub unsafe fn new(ptr: Word, size: Size, allocated: bool) -> Self {
        assert_ne!(size, 0, "Zero size is forbidden");

        Self::write_meta(ptr, size, allocated);

        Self(ptr)
    }

    /// # Safety
    /// This method may create a duplicate of the same unredlying memory slice. Make sure, your logic
    /// doesn't do that.
    pub unsafe fn from_ptr(mut ptr: Word, side: Side) -> Option<Self> {
        let (size_1, size_2) = match side {
            Side::Start => {
                let (size_1, _) = Self::read_meta(ptr);
                if size_1 == 0 {
                    return None;
                }

                let (size_2, _) = Self::read_meta(ptr + MEM_BOX_META_SIZE as Word + size_1 as Word);

                (size_1, size_2)
            }
            Side::End => {
                ptr -= MEM_BOX_META_SIZE as Word;
                let (size_2, _) = Self::read_meta(ptr);
                if size_2 == 0 {
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
            Some(Self(ptr))
        }
    }

    pub fn get_ptr(&self) -> Word {
        self.0
    }

    pub fn get_meta(&self) -> (Size, bool) {
        Self::read_meta(self.0)
    }

    pub(crate) fn set_allocated(&mut self, allocated: bool) {
        let (size, _) = self.get_meta();
        Self::write_meta(self.get_ptr(), size, allocated);
    }

    pub fn write(&mut self, offset: Size, data: &[u8]) {
        let (size, allocated) = self.get_meta();
        assert!(allocated, "Unable to write to free a MemBox");

        assert!(
            offset + data.len() as Size <= size,
            "MemBox overflow (max {}, provided {})",
            size,
            offset + data.len() as Size
        );

        stable::write(
            self.get_ptr() + MEM_BOX_META_SIZE as Word + offset as Word,
            data,
        );
    }

    pub fn read(&self, offset: Size, data: &mut [u8]) {
        let (size, allocated) = self.get_meta();
        assert!(allocated, "Unable to read data from a free MemBox");
        assert!(
            data.len() as Size + offset <= size,
            "MemBox overflow (max {}, provided {})",
            size,
            data.len() as Size + offset
        );

        stable::read(
            self.get_ptr() + MEM_BOX_META_SIZE as Word + offset as Word,
            data,
        );
    }

    /// Splits this free `MemBox` into two new ones, if possible. The first one will have the provided size, the second
    /// one will have the rest (but not less than `min_size_second`. If size is not enough, returns
    /// `Err(self)`. Both new `MemBox`-es are free.
    ///
    /// # Safety
    /// Make sure there are no duplicates of this `MemBox` left before splitting.
    pub unsafe fn split(
        self,
        size_first: Size,
        min_size_second: Size,
    ) -> Result<(Self, Self), Self> {
        let (size, allocated) = self.get_meta();
        assert!(!allocated, "Unable to split an allocated MemBox");

        if size < size_first + min_size_second + MEM_BOX_META_SIZE * 2 {
            return Err(self);
        }

        let size_second = size - size_first - MEM_BOX_META_SIZE * 2;
        let ptr_second = self.get_ptr() + (MEM_BOX_META_SIZE * 2 + size_first) as Word;

        let first = Self::new(self.get_ptr(), size_first, false);
        let second = Self::new(ptr_second, size_second, false);

        Ok((first, second))
    }

    /// # Safety
    /// Make sure this MemBox and its neighbor are both have no duplicates, before merging.
    pub unsafe fn merge_with_neighbor(self, neighbor: Self) -> Self {
        let (self_size, self_allocated) = self.get_meta();
        assert!(!self_allocated, "Unable to merge allocated MemBox");

        let (neighbor_size, neighbor_allocated) = neighbor.get_meta();
        assert!(!neighbor_allocated, "Unable to merge with allocated MemBox");

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
    pub unsafe fn get_neighbor(&self, side: Side) -> Option<Self> {
        match side {
            Side::Start => Self::from_ptr(self.get_ptr(), Side::End),
            Side::End => Self::from_ptr(self.get_next_neighbor_ptr(), Side::Start),
        }
    }

    pub fn get_next_neighbor_ptr(&self) -> Word {
        self.get_ptr() + (MEM_BOX_META_SIZE * 2 + self.get_meta().0) as Word
    }

    fn read_meta(ptr: Word) -> (Size, bool) {
        let mut meta = [0u8; MEM_BOX_META_SIZE as usize];
        stable::read(ptr, &mut meta);

        let mut size = Size::from_le_bytes(meta);
        let allocated = if size & ALLOCATED == ALLOCATED {
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
        stable::write(ptr + MEM_BOX_META_SIZE as Word + size as Word, &meta);
    }
}

/// Only run these tests with `-- --test-threads=1`. It fails otherwise.
#[cfg(test)]
mod tests {
    use crate::mem_context::stable;
    use crate::membox::{MemBox, Side, Size, Word, MEM_BOX_META_SIZE};
    use std::mem::size_of;

    #[test]
    fn creation_works_fine() {
        unsafe {
            stable::clear();
            stable::grow(10).expect("Unable to grow");

            let m1_size: Size = 100;
            let m2_size: Size = 200;
            let m3_size: Size = 300;

            let m1 = MemBox::new(0, m1_size, false);
            assert_eq!(m1.get_meta(), (m1_size, false));
            assert_eq!(
                m1.get_next_neighbor_ptr(),
                (0 + m1_size + MEM_BOX_META_SIZE * 2) as Word
            );

            let m2 = MemBox::new(m1.get_next_neighbor_ptr(), m2_size, true);
            assert_eq!(m2.get_meta(), (m2_size, true));
            assert_eq!(
                m2.get_next_neighbor_ptr(),
                m1.get_next_neighbor_ptr() + (m2_size + MEM_BOX_META_SIZE * 2) as Word
            );

            let m3 = MemBox::new(m2.get_next_neighbor_ptr(), m3_size, false);
            assert_eq!(m3.get_meta(), (m3_size, false));
            assert_eq!(
                m3.get_next_neighbor_ptr(),
                m2.get_next_neighbor_ptr() + (m3_size + MEM_BOX_META_SIZE * 2) as Word
            );

            let m1 = MemBox::from_ptr(0, Side::Start).unwrap();
            assert_eq!(m1.get_meta(), (m1_size, false));
            assert_eq!(
                m1.get_next_neighbor_ptr(),
                0 + (m1_size + MEM_BOX_META_SIZE * 2) as Word
            );

            let m1 = MemBox::from_ptr(m1.get_next_neighbor_ptr(), Side::End).unwrap();
            assert_eq!(m1.get_meta(), (m1_size, false));
            assert_eq!(
                m1.get_next_neighbor_ptr(),
                0 + (m1_size + MEM_BOX_META_SIZE * 2) as Word
            );

            let m2 = MemBox::from_ptr(m1.get_next_neighbor_ptr(), Side::Start).unwrap();
            assert_eq!(m2.get_meta(), (m2_size, true));
            assert_eq!(
                m2.get_next_neighbor_ptr(),
                m1.get_next_neighbor_ptr() + (m2_size + MEM_BOX_META_SIZE * 2) as Word
            );

            let m2 = MemBox::from_ptr(m2.get_next_neighbor_ptr(), Side::End).unwrap();
            assert_eq!(m2.get_meta(), (m2_size, true));
            assert_eq!(
                m2.get_next_neighbor_ptr(),
                m1.get_next_neighbor_ptr() + (m2_size + MEM_BOX_META_SIZE * 2) as Word
            );

            let m3 = MemBox::from_ptr(m2.get_next_neighbor_ptr(), Side::Start).unwrap();
            assert_eq!(m3.get_meta(), (m3_size, false));
            assert_eq!(
                m3.get_next_neighbor_ptr(),
                m2.get_next_neighbor_ptr() + (m3_size + MEM_BOX_META_SIZE * 2) as Word
            );

            let m3 = MemBox::from_ptr(m3.get_next_neighbor_ptr(), Side::End).unwrap();
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

            let m1 = MemBox::new(0, m1_size, false);
            let m2 = MemBox::new(m1.get_next_neighbor_ptr(), m2_size, false);
            let m3 = MemBox::new(m2.get_next_neighbor_ptr(), m3_size, false);

            let initial_m3_next_ptr = m3.get_next_neighbor_ptr();

            let (m3, m4) = m3
                .split(100, size_of::<Size>() as Size)
                .expect("Unable to split m3");
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

            let (m1, m2) = m1.split(m1_size, 0).expect("Unable to split m1");
            assert_eq!(m1.get_meta(), (m1_size, false));
            assert_eq!(
                m2.get_meta(),
                (m2_size + m3_size + 2 * MEM_BOX_META_SIZE, false)
            );
            assert_eq!(m1.get_next_neighbor_ptr(), m2.get_ptr());
            assert_eq!(m2.get_next_neighbor_ptr(), initial_m3_next_ptr);

            let (m2, m3) = m2.split(m2_size, 0).expect("Unable to split m2");
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

            let mut m1 = MemBox::new(0, 100, true);

            let a = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
            let b = vec![1u8, 3, 3, 7];
            let c = vec![9u8, 8, 7, 6, 5, 4, 3, 2, 1];

            m1.write(0, &a);
            m1.write(8, &b);
            m1.write(90, &c);

            let mut a1 = [0u8; 8];
            let mut b1 = [0u8; 4];
            let mut c1 = [0u8; 9];

            m1.read(0, &mut a1);
            m1.read(8, &mut b1);
            m1.read(90, &mut c1);

            assert_eq!(&a, &a1);
            assert_eq!(&b, &b1);
            assert_eq!(&c, &c1);
        }
    }
}
