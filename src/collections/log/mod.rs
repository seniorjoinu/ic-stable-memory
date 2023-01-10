use crate::mem::allocator::EMPTY_PTR;
use crate::primitive::s_box_mut::SBoxMut;
use crate::primitive::StableAllocated;
use crate::utils::encoding::{AsDynSizeBytes, AsFixedSizeBytes, FixedSize};
use crate::{allocate, SSlice};
use std::marker::PhantomData;
use std::mem::size_of;
use std::ops::Deref;

const DEFAULT_CAPACITY: usize = 2;

pub struct SLog<T> {
    len: u64,
    cur_sector_ptr: u64,
    cur_sector_last_item_offset: usize,
    cur_sector_capacity: usize,
    cur_sector_len: usize,
    _marker: PhantomData<T>,
}

impl<T> SLog<T> {
    pub fn new() -> Self {
        Self {
            len: 0,
            cur_sector_ptr: EMPTY_PTR,
            cur_sector_last_item_offset: 0,
            cur_sector_capacity: DEFAULT_CAPACITY,
            cur_sector_len: 0,
            _marker: PhantomData::default(),
        }
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

    pub fn push(&mut self, it: T) {
        let mut sector = self.get_or_create_current_sector();
        self.grow_if_needed(&mut sector);

        sector.write_element(self.cur_sector_last_item_offset, it);
        self.cur_sector_last_item_offset += T::SIZE;
        self.cur_sector_len += 1;
        self.len += 1;
    }

    pub fn pop(&mut self) -> Option<T> {
        let mut sector = self.get_current_sector()?;

        self.cur_sector_last_item_offset -= T::SIZE;
        self.cur_sector_len -= 1;
        self.len -= 1;
    }

    #[inline]
    fn get_or_create_current_sector(&mut self) -> Sector<T> {
        if self.cur_sector_ptr == EMPTY_PTR {
            self.cur_sector_capacity *= 2;
            Sector::<T>::new(self.cur_sector_capacity, EMPTY_PTR)
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
    fn grow_if_needed(&mut self, sector: &mut Sector<T>) {
        if self.cur_sector_len == self.cur_sector_capacity {
            if self.cur_sector_capacity < Self::max_capacity() {
                self.cur_sector_capacity *= 2;
            }

            let new_sector = Sector::<T>::new(self.cur_sector_capacity, sector.as_ptr());
            sector.write_next_ptr(new_sector.as_ptr());

            self.cur_sector_ptr = new_sector.as_ptr();
            self.cur_sector_len = 0;
            self.cur_sector_last_item_offset = 0;

            *sector = new_sector;
        }
    }
}

const PREV_OFFSET: usize = 0;
const NEXT_OFFSET: usize = PREV_OFFSET + u64::SIZE;
const CAP_OFFSET: usize = NEXT_OFFSET + u64::SIZE;
const ELEMENTS_OFFSET: usize = CAP_OFFSET + usize::SIZE;

struct Sector<T>(u64, PhantomData<T>);

impl<T: StableAllocated> Sector<T>
where
    [(); T::SIZE]: Sized,
{
    fn new(cap: usize, prev: u64) -> Self {
        let slice = allocate(u64::SIZE * 2 + cap * T::SIZE);

        let mut it = Self(slice.get_ptr(), PhantomData::default());
        it.write_cap(cap);
        it.write_prev_ptr(prev);

        it
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
        SSlice::_as_fixed_size_bytes_read(self.0, PREV_OFFSET)
    }

    #[inline]
    fn write_prev_ptr(&mut self, ptr: u64) {
        SSlice::_as_fixed_size_bytes_write(self.0, PREV_OFFSET, ptr)
    }

    #[inline]
    fn read_next_ptr(&self) -> u64 {
        SSlice::_as_fixed_size_bytes_read(self.0, NEXT_OFFSET)
    }

    #[inline]
    fn write_next_ptr(&mut self, ptr: u64) {
        SSlice::_as_fixed_size_bytes_write(self.0, NEXT_OFFSET, ptr)
    }

    #[inline]
    fn read_cap(&self) -> usize {
        SSlice::_as_fixed_size_bytes_read(self.0, CAP_OFFSET)
    }

    #[inline]
    fn write_cap(&mut self, cap: usize) {
        SSlice::_as_fixed_size_bytes_write(self.0, CAP_OFFSET, cap)
    }

    #[inline]
    fn read_element(&self, offset: usize) -> T {
        SSlice::_as_fixed_size_bytes_read(self.0, offset)
    }

    #[inline]
    fn write_element(&self, offset: usize, element: T) {
        SSlice::_as_fixed_size_bytes_write(self.0, offset, element)
    }
}
