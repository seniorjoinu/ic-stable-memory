use crate::mem::membox::candid::CandidMemBoxError;
use crate::mem::membox::common::Side;
use crate::{allocate, deallocate, reallocate, MemBox};
use candid::types::{Serializer, Type};
use candid::{decode_one, encode_one, CandidType, Deserialize, Error as CandidError};
use serde::de::DeserializeOwned;
use std::fmt;
use std::fmt::{Debug, Display, Formatter};
use std::marker::PhantomData;
use std::mem::size_of;

pub const STABLE_VEC_DEFAULT_CAPACITY: u64 = 4;
pub const MAX_SECTOR_SIZE: usize = 2usize.pow(29); // 512MB
pub const PTR_SIZE: u64 = size_of::<u64>() as u64;

#[derive(Debug)]
pub enum StableVecError {
    CandidError(CandidError),
    OutOfMemory,
    OutOfBounds,
    NoVecAtPtr,
    IsEmpty,
}

#[derive(Clone)]
pub struct StableVec<T> {
    membox: MemBox<StableVecInfo<T>>,
}

impl<T> CandidType for StableVec<T> {
    fn _ty() -> Type {
        Type::Nat64
    }

    fn idl_serialize<S>(&self, serializer: S) -> Result<(), S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_nat64(self.membox.get_ptr())
    }
}

struct StableVecSector<T> {
    data: PhantomData<T>,
}

impl<T> Clone for StableVecSector<T> {
    fn clone(&self) -> Self {
        Self {
            data: PhantomData::default(),
        }
    }
}

// TODO: optimize - separate len and capacity from sectors
#[derive(CandidType, Deserialize, Clone)]
struct StableVecInfo<T> {
    len: u64,
    capacity: u64,
    // TODO: optimize - replace to struct {ptr; size}
    sectors: Vec<MemBox<StableVecSector<T>>>,
}

impl<'de, T: CandidType + DeserializeOwned> StableVec<T> {
    pub fn new() -> Result<Self, StableVecError> {
        Self::new_with_capacity(STABLE_VEC_DEFAULT_CAPACITY)
    }

    pub fn new_with_capacity(capacity: u64) -> Result<Self, StableVecError> {
        let mut sectors = vec![];
        let mut capacity_size = capacity * PTR_SIZE;

        while capacity_size > MAX_SECTOR_SIZE as u64 {
            let sector_res = allocate::<StableVecSector<T>>(MAX_SECTOR_SIZE)
                .map_err(|_| StableVecError::OutOfMemory);

            match sector_res {
                Ok(sector) => {
                    sectors.push(sector);
                    capacity_size -= MAX_SECTOR_SIZE as u64;
                }
                // revert
                Err(e) => {
                    for sector in sectors {
                        deallocate(sector);
                    }

                    return Err(e);
                }
            }
        }

        let sector_res = allocate::<StableVecSector<T>>(capacity_size as usize)
            .map_err(|_| StableVecError::OutOfMemory);

        match sector_res {
            Ok(sector) => {
                sectors.push(sector);
            }
            // revert
            Err(e) => {
                for sector in sectors {
                    deallocate(sector);
                }

                return Err(e);
            }
        }

        let info = StableVecInfo {
            len: 0,
            capacity,
            sectors: sectors.clone(),
        };

        let info_encoded_res = encode_one(info).map_err(StableVecError::CandidError);
        if let Err(e) = info_encoded_res {
            for sector in sectors {
                deallocate(sector);
            }

            return Err(e);
        }

        let info_encoded = info_encoded_res?;

        let vec_info_res = allocate::<StableVecInfo<T>>(info_encoded.len())
            .map_err(|_| StableVecError::OutOfMemory);

        if let Err(e) = vec_info_res {
            for sector in sectors {
                deallocate(sector);
            }

            return Err(e);
        }

        let mut vec_info = vec_info_res?;

        vec_info._write_bytes(0, &info_encoded);

        Ok(Self { membox: vec_info })
    }

    pub fn from_ptr(ptr: u64) -> Result<Self, StableVecError> {
        let membox = unsafe {
            MemBox::<StableVecInfo<T>>::from_ptr(ptr, Side::Start)
                .ok_or(StableVecError::NoVecAtPtr)?
        };
        membox.get_cloned().map_err(|e| match e {
            CandidMemBoxError::CandidError(e) => StableVecError::CandidError(e),
            _ => unreachable!(),
        })?;

        Ok(Self { membox })
    }

    pub fn push(&mut self, element: T) -> Result<(), StableVecError> {
        let encoded_element = encode_one(element).map_err(StableVecError::CandidError)?;
        let mut membox =
            allocate::<T>(encoded_element.len()).map_err(|_| StableVecError::OutOfMemory)?;
        membox._write_bytes(0, &encoded_element);

        match self.grow_if_needed() {
            Ok(new_sector_opt) => {
                self.set_len(self.len() + 1);

                let (mut sector, offset) = if let Some(new_sector) = new_sector_opt {
                    (new_sector, 0usize)
                } else {
                    let (sector, offset) = self.calculate_inner_index(self.len() - 1);
                    (sector, offset)
                };

                sector._write_word(offset, membox.get_ptr());

                Ok(())
            }
            Err(e) => match e {
                StableVecError::OutOfMemory => {
                    deallocate(membox);

                    Err(e)
                }
                _ => unreachable!(),
            },
        }
    }

    pub fn pop(&mut self) -> Result<T, StableVecError> {
        let len = self.len();
        if len == 0 {
            return Err(StableVecError::IsEmpty);
        }

        let idx = len - 1;

        let (mut sector, offset) = self.calculate_inner_index(idx);
        let element_ptr = sector._read_word(offset);
        assert_ne!(element_ptr, 0);

        let membox = unsafe { MemBox::<T>::from_ptr(element_ptr, Side::Start).unwrap() };
        let element = membox.get_cloned().map_err(|e| match e {
            CandidMemBoxError::CandidError(e) => StableVecError::CandidError(e),
            _ => unreachable!(),
        })?;

        self.set_len(idx);
        sector._write_word(offset, 0);

        Ok(element)
    }

    fn get_cloned(&self, idx: u64) -> Result<T, StableVecError> {
        if idx >= self.len() {
            return Err(StableVecError::OutOfBounds);
        }

        let (sector, offset) = self.calculate_inner_index(idx);
        let element_ptr = sector._read_word(offset);
        if element_ptr == 0 {
            unreachable!("It can't be empty");
        }

        let membox = unsafe { MemBox::<T>::from_ptr(element_ptr, Side::Start).unwrap() };
        let element = membox.get_cloned().map_err(|e| match e {
            CandidMemBoxError::CandidError(e) => StableVecError::CandidError(e),
            _ => unreachable!(),
        })?;

        Ok(element)
    }

    pub fn set(&mut self, idx: u64, element: T) -> Result<(), StableVecError> {
        if idx >= self.len() {
            return Err(StableVecError::OutOfBounds);
        }

        let (mut sector, offset) = self.calculate_inner_index(idx);
        let element_ptr = sector._read_word(offset);
        assert_ne!(element_ptr, 0);

        let mut membox = unsafe { MemBox::<T>::from_ptr(element_ptr, Side::Start).unwrap() };
        match membox.set(element) {
            Err(e) => match e {
                CandidMemBoxError::MemBoxOverflow(encoded_element) => {
                    membox = reallocate(membox, encoded_element.len())
                        .map_err(|_| StableVecError::OutOfMemory)?;
                    membox._write_bytes(0, &encoded_element);

                    sector._write_word(offset, membox.get_ptr());

                    Ok(())
                }
                CandidMemBoxError::CandidError(e) => Err(StableVecError::CandidError(e)),
            },
            _ => Ok(()),
        }
    }

    pub fn destroy(self) {
        let info = self.get_info();

        for i in 0..info.len {
            let (sector, offset) = self.calculate_inner_index(i);

            let ptr = sector._read_word(offset);
            if ptr != 0 {
                let membox = unsafe { MemBox::<T>::from_ptr(ptr, Side::Start).unwrap() };
                deallocate(membox);
            }
        }

        for sector in info.sectors {
            deallocate(sector);
        }

        deallocate(self.membox);
    }

    pub fn capacity(&self) -> u64 {
        self.get_info().capacity
    }

    pub fn len(&self) -> u64 {
        self.get_info().len
    }

    pub fn get_ptr(&self) -> u64 {
        self.membox.get_ptr()
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

    fn grow_if_needed(&mut self) -> Result<Option<MemBox<StableVecSector<T>>>, StableVecError> {
        let mut info = self.get_info();

        if info.len == info.capacity {
            let last_sector_size = info.sectors.last().map_or(
                STABLE_VEC_DEFAULT_CAPACITY as usize * PTR_SIZE as usize,
                |it| it.get_size_bytes() as usize,
            );
            let new_sector_size = if last_sector_size * 2 < MAX_SECTOR_SIZE {
                last_sector_size * 2
            } else {
                MAX_SECTOR_SIZE
            };

            let sector = allocate(new_sector_size).map_err(|_| StableVecError::OutOfMemory)?;

            info.capacity += new_sector_size as u64 / PTR_SIZE;
            info.sectors.push(sector.clone());

            if let Err(e) = self.set_info(info) {
                deallocate(sector);

                Err(e)
            } else {
                Ok(Some(sector))
            }
        } else {
            Ok(None)
        }
    }

    fn calculate_inner_index(&self, idx: u64) -> (MemBox<StableVecSector<T>>, usize) {
        let info = self.get_info();
        assert!(idx < info.len);

        let mut idx_counter: u64 = 0;

        for sector in info.sectors {
            let ptrs_in_sector = sector.get_size_bytes() as u64 / PTR_SIZE;
            idx_counter += ptrs_in_sector;

            if idx_counter > idx {
                if idx == 0 {
                    return (sector, 0);
                }

                // usize cast guaranteed by the fact that a single sector can only hold usize of
                // bytes and we iterate over them one by one

                let offset = ((ptrs_in_sector - (idx_counter - idx)) * PTR_SIZE) as usize;

                return (sector, offset);
            }
        }

        // guaranteed by the len check at the beginning of the function
        unreachable!("Unable to calculate inner index");
    }

    fn get_info(&self) -> StableVecInfo<T> {
        self.membox.get_cloned().unwrap()
    }

    fn set_info(&mut self, new_info: StableVecInfo<T>) -> Result<(), StableVecError> {
        if let Err(e) = self.membox.set(new_info) {
            match e {
                CandidMemBoxError::CandidError(e) => Err(StableVecError::CandidError(e)),
                CandidMemBoxError::MemBoxOverflow(encoded_info) => {
                    self.membox = reallocate(self.membox.clone(), encoded_info.len())
                        .map_err(|_| StableVecError::OutOfMemory)?;
                    self.membox._write_bytes(0, &encoded_info);

                    Ok(())
                }
            }
        } else {
            Ok(())
        }
    }
}

impl<T: CandidType + DeserializeOwned> Debug for MemBox<StableVecSector<T>> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let (size, allocated) = self.get_meta();
        let size_ptrs = (size / PTR_SIZE as usize) as usize;

        let mut content = vec![];
        for i in 0..size_ptrs {
            let ptr = self._read_word(i * PTR_SIZE as usize);
            content.push(ptr);
        }

        f.debug_struct("StableVecSector")
            .field("ptr", &self.get_ptr())
            .field("size", &size)
            .field("is_allocated", &allocated)
            .field("content", &content)
            .finish()
    }
}

impl<T: CandidType + DeserializeOwned> Debug for StableVec<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let info = self.get_info();

        f.debug_struct("StableVec")
            .field("len", &info.len)
            .field("capacity", &info.capacity)
            .field("sectors", &info.sectors)
            .finish()
    }
}

impl<T: CandidType + DeserializeOwned + Debug> Display for StableVec<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut elements = vec![];

        for i in 0..self.len() {
            let element = self.get_cloned(i).unwrap();
            elements.push(element);
        }

        write!(f, "{:#?}", elements)
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::vec::{StableVec, STABLE_VEC_DEFAULT_CAPACITY};
    use crate::utils::mem_context::stable;
    use crate::{get_allocator, init_allocator};
    use candid::{CandidType, Deserialize, Nat};

    #[derive(CandidType, Deserialize, Debug)]
    struct Test {
        a: Nat,
        b: String,
    }

    #[test]
    fn create_destroy_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();

        init_allocator(0);

        let mut stable_vec = StableVec::<Test>::new().unwrap();
        assert_eq!(stable_vec.capacity(), STABLE_VEC_DEFAULT_CAPACITY);
        assert_eq!(stable_vec.len(), 0);

        stable_vec = StableVec::<Test>::from_ptr(stable_vec.get_ptr())
            .ok()
            .unwrap();
        assert_eq!(stable_vec.capacity(), STABLE_VEC_DEFAULT_CAPACITY);
        assert_eq!(stable_vec.len(), 0);

        stable_vec.destroy();

        stable_vec = StableVec::<Test>::new_with_capacity(10_000).unwrap();
        assert_eq!(stable_vec.capacity(), 10_000);
        assert_eq!(stable_vec.len(), 0);

        stable_vec = StableVec::<Test>::from_ptr(stable_vec.get_ptr())
            .ok()
            .unwrap();
        assert_eq!(stable_vec.capacity(), 10_000);
        assert_eq!(stable_vec.len(), 0);

        stable_vec.destroy();
    }

    #[test]
    fn push_pop_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();

        init_allocator(0);

        let mut stable_vec = StableVec::new().unwrap();
        let count = 1000u64;

        for i in 0..count {
            stable_vec
                .push(Test {
                    a: Nat::from(i),
                    b: format!("Str {}", i),
                })
                .unwrap_or_else(|e| panic!("Unable to push at step {}: {:?}", i, e));
        }

        assert_eq!(stable_vec.len(), count, "Invalid len after push");

        for i in 0..count {
            stable_vec
                .set(
                    i,
                    Test {
                        a: Nat::from(i),
                        b: format!(
                            "Much bigger str that should cause reallocation of the element {}",
                            i
                        ),
                    },
                )
                .unwrap_or_else(|e| panic!("Unable to set at step {}: {:?}", i, e));
        }

        assert_eq!(stable_vec.len(), count, "Invalid len after push");

        for i in 0..count {
            let it = stable_vec
                .pop()
                .unwrap_or_else(|e| panic!("Unable to pop at step {}: {:?}", i, e));

            assert_eq!(it.a, Nat::from(count - 1 - i));
            assert_eq!(
                it.b,
                format!(
                    "Much bigger str that should cause reallocation of the element {}",
                    count - 1 - i
                )
            );
        }

        assert_eq!(stable_vec.len(), 0, "Invalid len after pop");
    }
}
