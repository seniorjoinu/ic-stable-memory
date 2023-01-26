use crate::collections::log::iter::SLogIter;
use crate::mem::allocator::EMPTY_PTR;
use crate::mem::s_slice::Side;
use crate::primitive::StableAllocated;
use crate::utils::encoding::{AsDynSizeBytes, AsFixedSizeBytes, FixedSize};
use crate::{allocate, deallocate, SSlice};
use std::fmt::Debug;
use std::marker::PhantomData;

pub mod iter;

pub(crate) const DEFAULT_CAPACITY: usize = 2;

pub struct SLog<T> {
    pub(crate) len: u64,
    pub(crate) first_sector_ptr: u64,
    pub(crate) cur_sector_ptr: u64,
    pub(crate) cur_sector_last_item_offset: usize,
    cur_sector_capacity: usize,
    pub(crate) cur_sector_len: usize,
    _marker: PhantomData<T>,
}

impl<T> SLog<T> {
    pub fn new() -> Self {
        Self {
            len: 0,
            first_sector_ptr: EMPTY_PTR,
            cur_sector_ptr: EMPTY_PTR,
            cur_sector_last_item_offset: 0,
            cur_sector_capacity: DEFAULT_CAPACITY,
            cur_sector_len: 0,
            _marker: PhantomData::default(),
        }
    }
}

impl<T> Default for SLog<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: StableAllocated> SLog<T>
where
    [(); T::SIZE]: Sized,
{
    #[inline]
    const fn max_capacity() -> usize {
        2usize.pow(31) / T::SIZE
    }

    pub fn push(&mut self, mut it: T) {
        let mut sector = self.get_or_create_current_sector();
        self.move_to_next_sector_if_needed(&mut sector);

        it.move_to_stable();
        sector.write_element(self.cur_sector_last_item_offset, it);
        self.cur_sector_last_item_offset += T::SIZE;
        self.cur_sector_len += 1;
        self.len += 1;
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }

        let sector = self.get_current_sector()?;

        self.cur_sector_last_item_offset -= T::SIZE;
        self.cur_sector_len -= 1;
        self.len -= 1;

        let mut it = sector.read_element(self.cur_sector_last_item_offset);
        it.remove_from_stable();

        self.move_to_prev_sector_if_needed(sector);

        Some(it)
    }

    pub fn last(&self) -> Option<T> {
        if self.len == 0 {
            return None;
        }

        let sector = self.get_current_sector()?;
        let it = sector.read_element(self.cur_sector_last_item_offset - T::SIZE);

        Some(it)
    }

    pub fn first(&self) -> Option<T> {
        if self.len == 0 {
            return None;
        }

        let sector = self.get_first_sector()?;
        let it = sector.read_element(0);

        Some(it)
    }

    pub fn get_copy(&self, idx: u64) -> Option<T> {
        if idx >= self.len || self.len == 0 {
            return None;
        }

        let mut sector = Sector::<T>::from_ptr(self.cur_sector_ptr);
        let mut sector_len = self.cur_sector_len;

        let mut len = self.len;

        loop {
            len -= sector_len as u64;
            if len <= idx {
                break;
            }

            sector_len = if sector.as_ptr() == self.cur_sector_ptr {
                self.cur_sector_capacity / 2
            } else {
                sector_len / 2
            };
            sector = Sector::<T>::from_ptr(sector.read_prev_ptr());
        }

        Some(sector.read_element((idx - len) as usize * T::SIZE))
    }

    #[inline]
    pub fn len(&self) -> u64 {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    pub fn iter(&self) -> SLogIter<'_, T> {
        SLogIter::new(self)
    }

    fn get_or_create_current_sector(&mut self) -> Sector<T> {
        if self.cur_sector_ptr == EMPTY_PTR {
            self.cur_sector_capacity *= 2;

            let it = Sector::<T>::new(self.cur_sector_capacity, EMPTY_PTR);

            self.first_sector_ptr = it.as_ptr();
            self.cur_sector_ptr = it.as_ptr();

            it
        } else {
            Sector::<T>::from_ptr(self.cur_sector_ptr)
        }
    }

    #[inline]
    fn get_current_sector(&self) -> Option<Sector<T>> {
        if self.cur_sector_ptr == EMPTY_PTR {
            None
        } else {
            Some(Sector::<T>::from_ptr(self.cur_sector_ptr))
        }
    }

    #[inline]
    fn get_first_sector(&self) -> Option<Sector<T>> {
        if self.first_sector_ptr == EMPTY_PTR {
            None
        } else {
            Some(Sector::<T>::from_ptr(self.first_sector_ptr))
        }
    }

    fn move_to_prev_sector_if_needed(&mut self, sector: Sector<T>) {
        if self.cur_sector_len > 0 {
            return;
        }

        let prev_sector_ptr = sector.read_prev_ptr();
        if prev_sector_ptr == EMPTY_PTR {
            return;
        }

        self.cur_sector_capacity /= 2;
        self.cur_sector_len = self.cur_sector_capacity;
        self.cur_sector_ptr = prev_sector_ptr;
        self.cur_sector_last_item_offset = self.cur_sector_capacity * T::SIZE;
    }

    fn move_to_next_sector_if_needed(&mut self, sector: &mut Sector<T>) {
        if self.cur_sector_len != self.cur_sector_capacity {
            return;
        }

        let next_sector_ptr = sector.read_next_ptr();

        if self.cur_sector_capacity < Self::max_capacity() {
            self.cur_sector_capacity *= 2;
        }

        let next_sector = if next_sector_ptr == EMPTY_PTR {
            let mut new_sector = Sector::<T>::new(self.cur_sector_capacity, sector.as_ptr());
            sector.write_next_ptr(new_sector.as_ptr());
            new_sector.write_prev_ptr(sector.as_ptr());

            new_sector
        } else {
            Sector::<T>::from_ptr(next_sector_ptr)
        };

        self.cur_sector_ptr = next_sector.as_ptr();
        self.cur_sector_len = 0;
        self.cur_sector_last_item_offset = 0;

        *sector = next_sector;
    }
}

const PREV_OFFSET: usize = 0;
const NEXT_OFFSET: usize = PREV_OFFSET + u64::SIZE;
const ELEMENTS_OFFSET: usize = NEXT_OFFSET + u64::SIZE;

struct Sector<T>(u64, PhantomData<T>);

impl<T: StableAllocated> Sector<T>
where
    [(); T::SIZE]: Sized,
{
    fn new(cap: usize, prev: u64) -> Self {
        let slice = allocate(u64::SIZE * 2 + cap * T::SIZE);

        let mut it = Self(slice.get_ptr(), PhantomData::default());
        it.write_prev_ptr(prev);
        it.write_next_ptr(EMPTY_PTR);

        it
    }

    fn destroy(self) {
        let slice = SSlice::from_ptr(self.0, Side::Start).unwrap();
        deallocate(slice);
    }

    #[inline]
    fn as_ptr(&self) -> u64 {
        self.0
    }

    #[inline]
    fn from_ptr(ptr: u64) -> Self {
        Self(ptr, PhantomData::default())
    }

    #[inline]
    fn read_prev_ptr(&self) -> u64 {
        SSlice::_as_fixed_size_bytes_read::<u64>(self.0, PREV_OFFSET)
    }

    #[inline]
    fn write_prev_ptr(&mut self, ptr: u64) {
        SSlice::_as_fixed_size_bytes_write::<u64>(self.0, PREV_OFFSET, ptr)
    }

    #[inline]
    fn read_next_ptr(&self) -> u64 {
        SSlice::_as_fixed_size_bytes_read::<u64>(self.0, NEXT_OFFSET)
    }

    #[inline]
    fn write_next_ptr(&mut self, ptr: u64) {
        SSlice::_as_fixed_size_bytes_write::<u64>(self.0, NEXT_OFFSET, ptr)
    }

    #[inline]
    fn read_element(&self, offset: usize) -> T {
        SSlice::_as_fixed_size_bytes_read::<T>(self.0, ELEMENTS_OFFSET + offset)
    }

    #[inline]
    fn write_element(&self, offset: usize, element: T) {
        SSlice::_as_fixed_size_bytes_write::<T>(self.0, ELEMENTS_OFFSET + offset, element)
    }
}

impl<T: StableAllocated + Debug> SLog<T>
where
    [(); T::SIZE]: Sized,
{
    pub fn debug_print(&self) {
        let mut sector = if let Some(s) = self.get_first_sector() {
            s
        } else {
            println!("SLog []");
            return;
        };

        let mut current_sector_len = DEFAULT_CAPACITY * 2;

        print!(
            "SLog({}, {}, {}, {}, {}, {})",
            self.len,
            self.first_sector_ptr,
            self.cur_sector_ptr,
            self.cur_sector_len,
            self.cur_sector_capacity,
            self.cur_sector_last_item_offset
        );

        print!(" [");

        loop {
            print!("[");
            let len = if sector.as_ptr() == self.cur_sector_ptr {
                self.cur_sector_len
            } else {
                current_sector_len
            };

            let mut offset = 0;
            for i in 0..len {
                let elem = sector.read_element(offset);
                offset += T::SIZE;

                print!("{:?}", elem);
                if i < len - 1 {
                    print!(", ");
                }
            }
            print!("]");

            if sector.as_ptr() == self.cur_sector_ptr {
                break;
            }

            print!(", ");

            let next_sector_ptr = sector.read_next_ptr();
            assert_ne!(next_sector_ptr, EMPTY_PTR);

            sector = Sector::<T>::from_ptr(next_sector_ptr);
            current_sector_len *= 2;
        }

        println!("]");
    }
}

impl<T> FixedSize for SLog<T> {
    const SIZE: usize = usize::SIZE * 3 + u64::SIZE * 3;
}

impl<T> AsFixedSizeBytes for SLog<T> {
    fn as_fixed_size_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = Self::_u8_arr_of_size();

        buf[0..u64::SIZE].copy_from_slice(&self.len.as_fixed_size_bytes());
        buf[u64::SIZE..(u64::SIZE * 2)]
            .copy_from_slice(&self.first_sector_ptr.as_fixed_size_bytes());
        buf[(u64::SIZE * 2)..(u64::SIZE * 3)]
            .copy_from_slice(&self.cur_sector_ptr.as_fixed_size_bytes());
        buf[(u64::SIZE * 3)..(u64::SIZE * 3 + usize::SIZE)]
            .copy_from_slice(&self.cur_sector_last_item_offset.as_fixed_size_bytes());
        buf[(u64::SIZE * 3 + usize::SIZE)..(u64::SIZE * 3 + usize::SIZE * 2)]
            .copy_from_slice(&self.cur_sector_capacity.as_fixed_size_bytes());
        buf[(u64::SIZE * 3 + usize::SIZE * 2)..(u64::SIZE * 3 + usize::SIZE * 3)]
            .copy_from_slice(&self.cur_sector_len.as_fixed_size_bytes());

        buf
    }

    fn from_fixed_size_bytes(buf: &[u8; Self::SIZE]) -> Self {
        let mut len_buf = u64::_u8_arr_of_size();
        len_buf.copy_from_slice(&buf[0..u64::SIZE]);
        let len = u64::from_fixed_size_bytes(&len_buf);

        let mut first_sector_ptr_buf = u64::_u8_arr_of_size();
        first_sector_ptr_buf.copy_from_slice(&buf[u64::SIZE..(u64::SIZE * 2)]);
        let first_sector_ptr = u64::from_fixed_size_bytes(&first_sector_ptr_buf);

        let mut cur_sector_ptr_buf = u64::_u8_arr_of_size();
        cur_sector_ptr_buf.copy_from_slice(&buf[(u64::SIZE * 2)..(u64::SIZE * 3)]);
        let cur_sector_ptr = u64::from_fixed_size_bytes(&cur_sector_ptr_buf);

        let mut cur_sector_last_item_offset_buf = usize::_u8_arr_of_size();
        cur_sector_last_item_offset_buf
            .copy_from_slice(&buf[(u64::SIZE * 3)..(u64::SIZE * 3 + usize::SIZE)]);
        let cur_sector_last_item_offset =
            usize::from_fixed_size_bytes(&cur_sector_last_item_offset_buf);

        let mut cur_sector_capacity_buf = usize::_u8_arr_of_size();
        cur_sector_capacity_buf.copy_from_slice(
            &buf[(u64::SIZE * 3 + usize::SIZE)..(u64::SIZE * 3 + usize::SIZE * 2)],
        );
        let cur_sector_capacity = usize::from_fixed_size_bytes(&cur_sector_capacity_buf);

        let mut cur_sector_len_buf = usize::_u8_arr_of_size();
        cur_sector_len_buf.copy_from_slice(
            &buf[(u64::SIZE * 3 + usize::SIZE * 2)..(u64::SIZE * 3 + usize::SIZE * 3)],
        );
        let cur_sector_len = usize::from_fixed_size_bytes(&cur_sector_len_buf);

        Self {
            len,
            first_sector_ptr,
            cur_sector_ptr,
            cur_sector_len,
            cur_sector_capacity,
            cur_sector_last_item_offset,
            _marker: PhantomData::default(),
        }
    }
}

impl<T: StableAllocated> StableAllocated for SLog<T>
where
    [(); T::SIZE]: Sized,
{
    #[inline]
    fn move_to_stable(&mut self) {}

    #[inline]
    fn remove_from_stable(&mut self) {}

    unsafe fn stable_drop(self) {
        let mut sector = if let Some(s) = self.get_first_sector() {
            s
        } else {
            return;
        };

        let mut len = if sector.as_ptr() == self.cur_sector_ptr {
            self.cur_sector_len
        } else {
            DEFAULT_CAPACITY * 2
        };
        let mut past = false;

        loop {
            for i in 0..len {
                let it = sector.read_element(i * T::SIZE);
                it.stable_drop();
            }

            let next = sector.read_next_ptr();
            sector.destroy();

            if next == EMPTY_PTR {
                break;
            }

            sector = Sector::from_ptr(next);
            len = if sector.as_ptr() == self.cur_sector_ptr {
                past = true;
                self.cur_sector_len
            } else if !past {
                len * 2
            } else {
                0
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::log::SLog;
    use crate::{init_allocator, stable, stable_memory_init};

    #[test]
    fn works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut log = SLog::new();

        assert!(log.is_empty());

        println!();
        println!("PUSHES");

        for i in 0..100 {
            log.debug_print();

            log.push(i);

            for j in 0..i {
                assert_eq!(log.get_copy(j).unwrap(), j);
            }
        }

        log.debug_print();

        assert_eq!(log.len(), 100);
        for i in 0..100 {
            assert_eq!(log.get_copy(i).unwrap(), i);
        }

        println!();
        println!("POPS");

        for i in (20..100).rev() {
            assert_eq!(log.pop().unwrap(), i);
            log.debug_print();
        }

        println!();
        println!("PUSHES again");

        assert_eq!(log.len(), 20);
        for i in 20..100 {
            log.push(i);
            log.debug_print();
        }

        for i in 0..100 {
            assert_eq!(log.get_copy(i).unwrap(), i);
        }

        println!();
        println!("POPS again");

        for i in (0..100).rev() {
            assert_eq!(log.pop().unwrap(), i);
            log.debug_print();
        }

        assert!(log.pop().is_none());
        assert!(log.is_empty());
    }

    #[test]
    fn iter_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut log = SLog::new();

        for i in 0..100 {
            log.push(i);
        }

        let mut j = 0;
        for i in log.iter() {
            assert_eq!(i, j);
            j += 1;
        }

        j -= 1;

        log.debug_print();

        for i in log.iter().rev() {
            assert_eq!(i, j);
            j -= 1;
        }
    }
}
