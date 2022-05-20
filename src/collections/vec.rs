use crate::mem::membox::raw::Side;
use crate::mem::membox::s::{SBox, SBoxError};
use crate::utils::encode::AsBytes;
use crate::{allocate, deallocate, RawSBox};
use candid::{CandidType, Deserialize, Error as CandidError};
use std::fmt;
use std::fmt::{Debug, Display, Formatter};
use std::marker::PhantomData;
use std::mem::size_of;

const STABLE_VEC_DEFAULT_CAPACITY: u64 = 4;
const MAX_SECTOR_SIZE: usize = 2usize.pow(29); // 512MB

#[derive(Debug)]
pub enum SVecError {
    CandidError(CandidError),
    OutOfMemory,
    OutOfBounds,
    NoVecAtPtr,
    IsEmpty,
}

impl SVecError {
    pub fn from_sbox_err(e: SBoxError) -> Self {
        match e {
            SBoxError::CandidError(e) => Self::CandidError(e),
            SBoxError::OutOfMemory => Self::OutOfMemory,
        }
    }
}

#[derive(Copy, Clone)]
pub struct SVec<T: Copy>(SBox<StableVecInfo>, PhantomData<T>);

#[derive(Copy, Clone)]
struct SVecSector;

// TODO: optimize - separate len and capacity from sectors
#[derive(CandidType, Deserialize, Clone)]
struct StableVecInfo {
    len: u64,
    capacity: u64,
    // TODO: optimize - replace with struct {ptr; size}
    sectors: Vec<RawSBox<SVecSector>>,
}

impl<T: Copy> SVec<T> {
    pub fn new() -> Result<Self, SVecError> {
        Self::new_with_capacity(STABLE_VEC_DEFAULT_CAPACITY)
    }

    pub fn new_with_capacity(capacity: u64) -> Result<Self, SVecError> {
        let mut sectors = vec![];
        let mut capacity_size = capacity * size_of::<T>() as u64;

        while capacity_size > MAX_SECTOR_SIZE as u64 {
            let sector_res =
                allocate::<SVecSector>(MAX_SECTOR_SIZE).map_err(|_| SVecError::OutOfMemory);

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

        let sector_res =
            allocate::<SVecSector>(capacity_size as usize).map_err(|_| SVecError::OutOfMemory);

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

        let info_sbox = SBox::new(&info).map_err(|e| {
            for sector in sectors {
                deallocate(sector);
            }

            SVecError::from_sbox_err(e)
        })?;

        Ok(Self(info_sbox, PhantomData::default()))
    }

    pub fn push(&mut self, element: &T) -> Result<(), SVecError> {
        let new_sector_opt = self.grow_if_needed()?;

        self.set_len(self.len() + 1);

        let (mut sector, offset) = if let Some(new_sector) = new_sector_opt {
            (new_sector, 0usize)
        } else {
            let (sector, offset) = self.calculate_inner_index(self.len() - 1);
            (sector, offset)
        };

        let bytes_element = element.as_bytes();
        sector._write_bytes(offset, &bytes_element);

        Ok(())
    }

    pub fn pop(&mut self) -> Result<T, SVecError> {
        let len = self.len();
        if len == 0 {
            return Err(SVecError::IsEmpty);
        }

        let idx = len - 1;

        let (sector, offset) = self.calculate_inner_index(idx);

        println!("{}", offset / size_of::<T>());

        let mut element_bytes = vec![0u8; size_of::<T>()];
        sector._read_bytes(offset, &mut element_bytes);
        let element = T::from_bytes(&element_bytes);

        self.set_len(idx);

        Ok(element)
    }

    fn get_cloned(&self, idx: u64) -> Result<T, SVecError> {
        if idx >= self.len() {
            return Err(SVecError::OutOfBounds);
        }

        let (sector, offset) = self.calculate_inner_index(idx);

        let mut element_bytes = vec![0u8; size_of::<T>()];
        sector._read_bytes(offset, &mut element_bytes);
        let element = T::from_bytes(&element_bytes);

        Ok(element)
    }

    pub fn set(&mut self, idx: u64, element: &T) -> Result<(), SVecError> {
        if idx >= self.len() {
            return Err(SVecError::OutOfBounds);
        }

        let (mut sector, offset) = self.calculate_inner_index(idx);

        let bytes_element = element.as_bytes();
        sector._write_bytes(offset, &bytes_element);

        Ok(())
    }

    pub fn destroy(self) {
        let info = self.get_info();

        for i in 0..info.len {
            let (sector, offset) = self.calculate_inner_index(i);

            let ptr = sector._read_word(offset);
            if ptr != 0 {
                let membox = unsafe { RawSBox::<T>::from_ptr(ptr, Side::Start).unwrap() };
                deallocate(membox);
            }
        }

        for sector in info.sectors {
            deallocate(sector);
        }

        self.0.destroy();
    }

    pub fn capacity(&self) -> u64 {
        self.get_info().capacity
    }

    pub fn len(&self) -> u64 {
        self.get_info().len
    }

    pub fn is_empty(&self) -> bool {
        self.get_info().len == 0
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

    fn grow_if_needed(&mut self) -> Result<Option<RawSBox<SVecSector>>, SVecError> {
        let mut info = self.get_info();

        if info.len == info.capacity {
            let last_sector_size = info.sectors.last().map_or(
                STABLE_VEC_DEFAULT_CAPACITY as usize * size_of::<T>() as usize,
                |it| it.get_size_bytes() as usize,
            );
            let new_sector_size = if last_sector_size * 2 < MAX_SECTOR_SIZE {
                last_sector_size * 2
            } else {
                MAX_SECTOR_SIZE
            };

            let sector = allocate(new_sector_size).map_err(|_| SVecError::OutOfMemory)?;

            info.capacity += (new_sector_size / size_of::<T>()) as u64;
            info.sectors.push(sector);

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

    fn calculate_inner_index(&self, idx: u64) -> (RawSBox<SVecSector>, usize) {
        let info = self.get_info();
        assert!(idx < info.len);

        let mut idx_counter: u64 = 0;

        for sector in info.sectors {
            let ptrs_in_sector = (sector.get_size_bytes() / size_of::<T>()) as u64;
            idx_counter += ptrs_in_sector;

            if idx_counter > idx {
                if idx == 0 {
                    return (sector, 0);
                }

                // usize cast guaranteed by the fact that a single sector can only hold usize of
                // bytes and we iterate over them one by one
                let offset = (ptrs_in_sector - (idx_counter - idx)) as usize * size_of::<T>();

                return (sector, offset);
            }
        }

        // guaranteed by the len check at the beginning of the function
        unreachable!("Unable to calculate inner index");
    }

    fn get_info(&self) -> StableVecInfo {
        self.0.get_cloned().unwrap()
    }

    fn set_info(&mut self, new_info: StableVecInfo) -> Result<(), SVecError> {
        self.0.set(new_info).map_err(SVecError::from_sbox_err)
    }
}

impl<T: Debug + Copy> Debug for SVec<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let info = self.get_info();

        let mut sector_strs = Vec::new();
        for sector in info.sectors {
            let mut elems = Vec::new();

            let size = sector.get_size_bytes();
            let size_elems = (size / size_of::<T>() as usize) as usize;

            for i in 0..size_elems {
                let mut elem_bytes = vec![0u8; size_of::<T>()];
                sector._read_bytes(i * size_of::<T>(), &mut elem_bytes);
                elems.push(format!("{:?}", T::from_bytes(&elem_bytes)));
            }

            sector_strs.push(elems)
        }

        f.debug_struct("SVec")
            .field("len", &info.len)
            .field("capacity", &info.capacity)
            .field("sectors", &sector_strs)
            .finish()
    }
}

impl<T: Copy + Display> Display for SVec<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut elements = vec![];

        for i in 0..self.len() {
            let element = self.get_cloned(i).unwrap();
            elements.push(format!("{}", element));
        }

        write!(f, "[{}]", elements.join(","))
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::vec::{SVec, STABLE_VEC_DEFAULT_CAPACITY};
    use crate::init_allocator;
    use crate::mem::membox::s::SBox;
    use crate::utils::encode::{decode_one_allow_trailing, AsBytes};
    use crate::utils::mem_context::stable;
    use candid::{encode_one, CandidType, Deserialize, Nat};

    #[test]
    fn create_destroy_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut stable_vec = SVec::<SBox<TestIndirect>>::new().unwrap();
        assert_eq!(stable_vec.capacity(), STABLE_VEC_DEFAULT_CAPACITY);
        assert_eq!(stable_vec.len(), 0);

        stable_vec.destroy();

        stable_vec = SVec::<SBox<TestIndirect>>::new_with_capacity(10_000).unwrap();
        assert_eq!(stable_vec.capacity(), 10_000);
        assert_eq!(stable_vec.len(), 0);

        stable_vec.destroy();
    }

    #[derive(Copy, Clone, Debug)]
    struct Test {
        a: u64,
        b: u64,
    }

    #[test]
    fn push_pop_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut stable_vec = SVec::new().unwrap();
        let count = 1000u64;

        for i in 0..count {
            let it = Test { a: i, b: count - i };

            stable_vec
                .push(&it)
                .unwrap_or_else(|e| panic!("Unable to push at step {}: {:?}", i, e));
        }

        assert_eq!(stable_vec.len(), count, "Invalid len after push");

        for i in 0..count {
            let it = Test { a: count - i, b: i };

            stable_vec
                .set(i, &it)
                .unwrap_or_else(|e| panic!("Unable to set at step {}: {:?}", i, e));
        }

        assert_eq!(stable_vec.len(), count, "Invalid len after set");

        for i in 0..count {
            let it = stable_vec
                .get_cloned(i)
                .unwrap_or_else(|e| panic!("Unable to set at step {}: {:?}", i, e));

            assert_eq!(it.a, count - i);
            assert_eq!(it.b, i);
        }

        for i in 0..count {
            let it = stable_vec
                .pop()
                .unwrap_or_else(|e| panic!("Unable to pop at step {}: {:?}", i, e));

            assert_eq!(it.a, (i + 1)); // i+1 because the last one will be {a: 1; b: 999}
            assert_eq!(it.b, count - (i + 1));
        }

        assert_eq!(stable_vec.len(), 0, "Invalid len after pop");
    }

    #[derive(CandidType, Deserialize, Debug)]
    struct TestIndirect {
        a: Nat,
        b: String,
    }

    impl AsBytes for TestIndirect {
        fn as_bytes(&self) -> Vec<u8> {
            encode_one(self).unwrap()
        }

        fn from_bytes(bytes: &[u8]) -> Self {
            decode_one_allow_trailing(bytes).unwrap()
        }
    }

    #[test]
    fn push_pop_indirect_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut stable_vec = SVec::new().unwrap();
        let count = 1000u64;

        for i in 0..count {
            let it = SBox::new(&TestIndirect {
                a: Nat::from(i),
                b: format!("Str {}", i),
            })
            .unwrap();

            stable_vec
                .push(&it)
                .unwrap_or_else(|e| panic!("Unable to push at step {}: {:?}", i, e));
        }

        assert_eq!(stable_vec.len(), count, "Invalid len after push");

        for i in 0..count {
            let it = SBox::new(&TestIndirect {
                a: Nat::from(i),
                b: format!(
                    "Much bigger str that should cause reallocation of the element {}",
                    i
                ),
            })
            .unwrap();

            stable_vec
                .set(i, &it)
                .unwrap_or_else(|e| panic!("Unable to set at step {}: {:?}", i, e));
        }

        assert_eq!(stable_vec.len(), count, "Invalid len after push");

        for i in 0..count {
            let it = stable_vec
                .pop()
                .unwrap_or_else(|e| panic!("Unable to pop at step {}: {:?}", i, e))
                .get_cloned()
                .unwrap_or_else(|e| panic!("Unable to get_cloned at step {}: {:?}", i, e));

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
