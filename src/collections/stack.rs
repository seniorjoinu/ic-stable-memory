use crate::mem::membox::candid::CandidMemBoxError;
use crate::mem::membox::common::Side;
use crate::{allocate, deallocate, reallocate, MemBox};
use candid::encode_one;
use candid::types::{Serializer, Type};
use ic_cdk::export::candid::{CandidType, Deserialize, Error as CandidError};
use std::marker::PhantomData;
use std::mem::size_of;

pub const STABLE_VEC_DEFAULT_CAPACITY: u64 = 4;
pub const MAX_SECTOR_SIZE: usize = 2usize.pow(29); // 512MB
pub const PTR_SIZE: u64 = size_of::<u64>() as u64;

#[derive(Debug)]
pub enum StackError {
    CandidError(CandidError),
    OutOfMemory,
    OutOfBounds,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Stack<T> {
    membox: MemBox<StackInfo>,
    data: PhantomData<T>,
}

impl<T> CandidType for Stack<T> {
    fn _ty() -> Type {
        Type::Empty
    }

    fn idl_serialize<S>(&self, serializer: S) -> Result<(), S::Error>
    where
        S: Serializer,
    {
        Ok(())
    }
}

#[derive(CandidType, Deserialize, Copy, Clone, Debug)]
struct StackSector;

// TODO: optimize - separate len and capacity from sectors
#[derive(CandidType, Deserialize, Clone, Debug)]
struct StackInfo {
    len: u64,
    capacity: u64,
    // TODO: optimize - replace to struct {ptr; size}
    sectors: Vec<MemBox<StackSector>>,
}

impl<'de, T: CandidType + Deserialize<'de>> Stack<T> {
    pub fn new() -> Result<Self, StackError> {
        Self::new_with_capacity(STABLE_VEC_DEFAULT_CAPACITY)
    }

    pub fn new_with_capacity(capacity: u64) -> Result<Self, StackError> {
        let mut sectors = vec![];
        let mut capacity_size = capacity * PTR_SIZE;

        while capacity_size > MAX_SECTOR_SIZE {
            let sector_res = Self::new_sector(MAX_SECTOR_SIZE);
            match sector_res {
                Ok(sector) => {
                    sectors.push(sector);
                    capacity_size -= MAX_SECTOR_SIZE;
                }
                // revert
                Err(e) => {
                    for sector in sectors {
                        Self::drop_sector(sector);
                    }

                    return Err(e);
                }
            }
        }

        let sector_res = Self::new_sector(capacity_size as usize);
        match sector_res {
            Ok(sector) => {
                sectors.push(sector);
            }
            // revert
            Err(e) => {
                for sector in sectors {
                    Self::drop_sector(sector);
                }

                return Err(e);
            }
        }

        let info = StackInfo {
            len: 0,
            capacity,
            sectors,
        };

        let info_encoded_res = encode_one(info).map_err(StackError::CandidError);
        if let Err(e) = info_encoded_res {
            for sector in sectors {
                Self::drop_sector(sector);
            }

            return Err(e);
        }

        let info_encoded = info_encoded_res?;

        let stack_info_res =
            allocate::<StackInfo>(info_encoded.len()).map_err(StackError::OutOfMemory);

        if let Err(e) = stack_info_res {
            for sector in sectors {
                Self::drop_sector(sector);
            }

            return Err(e);
        }

        let mut stack_info = stack_info_res?;

        stack_info._write_bytes(0, &info_encoded);

        Ok(Self {
            membox: stack_info,
            data: PhantomData::default(),
        })
    }

    pub fn push(&mut self, element: T) -> Result<(), StackError> {
        let encoded_element = encode_one(element).map_err(StackError::CandidError)?;
        let membox = allocate(encoded_element.len()).map_err(StackError::OutOfMemory)?;

        match self.grow_if_needed() {
            Ok(new_sector_opt) => {
                self.set_len(self.len() + 1);

                let (mut sector, offset) = if let Some(new_sector) = new_sector_opt {
                    (new_sector, 0usize)
                } else {
                    let (sector, offset) = self.calculate_inner_index(self.len() - 1).unwrap();
                    (sector, offset + PTR_SIZE)
                };

                sector._write_word(0, membox.get_ptr());

                Ok(())
            }
            Err(e) => match e {
                StackError::OutOfMemory => {
                    deallocate(membox);

                    Err(e)
                }
                _ => unreachable!(),
            },
        }
    }

    pub fn pop(&mut self) -> Option<T> {
        let idx = self.len() - 1;

        let (mut sector, offset) = self.calculate_inner_index(idx).unwrap();
        let element_ptr = sector._read_word(offset);
        if element_ptr == 0 {
            return None;
        }

        let membox = unsafe { MemBox::<T>::from_ptr(element_ptr, Side::Start)? };
        let element = membox.get_cloned().ok()?;

        self.set_len(idx);
        sector._write_word(offset, 0);

        Some(element)
    }

    fn get_cloned(&self, idx: u64) -> Result<T, StackError> {
        if idx >= self.len() {
            return Err(StackError::OutOfBounds);
        }

        let (mut sector, offset) = self.calculate_inner_index(idx).unwrap();
        let element_ptr = sector._read_word(offset);
        if element_ptr == 0 {
            unreachable!("It can't be empty");
        }

        let membox = unsafe { MemBox::<T>::from_ptr(element_ptr, Side::Start).unwrap() };
        let element = membox.get_cloned().map_err(StackError::CandidError)?;

        Ok(element)
    }

    pub fn set(&mut self, idx: u64, element: T) -> Result<(), StackError> {
        if idx >= self.len() {
            return Err(StackError::OutOfBounds);
        }

        let (sector, offset) = self.calculate_inner_index(idx)?;
        let element_ptr = sector._read_word(offset);
        if element_ptr == 0 {
            unreachable!("It can't be empty");
        }

        let mut membox = unsafe { MemBox::<T>::from_ptr(element_ptr, Side::Start).unwrap() };
        match membox.set(element) {
            Err(e) => match e {
                CandidMemBoxError::MemBoxOverflow(encoded_element) => {
                    membox = reallocate(membox, encoded_element.len())
                        .map_err(StackError::OutOfMemory)?;
                    membox._write_bytes(0, &encoded_element);

                    Ok(())
                }
                CandidMemBoxError::CandidError(e) => StackError::CandidError(e),
            },
            _ => Ok(()),
        }
    }

    pub fn capacity(&self) -> u64 {
        self.get_info().capacity
    }

    pub fn len(&self) -> u64 {
        self.get_info().len
    }

    fn set_len(&mut self, new_len: u64) {
        let mut info = self.get_info();
        info.len = new_len;

        self.set_info(info).unwrap();
    }

    pub fn is_about_to_grow(&self) -> bool {
        let info = self.get_info();

        info.len == info.capacity
    }

    fn grow_if_needed(&mut self) -> Result<Option<MemBox<StackSector>>, StackError> {
        let mut info = self.get_info();
        if info.len == info.capacity {
            let last_sector_size = info
                .sectors
                .last()
                .map_or(STABLE_VEC_DEFAULT_CAPACITY as usize * PTR_SIZE, |it| {
                    it.get_size_bytes() as usize
                });
            let new_sector_size = if last_sector_size * 2 < MAX_SECTOR_SIZE {
                last_sector_size * 2
            } else {
                MAX_SECTOR_SIZE
            };

            let sector = Self::new_sector(new_sector_size).map_err(StackError::OutOfMemory)?;

            info.capacity += new_sector_size / PTR_SIZE;
            info.sectors.push(sector.clone());

            if let Err(e) = self.set_info(info) {
                Self::drop_sector(sector);

                Err(e)
            } else {
                Ok(Some(sector))
            }
        } else {
            Ok(None)
        }
    }

    fn calculate_inner_index(&self, idx: u64) -> Option<(MemBox<StackSector>, usize)> {
        let info = self.get_info();
        if idx >= info.len {
            return None;
        }

        let mut idx_counter: u64 = 0;

        for sector in info.sectors {
            let ptrs_in_sector = sector.get_size_bytes() as u64 / PTR_SIZE;
            idx_counter += ptrs_in_sector;

            if idx_counter > idx {
                // usize cast guaranteed by the fact that a single sector can only hold usize of
                // bytes and we iterate over them one by one
                return Some((sector, (idx_counter % idx) as usize));
            }
        }

        // guaranteed by the len check at the beginning of the function
        unreachable!("Unable to calculate inner index");
    }

    fn get_info(&self) -> StackInfo {
        self.membox.get_cloned().unwrap()
    }

    fn set_info(&mut self, new_info: StackInfo) -> Result<(), StackError> {
        if let Err(e) = self.membox.set(new_info) {
            match e {
                CandidMemBoxError::CandidError(e) => Err(StackError::CandidError(e)),
                CandidMemBoxError::MemBoxOverflow(encoded_info) => {
                    self.membox = reallocate(self.membox.clone(), encoded_info.len())
                        .map_err(|_| StackError::OutOfMemory)?;
                    self.membox._write_bytes(0, &encoded_info);

                    Ok(())
                }
            }
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn creation_works_fine() {}
}
