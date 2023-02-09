use crate::collections::log::{SLog, Sector};
use crate::encoding::AsFixedSizeBytes;
use crate::mem::allocator::EMPTY_PTR;
use crate::mem::StablePtr;
use crate::primitive::s_ref::SRef;
use crate::primitive::StableType;

struct CurSector {
    ptr: StablePtr,
    len: u64,
    idx: u64,
}

pub struct SLogIter<'a, T: StableType + AsFixedSizeBytes> {
    log: &'a SLog<T>,
    cur_sector: Option<CurSector>,
}

impl<'a, T: StableType + AsFixedSizeBytes> SLogIter<'a, T> {
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

impl<'a, T: StableType + AsFixedSizeBytes> Iterator for SLogIter<'a, T> {
    type Item = SRef<'a, T>;

    fn next(&mut self) -> Option<Self::Item> {
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
        let ptr = sector.get_element_ptr(cur_sector.idx * T::SIZE as u64);

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
