use crate::collections::hash_map::iter::SHashMapIter;
use crate::mem::allocator::EMPTY_PTR;
use crate::mem::s_slice::Side;
use crate::primitive::StableAllocated;
use crate::utils::phantom_data::SPhantomData;
use crate::{allocate, deallocate, SSlice};
use copy_as_bytes::traits::{AsBytes, SuperSized};
use speedy::{Context, LittleEndian, Readable, Reader, Writable, Writer};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::mem::size_of;

pub mod iter;

const LOAD_FACTOR: f64 = 0.75;
const DEFAULT_CAPACITY: usize = 5;

const EMPTY: u8 = 0;
const OCCUPIED: u8 = 1;
const TOMBSTONE: u8 = 255;

// reallocating, open addressing, quadratic probing
pub struct SHashMap<K, V> {
    pub(crate) len: usize,
    pub(crate) capacity: usize,
    pub(crate) table: Option<SSlice>,
    _marker_k: SPhantomData<K>,
    _marker_v: SPhantomData<V>,
}

impl<K, V> SHashMap<K, V> {
    #[inline]
    pub fn new() -> Self {
        Self::new_with_capacity(DEFAULT_CAPACITY)
    }

    pub fn new_with_capacity(capacity: usize) -> Self {
        Self {
            len: 0,
            capacity,
            table: None,
            _marker_k: SPhantomData::default(),
            _marker_v: SPhantomData::default(),
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub unsafe fn stable_drop_collection(&mut self) {
        if let Some(slice) = self.table {
            deallocate(slice);
            self.table = None;
        }
    }

    fn hash<T: Hash>(&self, val: &T) -> u64 {
        let mut hasher = DefaultHasher::new();
        val.hash(&mut hasher);

        hasher.finish()
    }

    fn to_offset_or_size(idx: usize, size_k: usize, size_v: usize) -> usize {
        idx * (1 + size_k + size_v)
    }

    fn is_about_to_grow(&self) -> bool {
        // TODO: optimize - can be calculated once at each resize
        self.table.is_none() || self.len as f64 > (self.capacity as f64) * LOAD_FACTOR
    }
}

impl<K: StableAllocated + Hash + Eq, V: StableAllocated> SHashMap<K, V>
where
    [u8; K::SIZE]: Sized,
    [u8; V::SIZE]: Sized,
{
    pub fn insert(&mut self, mut key: K, mut value: V) -> Option<V> {
        self.maybe_reallocate();

        let mut prev = None;
        let key_hash = self.hash(&key) as usize;
        let mut i = 0;

        let table = self.table.as_ref().unwrap();

        let mut remembered_at = None;

        loop {
            let at = (key_hash + i * i) % self.capacity;

            i += 1;

            match Self::read_key_at(table, at, true) {
                HashMapKey::Occupied(prev_key) => {
                    if prev_key.eq(&key) {
                        let mut prev_value = Self::read_val_at(table, at);
                        prev_value.remove_from_stable();

                        prev = Some(prev_value);

                        value.move_to_stable();
                        Self::write_val_at(table, at, value);

                        break;
                    } else {
                        continue;
                    }
                }
                HashMapKey::Tombstone => {
                    if remembered_at.is_none() {
                        remembered_at = Some(at);
                    }
                    continue;
                }
                HashMapKey::Empty => {
                    let at = if let Some(a) = remembered_at { a } else { at };

                    key.move_to_stable();
                    value.move_to_stable();

                    Self::write_key_at(table, at, HashMapKey::Occupied(key));
                    Self::write_val_at(table, at, value);

                    self.len += 1;

                    break;
                }
                _ => unreachable!(),
            }
        }

        prev
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.table?;

        let mut prev = None;
        let key_hash = self.hash(key) as usize;
        let mut i = 0;

        let table = self.table.as_ref().unwrap();

        loop {
            let at = (key_hash + i * i) % self.capacity;
            i += 1;

            match Self::read_key_at(table, at, true) {
                HashMapKey::Occupied(mut prev_key) => {
                    if prev_key.eq(key) {
                        let mut prev_value = Self::read_val_at(table, at);

                        prev_key.remove_from_stable();
                        prev_value.remove_from_stable();

                        prev = Some(prev_value);
                        Self::write_key_at(table, at, HashMapKey::Tombstone);

                        self.len -= 1;

                        break;
                    } else {
                        continue;
                    }
                }
                HashMapKey::Tombstone => {
                    continue;
                }
                HashMapKey::Empty => {
                    break;
                }
                _ => unreachable!(),
            }
        }

        prev
    }

    pub fn get_copy(&self, key: &K) -> Option<V> {
        self.table?;

        let mut prev = None;
        let key_hash = self.hash(key) as usize;
        let mut i = 0;

        let table = self.table.as_ref().unwrap();

        loop {
            let at = (key_hash + i * i) % self.capacity;
            i += 1;

            match Self::read_key_at(table, at, true) {
                HashMapKey::Occupied(prev_key) => {
                    if prev_key.eq(key) {
                        prev = Some(Self::read_val_at(table, at));

                        break;
                    } else {
                        continue;
                    }
                }
                HashMapKey::Tombstone => {
                    continue;
                }
                HashMapKey::Empty => {
                    break;
                }
                _ => unreachable!(),
            }
        }

        prev
    }

    pub fn contains_key(&self, key: &K) -> bool {
        if self.table.is_none() {
            return false;
        }

        let key_hash = self.hash(key) as usize;
        let mut i = 0;

        let table = self.table.as_ref().unwrap();

        loop {
            let at = (key_hash + i * i) % self.capacity;
            i += 1;

            match Self::read_key_at(table, at, true) {
                HashMapKey::Occupied(prev_key) => {
                    if prev_key.eq(key) {
                        return true;
                    } else {
                        continue;
                    }
                }
                HashMapKey::Tombstone => {
                    continue;
                }
                HashMapKey::Empty => {
                    break;
                }
                _ => unreachable!(),
            }
        }

        false
    }

    pub fn iter(&self) -> SHashMapIter<K, V> {
        SHashMapIter::new(self)
    }

    fn read_key_at(slice: &SSlice, idx: usize, read_value: bool) -> HashMapKey<K> {
        let mut key_flag = [0u8];
        let at = Self::to_offset_or_size(idx, size_of::<K>(), size_of::<V>());

        slice.read_bytes(at, &mut key_flag);

        match key_flag[0] {
            EMPTY => HashMapKey::Empty,
            TOMBSTONE => HashMapKey::Tombstone,
            OCCUPIED => {
                if read_value {
                    let mut key_at_idx = K::super_size_u8_arr();
                    slice.read_bytes(at + 1, &mut key_at_idx);

                    HashMapKey::Occupied(K::from_bytes(key_at_idx))
                } else {
                    HashMapKey::OccupiedNull
                }
            }
            _ => unreachable!(),
        }
    }

    fn read_val_at(slice: &SSlice, idx: usize) -> V {
        let at = Self::to_offset_or_size(idx, size_of::<K>(), size_of::<V>()) + 1 + size_of::<K>();

        let mut val_at_idx = V::super_size_u8_arr();
        slice.read_bytes(at, &mut val_at_idx);

        V::from_bytes(val_at_idx)
    }

    fn write_key_at(slice: &SSlice, idx: usize, key: HashMapKey<K>) {
        let at = Self::to_offset_or_size(idx, size_of::<K>(), size_of::<V>());

        let key_flag = match key {
            HashMapKey::Empty => [EMPTY],
            HashMapKey::Tombstone => [TOMBSTONE],
            HashMapKey::Occupied(k) => {
                let key_bytes = k.to_bytes();
                slice.write_bytes(at + 1, &key_bytes);

                [OCCUPIED]
            }
            _ => unreachable!(),
        };

        slice.write_bytes(at, &key_flag);
    }

    fn write_val_at(slice: &SSlice, idx: usize, val: V) {
        let at = Self::to_offset_or_size(idx, size_of::<K>(), size_of::<V>()) + 1 + size_of::<K>();
        let val_bytes = val.to_bytes();

        slice.write_bytes(at, &val_bytes);
    }

    fn maybe_reallocate(&mut self) {
        if !self.is_about_to_grow() {
            return;
        }

        if let Some(old_table) = self.table {
            let new_capacity = self.capacity * 2 + 1;

            let new_table = allocate(Self::to_offset_or_size(
                new_capacity,
                size_of::<K>(),
                size_of::<V>(),
            ));
            new_table.write_bytes(0, &vec![0u8; new_table.get_size_bytes()]);

            for idx in 0..self.capacity {
                let k = Self::read_key_at(&old_table, idx, true);
                if matches!(k, HashMapKey::Empty | HashMapKey::Tombstone) {
                    continue;
                }

                let key = k.unwrap();
                let val = Self::read_val_at(&old_table, idx);
                let key_hash = self.hash(&key) as usize;

                let mut i = 0;

                loop {
                    let at = (key_hash + i * i) % new_capacity as usize;

                    i += 1;

                    match Self::read_key_at(&new_table, at, false) {
                        HashMapKey::OccupiedNull => {
                            continue;
                        }
                        HashMapKey::Empty => {
                            Self::write_key_at(&new_table, at, HashMapKey::Occupied(key));
                            Self::write_val_at(&new_table, at, val);

                            break;
                        }
                        _ => unreachable!(),
                    }
                }
            }

            self.capacity = new_capacity;
            self.table = Some(new_table);

            deallocate(old_table);
        } else {
            let slice = allocate(Self::to_offset_or_size(
                self.capacity,
                size_of::<K>(),
                size_of::<V>(),
            ));
            slice.write_bytes(0, &vec![0u8; slice.get_size_bytes()]);

            self.table = Some(slice)
        }
    }
}

impl<K, V> Default for SHashMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, K, V> Readable<'a, LittleEndian> for SHashMap<K, V> {
    fn read_from<R: Reader<'a, LittleEndian>>(
        reader: &mut R,
    ) -> Result<Self, <speedy::LittleEndian as Context>::Error> {
        let ptr = reader.read_u64()?;
        let len = reader.read_u32()? as usize;
        let capacity = reader.read_u32()? as usize;

        let table = if ptr == EMPTY_PTR {
            None
        } else {
            SSlice::from_ptr(ptr, Side::Start)
        };

        Ok(Self {
            len,
            capacity,
            table,
            _marker_k: SPhantomData::default(),
            _marker_v: SPhantomData::default(),
        })
    }
}

impl<K, V> Writable<LittleEndian> for SHashMap<K, V> {
    fn write_to<T: ?Sized + Writer<LittleEndian>>(
        &self,
        writer: &mut T,
    ) -> Result<(), <speedy::LittleEndian as Context>::Error> {
        if let Some(slice) = self.table {
            writer.write_u64(slice.get_ptr())?;
        } else {
            writer.write_u64(EMPTY_PTR)?;
        }

        writer.write_u32(self.len as u32)?;
        writer.write_u32(self.capacity as u32)
    }
}

enum HashMapKey<K> {
    Empty,
    Tombstone,
    Occupied(K),
    OccupiedNull,
}

impl<K> HashMapKey<K> {
    fn unwrap(self) -> K {
        match self {
            HashMapKey::Occupied(k) => k,
            _ => unreachable!(),
        }
    }
}

impl<K, V> SuperSized for SHashMap<K, V> {
    const SIZE: usize = usize::SIZE * 2 + u64::SIZE;
}

impl<K, V> AsBytes for SHashMap<K, V> {
    fn to_bytes(self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[..usize::SIZE].copy_from_slice(&self.len.to_bytes());
        buf[usize::SIZE..(usize::SIZE * 2)].copy_from_slice(&self.capacity.to_bytes());
        buf[(usize::SIZE * 2)..(usize::SIZE * 2 + u64::SIZE)].copy_from_slice(
            &self
                .table
                .map(|it| it.get_ptr())
                .unwrap_or(EMPTY_PTR)
                .to_bytes(),
        );

        buf
    }

    fn from_bytes(arr: [u8; Self::SIZE]) -> Self {
        let mut len_buf = [0u8; usize::SIZE];
        let mut cap_buf = [0u8; usize::SIZE];
        let mut ptr_buf = [0u8; u64::SIZE];

        len_buf.copy_from_slice(&arr[..usize::SIZE]);
        cap_buf.copy_from_slice(&arr[usize::SIZE..(usize::SIZE * 2)]);
        ptr_buf.copy_from_slice(&arr[(usize::SIZE * 2)..(usize::SIZE * 2 + u64::SIZE)]);

        let table_ptr = u64::from_bytes(ptr_buf);
        let table = if table_ptr == EMPTY_PTR {
            None
        } else {
            Some(SSlice::from_ptr(table_ptr, Side::Start).unwrap())
        };

        Self {
            len: usize::from_bytes(len_buf),
            capacity: usize::from_bytes(cap_buf),
            table,
            _marker_k: SPhantomData::default(),
            _marker_v: SPhantomData::default(),
        }
    }
}

impl<K: StableAllocated + Eq + Hash, V: StableAllocated> StableAllocated for SHashMap<K, V>
where
    [u8; K::SIZE]: Sized,
    [u8; V::SIZE]: Sized,
{
    #[inline]
    fn move_to_stable(&mut self) {}

    #[inline]
    fn remove_from_stable(&mut self) {}

    unsafe fn stable_drop(mut self) {
        for (k, v) in self.iter() {
            k.stable_drop();
            v.stable_drop();
        }

        self.stable_drop_collection();
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::hash_map::SHashMap;
    use crate::init_allocator;
    use crate::primitive::StableAllocated;
    use crate::utils::mem_context::stable;
    use copy_as_bytes::traits::AsBytes;
    use speedy::{Readable, Writable};

    #[test]
    fn simple_flow_works_well() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SHashMap::new_with_capacity(3);

        let k1 = 1;
        let k2 = 2;
        let k3 = 3;
        let k4 = 4;
        let k5 = 5;
        let k6 = 6;
        let k7 = 7;
        let k8 = 8;

        map.insert(k1, 1);
        map.insert(k2, 2);
        map.insert(k3, 3);
        map.insert(k4, 4);
        map.insert(k5, 5);
        map.insert(k6, 6);
        map.insert(k7, 7);
        map.insert(k8, 8);

        assert_eq!(map.get_copy(&k1).unwrap(), 1);
        assert_eq!(map.get_copy(&k2).unwrap(), 2);
        assert_eq!(map.get_copy(&k3).unwrap(), 3);
        assert_eq!(map.get_copy(&k4).unwrap(), 4);
        assert_eq!(map.get_copy(&k5).unwrap(), 5);
        assert_eq!(map.get_copy(&k6).unwrap(), 6);
        assert_eq!(map.get_copy(&k7).unwrap(), 7);
        assert_eq!(map.get_copy(&k8).unwrap(), 8);

        assert!(map.get_copy(&9).is_none());
        assert!(map.get_copy(&0).is_none());

        assert_eq!(map.remove(&k3).unwrap(), 3);
        assert!(map.get_copy(&k3).is_none());

        assert_eq!(map.remove(&k1).unwrap(), 1);
        assert!(map.get_copy(&k1).is_none());

        assert_eq!(map.remove(&k5).unwrap(), 5);
        assert!(map.get_copy(&k5).is_none());

        assert_eq!(map.remove(&k7).unwrap(), 7);
        assert!(map.get_copy(&k7).is_none());

        assert_eq!(map.get_copy(&k2).unwrap(), 2);
        assert_eq!(map.get_copy(&k4).unwrap(), 4);
        assert_eq!(map.get_copy(&k6).unwrap(), 6);
        assert_eq!(map.get_copy(&k8).unwrap(), 8);

        unsafe { map.stable_drop() };
    }

    #[test]
    fn basic_flow_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SHashMap::new_with_capacity(3);

        assert!(map.remove(&10).is_none());
        assert!(map.get_copy(&10).is_none());

        let it = map.insert(1, 1);
        assert!(it.is_none());
        assert!(map.insert(2, 2).is_none());
        assert!(map.insert(3, 3).is_none());
        assert_eq!(map.insert(1, 10).unwrap(), 1);

        assert!(map.remove(&5).is_none());
        assert_eq!(map.remove(&1).unwrap(), 10);

        assert!(map.contains_key(&2));
        assert!(!map.contains_key(&5));

        unsafe { map.stable_drop() };

        let mut map = SHashMap::default();
        for i in 0..100 {
            assert!(map.insert(i, i).is_none());
        }

        for i in 0..100 {
            assert_eq!(map.get_copy(&i).unwrap(), i);
        }

        for i in 0..100 {
            assert_eq!(map.remove(&(99 - i)).unwrap(), 99 - i);
        }

        unsafe { map.stable_drop() };
    }

    #[test]
    fn tombstones_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SHashMap::new();

        for i in 0..100 {
            map.insert(i, i);
        }

        assert_eq!(map.len(), 100);

        for i in 0..50 {
            map.remove(&i);
        }

        assert_eq!(map.len(), 50);

        for i in 0..50 {
            map.insert(i, i);
        }

        assert_eq!(map.len(), 100);

        unsafe { map.stable_drop() };
    }

    #[test]
    fn serialization_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SHashMap::new();
        map.insert(0, 0);

        let buf = map.write_to_vec().unwrap();
        let map1 = SHashMap::<i32, i32>::read_from_buffer_copying_data(&buf).unwrap();

        assert_eq!(map.len, map1.len);
        assert_eq!(map.capacity, map1.capacity);
        assert_eq!(map.table.unwrap().get_ptr(), map1.table.unwrap().get_ptr());

        let len = map.len;
        let cap = map.capacity;
        let ptr = map.table.unwrap().get_ptr();

        let buf = map.to_bytes();
        let map1 = SHashMap::<i32, i32>::from_bytes(buf);

        assert_eq!(len, map1.len);
        assert_eq!(cap, map1.capacity);
        assert_eq!(ptr, map1.table.unwrap().get_ptr());
    }

    #[test]
    fn iter_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SHashMap::new();
        for i in 0..100 {
            map.insert(i, i);
        }

        let mut c = 0;
        for (k, v) in map.iter() {
            c += 1;

            assert!(k < 100);
        }

        assert_eq!(c, 100);
    }
}
