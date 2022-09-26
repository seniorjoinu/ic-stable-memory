use crate::mem::allocator::EMPTY_PTR;
use crate::mem::s_slice::{Side, PTR_SIZE};
use crate::mem::Anyway;
use crate::utils::math::fast_log2_64;
use crate::utils::phantom_data::SPhantomData;
use crate::{allocate, deallocate, reallocate, SSlice, SUnsafeCell};
use speedy::{LittleEndian, Readable, Writable};
use std::cmp::min;

const TWO_IN_29: u64 = 2u64.pow(29);

#[derive(Readable, Writable)]
struct SVecInfo {
    _len: u64,
    _sectors: Option<SSlice>,
    _sectors_len: usize,
}

#[derive(Readable, Writable)]
pub struct SVec<T> {
    _info: SVecInfo,
    _data: SPhantomData<T>,
}

impl<'a, T: Readable<'a, LittleEndian> + Writable<LittleEndian>> SVec<T> {
    pub const fn new() -> Self {
        let _info = SVecInfo {
            _len: 0,
            _sectors: None,
            _sectors_len: 0,
        };

        Self {
            _info,
            _data: SPhantomData::default(),
        }
    }

    pub fn push(&mut self, element: &T) {
        let elem_cell = SUnsafeCell::new(element);
        let elem_ptr = unsafe { elem_cell.as_ptr() };

        self.grow_if_needed();
        self.set_len(self.len() + 1);

        let (sector_idx, offset) = self.calculate_inner_index(self.len() - 1);
        let sector = self.get_or_create_sector(sector_idx);

        sector._write_word(offset, elem_ptr);
    }

    pub fn pop(&mut self) -> Option<T> {
        let len = self.len();
        if len == 0 {
            return None;
        }

        let idx = len - 1;
        let (sector_idx, offset) = self.calculate_inner_index(idx);
        let sector = self.get_sector(sector_idx);
        let elem_ptr = sector._read_word(offset);
        self.set_len(idx);

        let elem_cell = unsafe { SUnsafeCell::<T>::from_ptr(elem_ptr) };
        let elem = elem_cell.get_cloned();
        elem_cell.drop();

        Some(elem)
    }

    pub fn get_cloned(&self, idx: u64) -> Option<T> {
        if idx >= self.len() || self.is_empty() {
            return None;
        }

        let (sector_idx, offset) = self.calculate_inner_index(idx);
        let sector = self.get_sector(sector_idx);

        let elem_ptr = sector._read_word(offset);
        let elem_cell = unsafe { SUnsafeCell::<T>::from_ptr(elem_ptr) };
        let elem = elem_cell.get_cloned();

        Some(elem)
    }

    pub fn replace(&mut self, idx: u64, element: &T) -> T {
        assert!(idx < self.len(), "Out of bounds");
        let new_elem_cell = SUnsafeCell::new(element);
        let new_elem_ptr = unsafe { new_elem_cell.as_ptr() };

        let (sector_idx, offset) = self.calculate_inner_index(idx);
        let sector = self.get_sector(sector_idx);

        let prev_elem_ptr = sector._read_word(offset);
        let prev_elem_cell = unsafe { SUnsafeCell::<T>::from_ptr(prev_elem_ptr) };
        let prev_elem = prev_elem_cell.get_cloned();

        sector._write_word(offset, new_elem_ptr);

        prev_elem
    }

    pub fn swap(&mut self, idx1: u64, idx2: u64) {
        assert!(idx1 < self.len(), "idx1 out of bounds");
        assert!(idx2 < self.len(), "idx2 out of bounds");
        assert!(idx1 != idx2, "Indices should differ");

        let (sector1_idx, offset1) = self.calculate_inner_index(idx1);
        let sector1 = self.get_sector(sector1_idx);

        let (sector2_idx, offset2) = self.calculate_inner_index(idx2);
        let sector2 = self.get_sector(sector2_idx);

        let elem_ptr_1 = sector1._read_word(offset1);
        let elem_ptr_2 = sector2._read_word(offset2);

        sector1._write_word(offset1, elem_ptr_2);
        sector2._write_word(offset2, elem_ptr_1);
    }

    pub fn drop(mut self) {
        loop {
            if self.pop().is_none() {
                break;
            }
        }

        for idx in 0..(self._info._sectors_len / PTR_SIZE) {
            let sector = self.get_sector(idx);

            deallocate(sector);
        }
    }

    pub fn capacity(&self) -> u64 {
        if let Some(sectors_slice) = self._info._sectors {
            (sectors_slice.size / PTR_SIZE) as u64
        } else {
            0
        }
    }

    pub fn len(&self) -> u64 {
        self._info._len
    }

    pub fn is_empty(&self) -> bool {
        self._info._len == 0
    }

    fn set_len(&mut self, new_len: u64) {
        self._info._len = new_len;
    }

    fn get_sector(&self, idx: usize) -> SSlice {
        assert!(idx < self._info._sectors_len);

        let sectors = self._info._sectors.as_ref().unwrap();
        let sector_ptr = sectors._read_word(idx * PTR_SIZE);

        SSlice::from_ptr(sector_ptr, Side::Start, false).unwrap()
    }

    fn get_or_create_sector(&mut self, idx: usize) -> SSlice {
        assert!(idx <= self._info._sectors_len);

        if idx == self._info._sectors_len {
            let sectors = self._info._sectors.as_ref().unwrap();

            let new_sector_size =
                2u64.pow(min(self._info._sectors_len as u32 + 2, 29)) as usize * PTR_SIZE;

            let sector = allocate(new_sector_size);

            sectors._write_word(idx * PTR_SIZE, sector.ptr);

            self._info._sectors_len += 1;

            sector
        } else {
            self.get_sector(idx)
        }
    }

    pub fn is_about_to_grow(&self) -> bool {
        self.len() == self.capacity()
    }

    fn grow_if_needed(&mut self) {
        if self.is_about_to_grow() {
            if let Some(sslice) = self._info._sectors {
                let sslice = reallocate(sslice, sslice.size * 2).anyway();

                self._info._sectors = Some(sslice)
            } else {
                self._info._sectors = Some(allocate(4 * PTR_SIZE));
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

        (sector_idx, offset_ptr * PTR_SIZE)
    }
}

impl<'a, T: Readable<'a, LittleEndian> + Writable<LittleEndian>> Default for SVec<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::vec::SVec;
    use crate::utils::mem_context::stable;
    use crate::{init_allocator, set_max_allocation_pages, set_max_grow_pages};
    use speedy::{Readable, Writable};

    #[derive(Readable, Writable, Debug)]
    struct Test {
        a: u64,
        b: String,
    }

    #[test]
    fn create_destroy_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut stable_vec = SVec::<u64>::new();
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

        let mut stable_vec = SVec::new();
        let count = 10u64;

        for i in 0..count {
            let it = Test {
                a: i,
                b: format!("Str {}", i),
            };

            stable_vec.push(&it);
        }

        assert_eq!(stable_vec.len(), count, "Invalid len after push");

        for i in 0..count {
            let it = Test {
                a: i,
                b: format!("String of the element {}", i),
            };

            stable_vec.replace(i, &it);
        }

        assert_eq!(stable_vec.len(), count, "Invalid len after push");

        for i in 0..count {
            let it = stable_vec.pop().unwrap();

            assert_eq!(it.a, count - 1 - i);
            assert_eq!(it.b, format!("String of the element {}", count - 1 - i));
        }

        assert_eq!(stable_vec.len(), 0, "Invalid len after pop");

        for i in 0..count {
            let it = Test {
                a: i,
                b: format!("Str {}", i),
            };

            stable_vec.push(&it);
        }

        stable_vec.drop();
    }

    #[test]
    fn basic_flow_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut v = SVec::<u64>::default();
        assert!(v.get_cloned(100).is_none());

        v.push(&10);
        v.push(&20);

        assert_eq!(v.replace(0, &11), 10);

        v.drop();
    }
}
