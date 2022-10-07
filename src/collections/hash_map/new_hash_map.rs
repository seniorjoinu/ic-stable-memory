use crate::utils::phantom_data::SPhantomData;
use crate::utils::{any_as_u8_slice, u8_fixed_array_as_any, NotReference};
use crate::{allocate, deallocate, SSlice};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::mem::size_of;

const LOAD_FACTOR: f64 = 0.75;
const DEFAULT_CAPACITY: usize = 5;

const EMPTY: u8 = 0;
const OCCUPIED: u8 = 1;
const TOMBSTONE: u8 = 255;

// reallocating, open addressing, quadratic probing
pub struct SHashMapDirect<K, V>
where
    [(); size_of::<K>()]: Sized,
    [(); size_of::<V>()]: Sized,
{
    len: usize,
    capacity: usize,
    table: Option<SSlice>,
    _marker_k: SPhantomData<K>,
    _marker_v: SPhantomData<V>,
}

impl<K: Copy + NotReference + Eq + Hash, V: Copy + NotReference> SHashMapDirect<K, V>
where
    [(); size_of::<K>()]: Sized,
    [(); size_of::<V>()]: Sized,
{
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

    pub fn insert(&mut self, key: &K, value: &V) -> Option<V> {
        self.maybe_reallocate();

        let mut prev = None;
        let key_hash = self.hash(*key) as usize;
        let mut i = 0;

        let table = self.table.as_ref().unwrap();

        let mut remembered_at = None;

        loop {
            let at = (key_hash + i * i) % self.capacity;

            i += 1;

            match Self::read_key_at(table, at, true) {
                HashMapKey::Occupied(prev_key) => {
                    if prev_key.eq(key) {
                        prev = Some(Self::read_val_at(table, at));
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
        let key_hash = self.hash(*key) as usize;
        let mut i = 0;

        let table = self.table.as_ref().unwrap();

        loop {
            let at = (key_hash + i * i) % self.capacity;
            i += 1;

            match Self::read_key_at(table, at, true) {
                HashMapKey::Occupied(prev_key) => {
                    if prev_key.eq(key) {
                        prev = Some(Self::read_val_at(table, at));
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

    pub fn get_cloned(&self, key: &K) -> Option<V> {
        self.table?;

        let mut prev = None;
        let key_hash = self.hash(*key) as usize;
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

        let key_hash = self.hash(*key) as usize;
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

    pub fn drop(self) {
        if let Some(slice) = self.table {
            deallocate(slice);
        }
    }

    fn hash<T: Hash>(&self, val: T) -> u64 {
        let mut hasher = DefaultHasher::new();
        val.hash(&mut hasher);

        hasher.finish()
    }

    fn read_key_at(slice: &SSlice, idx: usize, read_value: bool) -> HashMapKey<K> {
        let mut key_flag = [0u8];
        let at = Self::to_offset_or_size(idx);

        slice.read_bytes(at, &mut key_flag);

        match key_flag[0] {
            EMPTY => HashMapKey::Empty,
            TOMBSTONE => HashMapKey::Tombstone,
            OCCUPIED => {
                if read_value {
                    let mut key_at_idx = [0u8; size_of::<K>()];
                    slice.read_bytes(at + 1, &mut key_at_idx);

                    HashMapKey::Occupied(unsafe { u8_fixed_array_as_any(key_at_idx) })
                } else {
                    HashMapKey::OccupiedNull
                }
            }
            _ => unreachable!(),
        }
    }

    fn read_val_at(slice: &SSlice, idx: usize) -> V {
        let at = Self::to_offset_or_size(idx) + 1 + size_of::<K>();

        let mut val_at_idx = [0u8; size_of::<V>()];
        slice.read_bytes(at, &mut val_at_idx);

        unsafe { u8_fixed_array_as_any(val_at_idx) }
    }

    fn write_key_at(slice: &SSlice, idx: usize, key: HashMapKey<&K>) {
        let at = Self::to_offset_or_size(idx);

        let key_flag = match key {
            HashMapKey::Empty => [EMPTY],
            HashMapKey::Tombstone => [TOMBSTONE],
            HashMapKey::Occupied(k) => {
                let key_bytes = unsafe { any_as_u8_slice(k) };
                slice.write_bytes(at + 1, key_bytes);

                [OCCUPIED]
            }
            _ => unreachable!(),
        };

        slice.write_bytes(at, &key_flag);
    }

    fn write_val_at(slice: &SSlice, idx: usize, val: &V) {
        let at = Self::to_offset_or_size(idx) + 1 + size_of::<K>();
        let val_bytes = unsafe { any_as_u8_slice(val) };

        slice.write_bytes(at, val_bytes);
    }

    fn to_offset_or_size(idx: usize) -> usize {
        idx * (1 + size_of::<K>() + size_of::<V>())
    }

    fn maybe_reallocate(&mut self) {
        if !self.is_about_to_grow() {
            return;
        }

        if let Some(old_table) = self.table {
            let new_capacity = self.capacity * 2 + 1;

            let new_table = allocate(Self::to_offset_or_size(new_capacity));
            new_table.write_bytes(0, &vec![0u8; new_table.get_size_bytes()]);

            for idx in 0..self.capacity {
                let k = Self::read_key_at(&old_table, idx, true);
                if matches!(k, HashMapKey::Empty | HashMapKey::Tombstone) {
                    continue;
                }

                let key = k.unwrap();
                let val = Self::read_val_at(&old_table, idx);
                let key_hash = self.hash(key) as usize;

                let mut i = 0;

                loop {
                    let at = (key_hash + i * i) % new_capacity as usize;

                    i += 1;

                    match Self::read_key_at(&new_table, at, false) {
                        HashMapKey::OccupiedNull => {
                            continue;
                        }
                        HashMapKey::Empty => {
                            Self::write_key_at(&new_table, at, HashMapKey::Occupied(&key));
                            Self::write_val_at(&new_table, at, &val);

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
            let slice = allocate(Self::to_offset_or_size(self.capacity));
            slice.write_bytes(0, &vec![0u8; slice.get_size_bytes()]);

            self.table = Some(slice)
        }
    }

    fn is_about_to_grow(&self) -> bool {
        // TODO: optimize - can be calculated once
        self.table.is_none() || self.len as f64 > (self.capacity as f64) * LOAD_FACTOR
    }
}

impl<K: Copy + NotReference + Eq + Hash, V: Copy + NotReference> Default for SHashMapDirect<K, V>
where
    [(); size_of::<K>()]: Sized,
    [(); size_of::<Option<K>>()]: Sized,
    [(); size_of::<V>()]: Sized,
    [(); size_of::<Option<V>>()]: Sized,
{
    fn default() -> Self {
        Self::new()
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

#[cfg(test)]
mod tests {
    use crate::collections::hash_map::new_hash_map::SHashMapDirect;
    use crate::init_allocator;
    use crate::utils::mem_context::stable;

    fn test_body(mut map: SHashMapDirect<i32, i32>) {
        let k1 = 1;
        let k2 = 2;
        let k3 = 3;
        let k4 = 4;
        let k5 = 5;
        let k6 = 6;
        let k7 = 7;
        let k8 = 8;

        map.insert(&k1, &1);
        map.insert(&k2, &2);
        map.insert(&k3, &3);
        map.insert(&k4, &4);
        map.insert(&k5, &5);
        map.insert(&k6, &6);
        map.insert(&k7, &7);
        map.insert(&k8, &8);

        assert_eq!(map.get_cloned(&k1).unwrap(), 1);
        assert_eq!(map.get_cloned(&k2).unwrap(), 2);
        assert_eq!(map.get_cloned(&k3).unwrap(), 3);
        assert_eq!(map.get_cloned(&k4).unwrap(), 4);
        assert_eq!(map.get_cloned(&k5).unwrap(), 5);
        assert_eq!(map.get_cloned(&k6).unwrap(), 6);
        assert_eq!(map.get_cloned(&k7).unwrap(), 7);
        assert_eq!(map.get_cloned(&k8).unwrap(), 8);

        assert!(map.get_cloned(&9).is_none());
        assert!(map.get_cloned(&0).is_none());

        assert_eq!(map.remove(&k3).unwrap(), 3);
        assert!(map.get_cloned(&k3).is_none());

        assert_eq!(map.remove(&k1).unwrap(), 1);
        assert!(map.get_cloned(&k1).is_none());

        assert_eq!(map.remove(&k5).unwrap(), 5);
        assert!(map.get_cloned(&k5).is_none());

        assert_eq!(map.remove(&k7).unwrap(), 7);
        assert!(map.get_cloned(&k7).is_none());

        assert_eq!(map.get_cloned(&k2).unwrap(), 2);
        assert_eq!(map.get_cloned(&k4).unwrap(), 4);
        assert_eq!(map.get_cloned(&k6).unwrap(), 6);
        assert_eq!(map.get_cloned(&k8).unwrap(), 8);

        map.drop();
    }

    #[test]
    fn simple_flow_works_well_for_big() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let map = SHashMapDirect::new();
        test_body(map);
    }

    #[test]
    fn simple_flow_works_well_for_small() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let map = SHashMapDirect::new_with_capacity(3);
        test_body(map);
    }

    #[test]
    fn basic_flow_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SHashMapDirect::<u64, u64>::new_with_capacity(7773);

        assert!(map.remove(&10).is_none());
        assert!(map.get_cloned(&10).is_none());

        let it = map.insert(&1, &1);
        assert!(it.is_none());
        assert!(map.insert(&2, &2).is_none());
        assert!(map.insert(&3, &3).is_none());
        assert_eq!(map.insert(&1, &10).unwrap(), 1);

        assert!(map.remove(&5).is_none());
        assert_eq!(map.remove(&1).unwrap(), 10);

        assert!(map.contains_key(&2));
        assert!(!map.contains_key(&5));

        map.drop();

        let mut map = SHashMapDirect::<u64, u64>::default();
        for i in 0..100 {
            assert!(map.insert(&i, &i).is_none());
        }

        for i in 0..100 {
            assert_eq!(map.get_cloned(&i).unwrap(), i);
        }

        for i in 0..100 {
            assert_eq!(map.remove(&(99 - i)).unwrap(), 99 - i);
        }

        map.drop();
    }
}
