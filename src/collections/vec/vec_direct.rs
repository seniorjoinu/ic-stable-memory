use crate::mem::s_slice::{SSlice, Side, PTR_SIZE};
use crate::mem::Anyway;
use crate::utils::math::fast_log2_32;
use crate::utils::phantom_data::SPhantomData;
use crate::utils::{any_as_u8_slice, u8_fixed_array_as_any, NotReference};
use crate::{allocate, deallocate, reallocate};
use speedy::{Readable, Writable};
use std::cmp::min;
use std::mem::size_of;

#[derive(Readable, Writable)]
pub struct SVecDirect<T> {
    len: u64,
    sectors_len: usize,
    sectors: Option<SSlice>,
    #[speedy(skip)]
    _sectors_cache: Vec<u64>,
    #[speedy(skip)]
    _data: SPhantomData<T>,
}

impl<T: Copy + NotReference> SVecDirect<T>
where
    [(); size_of::<T>()]: Sized,
{
    pub fn new() -> Self {
        Self {
            len: 0,
            sectors_len: 0,
            sectors: None,
            _sectors_cache: Vec::new(),
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

    pub fn pop(&mut self) -> Option<T> {
        let len = self.len();
        if len == 0 {
            return None;
        }

        let idx = len - 1;
        let (sector_idx, offset) = self.calculate_inner_index(idx);
        let sector_ptr = self.get_sector(sector_idx);

        self.set_len(idx);

        let mut arr = [0u8; size_of::<T>()];
        SSlice::_read_bytes(sector_ptr, offset, &mut arr);

        Some(unsafe { u8_fixed_array_as_any(arr) })
    }

    pub fn get_cloned(&self, idx: u64) -> Option<T> {
        if idx >= self.len() || self.is_empty() {
            return None;
        }

        let (sector_idx, offset) = self.calculate_inner_index(idx);
        let sector_ptr = self.get_sector(sector_idx);

        let mut arr = [0u8; size_of::<T>()];
        SSlice::_read_bytes(sector_ptr, offset, &mut arr);

        Some(unsafe { u8_fixed_array_as_any(arr) })
    }

    pub fn replace(&mut self, idx: u64, element: &T) -> T {
        assert!(idx < self.len(), "Out of bounds");

        let (sector_idx, offset) = self.calculate_inner_index(idx);
        let sector_ptr = self.get_sector(sector_idx);

        let mut arr = [0u8; size_of::<T>()];
        SSlice::_read_bytes(sector_ptr, offset, &mut arr);

        let new_elem_bytes = unsafe { any_as_u8_slice(element) };
        SSlice::_write_bytes(sector_ptr, offset, new_elem_bytes);

        unsafe { u8_fixed_array_as_any(arr) }
    }

    pub fn swap(&mut self, idx1: u64, idx2: u64) {
        assert!(idx1 < self.len(), "idx1 out of bounds");
        assert!(idx2 < self.len(), "idx2 out of bounds");
        assert!(idx1 != idx2, "Indices should differ");

        let (sector1_idx, offset1) = self.calculate_inner_index(idx1);
        let sector1_ptr = self.get_sector(sector1_idx);
        let mut arr1 = [0u8; size_of::<T>()];

        let (sector2_idx, offset2) = self.calculate_inner_index(idx2);
        let sector2_ptr = self.get_sector(sector2_idx);
        let mut arr2 = [0u8; size_of::<T>()];

        SSlice::_read_bytes(sector1_ptr, offset1, &mut arr1);
        SSlice::_read_bytes(sector2_ptr, offset2, &mut arr2);

        SSlice::_write_bytes(sector1_ptr, offset1, &arr2);
        SSlice::_write_bytes(sector2_ptr, offset2, &arr1);
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

    pub fn is_about_to_grow(&self) -> bool {
        self.len() == self.capacity()
    }

    pub fn recache_sectors(&mut self) {
        if let Some(sectors) = self.sectors {
            self._sectors_cache = Vec::new();

            for i in 0..self.sectors_len {
                self._sectors_cache
                    .push(SSlice::_read_word(sectors.ptr, i * PTR_SIZE));
            }
        }
    }

    fn set_len(&mut self, new_len: u64) {
        self.len = new_len;
    }

    fn get_sector(&self, idx: usize) -> u64 {
        if idx < self._sectors_cache.len() {
            self._sectors_cache[idx]
        } else {
            let sectors = self.sectors.as_ref().unwrap();

            sectors.read_word(idx * PTR_SIZE)
        }
    }

    fn get_or_create_sector(&mut self, idx: usize) -> u64 {
        if idx == self.sectors_len {
            let sectors = self.sectors.as_ref().unwrap();

            let new_sector_size =
                2u64.pow(min(self.sectors_len as u32 + 2, 29)) as usize * size_of::<T>();

            let sector = allocate(new_sector_size);

            sectors.write_word(idx * PTR_SIZE, sector.ptr);

            if idx == self._sectors_cache.len() {
                self._sectors_cache.push(sector.ptr);
            }

            self.sectors_len += 1;

            sector.ptr
        } else {
            self.get_sector(idx)
        }
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
        if idx < 4 {
            return (0, idx as usize * size_of::<T>());
        }

        let (sector_idx, offset_ptr) = if idx > TWO_IN_29_MINUS_4 {
            idx -= TWO_IN_29_MINUS_4;
            let sector_idx = 27 + (idx / TWO_IN_29) as usize;
            let offset_ptr = (idx % TWO_IN_29) as usize;

            (sector_idx, offset_ptr)
        } else {
            let sector_idx = fast_log2_32(idx as u32 + 4) as usize - 2;
            let ptrs_in_prev_sectors = TWOS[sector_idx];

            let offset_ptr = (idx - ptrs_in_prev_sectors) as usize;

            (sector_idx, offset_ptr)
        };

        (sector_idx, offset_ptr * size_of::<T>())
    }
}

const TWO_IN_2: u64 = 0;
const TWO_IN_3: u64 = 8 - 4;
const TWO_IN_4: u64 = 16 - 4;
const TWO_IN_5: u64 = 32 - 4;
const TWO_IN_6: u64 = 64 - 4;
const TWO_IN_7: u64 = 128 - 4;
const TWO_IN_8: u64 = 256 - 4;
const TWO_IN_9: u64 = 512 - 4;
const TWO_IN_10: u64 = 1024 - 4;
const TWO_IN_11: u64 = 2048 - 4;
const TWO_IN_12: u64 = 4096 - 4;
const TWO_IN_13: u64 = 2u64.pow(13) - 4;
const TWO_IN_14: u64 = 2u64.pow(14) - 4;
const TWO_IN_15: u64 = 2u64.pow(15) - 4;
const TWO_IN_16: u64 = 2u64.pow(16) - 4;
const TWO_IN_17: u64 = 2u64.pow(17) - 4;
const TWO_IN_18: u64 = 2u64.pow(18) - 4;
const TWO_IN_19: u64 = 2u64.pow(19) - 4;
const TWO_IN_20: u64 = 2u64.pow(20) - 4;
const TWO_IN_21: u64 = 2u64.pow(21) - 4;
const TWO_IN_22: u64 = 2u64.pow(22) - 4;
const TWO_IN_23: u64 = 2u64.pow(23) - 4;
const TWO_IN_24: u64 = 2u64.pow(24) - 4;
const TWO_IN_25: u64 = 2u64.pow(25) - 4;
const TWO_IN_26: u64 = 2u64.pow(26) - 4;
const TWO_IN_27: u64 = 2u64.pow(27) - 4;
const TWO_IN_28: u64 = 2u64.pow(28) - 4;

const TWO_IN_29: u64 = 2u64.pow(29);
const TWO_IN_29_MINUS_4: u64 = TWO_IN_29 - 4;

const TWOS: [u64; 27] = [
    TWO_IN_2, TWO_IN_3, TWO_IN_4, TWO_IN_5, TWO_IN_6, TWO_IN_7, TWO_IN_8, TWO_IN_9, TWO_IN_10,
    TWO_IN_11, TWO_IN_12, TWO_IN_13, TWO_IN_14, TWO_IN_15, TWO_IN_16, TWO_IN_17, TWO_IN_18,
    TWO_IN_19, TWO_IN_20, TWO_IN_21, TWO_IN_22, TWO_IN_23, TWO_IN_24, TWO_IN_25, TWO_IN_26,
    TWO_IN_27, TWO_IN_28,
];

impl<T: Copy + NotReference> Default for SVecDirect<T>
where
    [(); size_of::<T>()]: Sized,
{
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

        assert_eq!(v.get_cloned(0).unwrap(), 10);
        assert_eq!(v.get_cloned(1).unwrap(), 20);
        assert_eq!(v.replace(0, &11), 10);

        v.drop();
    }
}
