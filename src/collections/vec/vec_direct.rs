use crate::mem::s_slice::{SSlice, Side, PTR_SIZE};
use crate::mem::Anyway;
use crate::utils::math::fast_log2_64;
use crate::utils::phantom_data::SPhantomData;
use crate::utils::{any_as_u8_slice, u8_slice_as_any, NotReference};
use crate::{allocate, deallocate, reallocate};
use speedy::{Readable, Writable};
use std::cmp::min;
use std::mem::size_of;

const TWO_IN_29: u64 = 2u64.pow(29);

#[derive(Readable, Writable)]
pub struct SVecDirect<T> {
    len: u64,
    sectors_len: usize,
    sectors: Option<SSlice>,
    #[speedy(skip)]
    _data: SPhantomData<T>,
}

impl<T: Copy + NotReference> SVecDirect<T> {
    pub const fn new() -> Self {
        Self {
            len: 0,
            sectors_len: 0,
            sectors: None,
            _data: SPhantomData::new(),
        }
    }

    pub fn push(&mut self, element: &T) {
        self.grow_if_needed();
        self.set_len(self.len() + 1);

        let (sector_idx, offset) = self.calculate_inner_index(self.len() - 1);
        let sector_ptr = self.get_or_create_sector(sector_idx);

        let elem_bytes = unsafe { any_as_u8_slice(element) };

        SSlice::_write_bytes(sector_ptr, offset, elem_bytes);
    }

    // TODO: POP IS TOO SLOW (13x slower)
    pub fn pop(&mut self) -> Option<T> {
        let len = self.len();
        if len == 0 {
            return None;
        }

        let idx = len - 1;
        let (sector_idx, offset) = self.calculate_inner_index(idx);
        let sector_ptr = self.get_sector(sector_idx);

        self.set_len(idx);

        let mut elem_bytes = vec![0u8; size_of::<T>()];

        SSlice::_read_bytes(sector_ptr, offset, &mut elem_bytes);

        Some(unsafe { u8_slice_as_any(&elem_bytes) })
    }

    pub fn get_cloned(&self, idx: u64) -> Option<T> {
        if idx >= self.len() || self.is_empty() {
            return None;
        }

        let (sector_idx, offset) = self.calculate_inner_index(idx);
        let sector_ptr = self.get_sector(sector_idx);

        let mut elem_bytes = vec![0u8; size_of::<T>()];
        SSlice::_read_bytes(sector_ptr, offset, &mut elem_bytes);

        Some(unsafe { u8_slice_as_any(&elem_bytes) })
    }

    pub fn replace(&mut self, idx: u64, element: &T) -> T {
        assert!(idx < self.len(), "Out of bounds");

        let (sector_idx, offset) = self.calculate_inner_index(idx);
        let sector_ptr = self.get_sector(sector_idx);

        let mut prev_elem_bytes = vec![0u8; size_of::<T>()];
        SSlice::_read_bytes(sector_ptr, offset, &mut prev_elem_bytes);

        let new_elem_bytes = unsafe { any_as_u8_slice(element) };
        SSlice::_write_bytes(sector_ptr, offset, new_elem_bytes);

        unsafe { u8_slice_as_any(&prev_elem_bytes) }
    }

    pub fn swap(&mut self, idx1: u64, idx2: u64) {
        assert!(idx1 < self.len(), "idx1 out of bounds");
        assert!(idx2 < self.len(), "idx2 out of bounds");
        assert!(idx1 != idx2, "Indices should differ");

        let (sector1_idx, offset1) = self.calculate_inner_index(idx1);
        let sector1_ptr = self.get_sector(sector1_idx);

        let mut elem1_bytes = vec![0u8; size_of::<T>()];
        SSlice::_read_bytes(sector1_ptr, offset1, &mut elem1_bytes);

        let (sector2_idx, offset2) = self.calculate_inner_index(idx2);
        let sector2_ptr = self.get_sector(sector2_idx);

        let mut elem2_bytes = vec![0u8; size_of::<T>()];
        SSlice::_read_bytes(sector2_ptr, offset2, &mut elem2_bytes);

        SSlice::_write_bytes(sector1_ptr, offset1, &elem2_bytes);
        SSlice::_write_bytes(sector2_ptr, offset2, &elem1_bytes);
    }

    pub fn drop(self) {
        for idx in 0..(self.sectors_len / PTR_SIZE) {
            let sector = self.get_sector(idx);

            deallocate(SSlice::from_ptr(sector, Side::Start).unwrap());
        }

        if let Some(sectors) = self.sectors {
            deallocate(sectors);
        }
    }

    pub fn capacity(&self) -> u64 {
        if let Some(sectors_slice) = self.sectors {
            (sectors_slice.size / PTR_SIZE) as u64
        } else {
            0
        }
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn set_len(&mut self, new_len: u64) {
        self.len = new_len;
    }

    fn get_sector(&self, idx: usize) -> u64 {
        assert!(idx < self.sectors_len);

        let sectors = self.sectors.as_ref().unwrap();

        sectors.read_word(idx * PTR_SIZE)
    }

    fn get_or_create_sector(&mut self, idx: usize) -> u64 {
        assert!(idx <= self.sectors_len);

        if idx == self.sectors_len {
            let sectors = self.sectors.as_ref().unwrap();

            let new_sector_size =
                2u64.pow(min(self.sectors_len as u32 + 2, 29)) as usize * size_of::<T>();

            let sector = allocate(new_sector_size);

            sectors.write_word(idx * PTR_SIZE, sector.ptr);

            self.sectors_len += 1;

            sector.ptr
        } else {
            self.get_sector(idx)
        }
    }

    pub fn is_about_to_grow(&self) -> bool {
        self.len() == self.capacity()
    }

    fn grow_if_needed(&mut self) {
        if self.is_about_to_grow() {
            if let Some(sslice) = self.sectors {
                let sslice = reallocate(sslice, sslice.size * 2).anyway();

                self.sectors = Some(sslice)
            } else {
                self.sectors = Some(allocate(4 * PTR_SIZE));
            }
        }
    }

    fn calculate_inner_index(&self, mut idx: u64) -> (usize, usize) {
        assert!(idx < self.len());

        let (sector_idx, offset_ptr) = if idx > TWO_IN_29 - 4 {
            idx -= TWO_IN_29 - 4;
            let sector_idx = 27 + (idx / TWO_IN_29) as usize;
            let offset_ptr = (idx % TWO_IN_29) as usize;

            (sector_idx, offset_ptr)
        } else {
            let sector_idx = fast_log2_64(idx + 4) as usize - 2;
            let ptrs_in_prev_sectors = 2u64.pow(sector_idx as u32 + 2) - 4;

            let offset_ptr = (idx - ptrs_in_prev_sectors) as usize;

            (sector_idx, offset_ptr)
        };

        (sector_idx, offset_ptr * size_of::<T>())
    }
}

impl<T: Copy + NotReference> Default for SVecDirect<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::vec::vec_direct::SVecDirect;
    use crate::init_allocator;
    use crate::utils::mem_context::stable;

    #[derive(Copy, Clone, Debug)]
    struct Test {
        a: u64,
        b: bool,
    }

    #[test]
    fn create_destroy_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut stable_vec = SVecDirect::<u64>::new();
        assert_eq!(stable_vec.capacity(), 0);
        assert_eq!(stable_vec.len(), 0);

        stable_vec.push(&10);
        assert_eq!(stable_vec.capacity(), 4);
        assert_eq!(stable_vec.len(), 1);

        stable_vec.drop();
    }

    #[test]
    fn push_pop_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut stable_vec = SVecDirect::new();
        let count = 10u64;

        for i in 0..count {
            let it = Test { a: i, b: true };

            stable_vec.push(&it);
        }

        assert_eq!(stable_vec.len(), count, "Invalid len after push");

        for i in 0..count {
            let it = Test { a: i, b: false };

            stable_vec.replace(i, &it);
        }

        assert_eq!(stable_vec.len(), count, "Invalid len after push");

        for i in 0..count {
            let it = stable_vec.pop().unwrap();

            assert_eq!(it.a, count - 1 - i);
            assert!(!it.b);
        }

        assert_eq!(stable_vec.len(), 0, "Invalid len after pop");

        for i in 0..count {
            let it = Test { a: i, b: true };

            stable_vec.push(&it);
        }

        stable_vec.drop();
    }

    #[test]
    fn basic_flow_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut v = SVecDirect::<u64>::default();
        assert!(v.get_cloned(100).is_none());

        v.push(&10);
        v.push(&20);

        assert_eq!(v.replace(0, &11), 10);

        v.drop();
    }
}
