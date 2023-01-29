use crate::collections::log::{SLog, Sector, DEFAULT_CAPACITY};
use crate::mem::allocator::EMPTY_PTR;
use crate::primitive::s_ref::SRef;
use crate::primitive::s_ref_mut::SRefMut;
use crate::primitive::StableAllocated;

struct CurSector {
    ptr: u64,
    len: usize,
    idx: usize,
}

pub struct SLogIter<'a, T> {
    log: &'a SLog<T>,
    cur_sector: Option<CurSector>,
}

impl<'a, T> SLogIter<'a, T> {
    pub(crate) fn new(log: &'a SLog<T>) -> Self {
        Self {
            log,
            cur_sector: None,
        }
    }

    fn get_cur_sector_mut(&mut self) -> &mut CurSector {
        self.cur_sector.as_mut().unwrap()
    }

    // len should be > 0
    fn init_from_front(&mut self) {
        if self.cur_sector.is_some() {
            return;
        }

        let cur_sector_len = if self.log.first_sector_ptr == self.log.cur_sector_ptr {
            self.log.cur_sector_len
        } else {
            DEFAULT_CAPACITY * 2
        };

        self.cur_sector = Some(CurSector {
            ptr: self.log.first_sector_ptr,
            len: cur_sector_len,
            idx: 0,
        });
    }

    // len should be > 0
    fn init_from_back(&mut self) {
        if self.cur_sector.is_some() {
            return;
        }

        self.cur_sector = Some(CurSector {
            ptr: self.log.cur_sector_ptr,
            len: self.log.cur_sector_len,
            idx: self.log.cur_sector_len - 1,
        });
    }
}

impl<'a, T: StableAllocated> Iterator for SLogIter<'a, T>
where
    [(); T::SIZE]: Sized,
{
    type Item = SRef<'a, T>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.log.is_empty() {
            return None;
        }

        self.init_from_front();

        let p = self.log.cur_sector_ptr;
        let l = self.log.cur_sector_len;

        let cur_sector = self.get_cur_sector_mut();

        if cur_sector.ptr == EMPTY_PTR {
            return None;
        }

        let sector = Sector::<T>::from_ptr(cur_sector.ptr);
        let ptr = sector.get_element_ptr(cur_sector.idx * T::SIZE);

        cur_sector.idx += 1;

        if cur_sector.idx == cur_sector.len {
            cur_sector.ptr = sector.read_next_ptr();
            cur_sector.len = if cur_sector.ptr == p {
                l
            } else {
                cur_sector.len * 2
            };
            cur_sector.idx = 0;
        }

        Some(SRef::new(ptr))
    }
}

impl<'a, T: StableAllocated> DoubleEndedIterator for SLogIter<'a, T>
where
    [(); T::SIZE]: Sized,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.log.is_empty() {
            return None;
        }

        self.init_from_back();

        let p = self.log.cur_sector_ptr;
        let c = self.log.cur_sector_capacity;

        let cur_sector = self.get_cur_sector_mut();

        if cur_sector.ptr == EMPTY_PTR {
            return None;
        }

        let sector = Sector::<T>::from_ptr(cur_sector.ptr);
        let ptr = sector.get_element_ptr(cur_sector.idx * T::SIZE);

        if cur_sector.idx == 0 {
            cur_sector.len = if cur_sector.ptr == p {
                c / 2
            } else {
                cur_sector.len / 2
            };
            cur_sector.ptr = sector.read_prev_ptr();
            cur_sector.idx = cur_sector.len - 1;
        } else {
            cur_sector.idx -= 1;
        }

        Some(SRef::new(ptr))
    }
}

pub struct SLogIterMut<'a, T> {
    log: &'a mut SLog<T>,
    cur_sector: Option<CurSector>,
}

impl<'a, T> SLogIterMut<'a, T> {
    pub(crate) fn new(log: &'a mut SLog<T>) -> Self {
        Self {
            log,
            cur_sector: None,
        }
    }

    fn get_cur_sector_mut(&mut self) -> &mut CurSector {
        self.cur_sector.as_mut().unwrap()
    }

    // len should be > 0
    fn init_from_front(&mut self) {
        if self.cur_sector.is_some() {
            return;
        }

        let cur_sector_len = if self.log.first_sector_ptr == self.log.cur_sector_ptr {
            self.log.cur_sector_len
        } else {
            DEFAULT_CAPACITY * 2
        };

        self.cur_sector = Some(CurSector {
            ptr: self.log.first_sector_ptr,
            len: cur_sector_len,
            idx: 0,
        });
    }

    // len should be > 0
    fn init_from_back(&mut self) {
        if self.cur_sector.is_some() {
            return;
        }

        self.cur_sector = Some(CurSector {
            ptr: self.log.cur_sector_ptr,
            len: self.log.cur_sector_len,
            idx: self.log.cur_sector_len - 1,
        });
    }
}

impl<'a, T: StableAllocated> Iterator for SLogIterMut<'a, T>
    where
        [(); T::SIZE]: Sized,
{
    type Item = SRefMut<'a, T>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.log.is_empty() {
            return None;
        }

        self.init_from_front();

        let p = self.log.cur_sector_ptr;
        let l = self.log.cur_sector_len;

        let cur_sector = self.get_cur_sector_mut();

        if cur_sector.ptr == EMPTY_PTR {
            return None;
        }

        let sector = Sector::<T>::from_ptr(cur_sector.ptr);
        let ptr = sector.get_element_ptr(cur_sector.idx * T::SIZE); 

        cur_sector.idx += 1;

        if cur_sector.idx == cur_sector.len {
            cur_sector.ptr = sector.read_next_ptr();
            cur_sector.len = if cur_sector.ptr == p {
                l
            } else {
                cur_sector.len * 2
            };
            cur_sector.idx = 0;
        }

        Some(SRefMut::new(ptr))
    }
}

impl<'a, T: StableAllocated> DoubleEndedIterator for SLogIterMut<'a, T>
    where
        [(); T::SIZE]: Sized,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.log.is_empty() {
            return None;
        }

        self.init_from_back();

        let p = self.log.cur_sector_ptr;
        let c = self.log.cur_sector_capacity;

        let cur_sector = self.get_cur_sector_mut();

        if cur_sector.ptr == EMPTY_PTR {
            return None;
        }

        let sector = Sector::<T>::from_ptr(cur_sector.ptr);
        let ptr = sector.get_element_ptr(cur_sector.idx * T::SIZE);

        if cur_sector.idx == 0 {
            cur_sector.len = if cur_sector.ptr == p {
                c / 2
            } else {
                cur_sector.len / 2
            };
            cur_sector.ptr = sector.read_prev_ptr();
            cur_sector.idx = cur_sector.len - 1;
        } else {
            cur_sector.idx -= 1;
        }

        Some(SRefMut::new(ptr))
    }
}
