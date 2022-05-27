use crate::primitive::raw_s_cell::Side;
use crate::primitive::s_cellbox::SCellBox;
use crate::utils::encode::AsBytes;
use crate::{allocate, deallocate, OutOfMemory, RawSCell};
use candid::{CandidType, Deserialize};
use std::fmt;
use std::fmt::{Debug, Display, Formatter};
use std::marker::PhantomData;
use std::mem::size_of;

const STABLE_VEC_DEFAULT_CAPACITY: u64 = 4;
const MAX_SECTOR_SIZE: usize = 2usize.pow(29); // 512MB

pub struct SVec<T: Sized + AsBytes>(SCellBox<StableVecInfo>, PhantomData<T>);

// TODO: optimize - separate len and capacity from sectors
#[derive(CandidType, Deserialize)]
struct StableVecInfo {
    len: u64,
    capacity: u64,
    // TODO: optimize - replace with struct {ptr; size}
    sectors: Vec<RawSCell<SVecSector>>,
}

#[derive(Copy, Clone)]
struct SVecSector;

impl<T: Sized + AsBytes> SVec<T> {
    pub fn new() -> Result<Self, OutOfMemory> {
        Self::new_with_capacity(STABLE_VEC_DEFAULT_CAPACITY)
    }

    pub fn new_with_capacity(capacity: u64) -> Result<Self, OutOfMemory> {
        let mut sectors = vec![];
        let mut capacity_size = capacity * size_of::<T>() as u64;

        while capacity_size > MAX_SECTOR_SIZE as u64 {
            let sector_res = allocate::<SVecSector>(MAX_SECTOR_SIZE);

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

        let sector_res = allocate::<SVecSector>(capacity_size as usize);

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
            sectors: sectors.iter().map(|it| unsafe { it.clone() }).collect(),
        };

        let res = SCellBox::new(&info);
        if res.is_err() {
            for sector in sectors {
                deallocate(sector);
            }
        };

        let info_sbox = res?;

        Ok(Self(info_sbox, PhantomData::default()))
    }

    pub fn push(&mut self, element: &T) -> Result<(), OutOfMemory> {
        let new_sector_opt = self.grow_if_needed()?;

        self.set_len(self.len() + 1);

        let (mut sector, offset) = if let Some(new_sector) = new_sector_opt {
            (new_sector, 0usize)
        } else {
            let (sector, offset) = self.calculate_inner_index(self.len() - 1);
            (sector, offset)
        };

        let bytes_element = unsafe { element.as_bytes() };
        sector._write_bytes(offset, &bytes_element);

        Ok(())
    }

    pub fn pop(&mut self) -> Option<T> {
        let len = self.len();
        if len == 0 {
            return None;
        }

        let idx = len - 1;
        let (sector, offset) = self.calculate_inner_index(idx);

        let mut element_bytes = vec![0u8; size_of::<T>()];
        sector._read_bytes(offset, &mut element_bytes);
        self.set_len(idx);

        unsafe { Some(T::from_bytes(&element_bytes)) }
    }

    fn get(&self, idx: u64) -> Option<T> {
        if idx >= self.len() {
            return None;
        }

        let (sector, offset) = self.calculate_inner_index(idx);

        let mut element_bytes = vec![0u8; size_of::<T>()];
        sector._read_bytes(offset, &mut element_bytes);

        unsafe { Some(T::from_bytes(&element_bytes)) }
    }

    pub fn set(&mut self, idx: u64, element: &T) {
        assert!(idx < self.len(), "Out of bounds");

        let (sector, offset) = self.calculate_inner_index(idx);

        let bytes_element = unsafe { element.as_bytes() };
        sector._write_bytes(offset, &bytes_element);
    }

    pub fn drop(self) {
        let info = self.get_info();

        for i in 0..info.len {
            let (sector, offset) = self.calculate_inner_index(i);

            let ptr = sector._read_word(offset);
            if ptr != 0 {
                let membox = unsafe { RawSCell::<T>::from_ptr(ptr, Side::Start).unwrap() };
                deallocate(membox);
            }
        }

        for sector in info.sectors {
            deallocate(sector);
        }

        self.0.drop();
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

        self.set_info(&info).unwrap();
    }

    pub fn is_about_to_grow(&self) -> bool {
        let info = self.get_info();

        info.len == info.capacity
    }

    fn grow_if_needed(&mut self) -> Result<Option<RawSCell<SVecSector>>, OutOfMemory> {
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

            let sector = allocate(new_sector_size)?;

            info.capacity += (new_sector_size / size_of::<T>()) as u64;
            unsafe {
                info.sectors.push(sector.clone());
            }

            match self.set_info(&info) {
                Err(e) => {
                    deallocate(sector);

                    Err(e)
                }
                Ok(res) => Ok(Some(sector)),
            }
        } else {
            Ok(None)
        }
    }

    fn calculate_inner_index(&self, idx: u64) -> (RawSCell<SVecSector>, usize) {
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
        self.0.get_cloned()
    }

    fn set_info(&mut self, new_info: &StableVecInfo) -> Result<(), OutOfMemory> {
        self.0.set(new_info)
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
                elems.push(format!("{:?}", unsafe { T::from_bytes(&elem_bytes) }));
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
            let element = self.get(i).unwrap();
            elements.push(format!("{}", element));
        }

        write!(f, "[{}]", elements.join(","))
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::vec::{SVec, STABLE_VEC_DEFAULT_CAPACITY};
    use crate::init_allocator;
    use crate::primitive::s_cellbox::SCellBox;
    use crate::utils::mem_context::stable;
    use candid::{CandidType, Deserialize, Nat};

    #[test]
    fn create_destroy_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut stable_vec = SVec::<Test>::new().unwrap();
        assert_eq!(stable_vec.capacity(), STABLE_VEC_DEFAULT_CAPACITY);
        assert_eq!(stable_vec.len(), 0);

        stable_vec.drop();

        stable_vec = SVec::<Test>::new_with_capacity(10_000).unwrap();
        assert_eq!(stable_vec.capacity(), 10_000);
        assert_eq!(stable_vec.len(), 0);

        stable_vec.drop();
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

            stable_vec.set(i, &it);
        }

        assert_eq!(stable_vec.len(), count, "Invalid len after set");

        for i in 0..count {
            let it = stable_vec.get(i).unwrap();

            assert_eq!(it.a, count - i);
            assert_eq!(it.b, i);
        }

        for i in 0..count {
            let it = stable_vec.pop().unwrap();

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

    #[test]
    fn push_pop_indirect_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut stable_vec = SVec::new().unwrap();
        let count = 1000u64;

        for i in 0..count {
            let it = SCellBox::new(&TestIndirect {
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
            let it = SCellBox::new(&TestIndirect {
                a: Nat::from(i),
                b: format!(
                    "Much bigger str that should cause reallocation of the element {}",
                    i
                ),
            })
            .unwrap();

            stable_vec.set(i, &it);
        }

        assert_eq!(stable_vec.len(), count, "Invalid len after push");

        for i in 0..count {
            let it = stable_vec.pop().unwrap().get_cloned();

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
