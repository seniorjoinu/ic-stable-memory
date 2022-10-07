use crate::collections::vec::vec_direct::SVecDirect;
use crate::mem::allocator::EMPTY_PTR;
use crate::mem::s_slice::{SSlice, PTR_SIZE};
use crate::primitive::s_unsafe_cell::SUnsafeCell;
use crate::utils::phantom_data::SPhantomData;
use crate::utils::NotReference;
use crate::{allocate, deallocate};
use speedy::{Readable, Writable};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::mem::size_of;

const STABLE_HASH_MAP_DEFAULT_CAPACITY: u32 = 7993;
type HashMapKeyBucket<K> = SUnsafeCell<SVecDirect<K>>;
type HashMapValueBucket<V> = SUnsafeCell<SVecDirect<V>>;

#[derive(Readable, Writable)]
pub struct SHashMapDirect<K, V> {
    _len: u64,
    _table_capacity: u32,
    _table: Option<SSlice>,
    _k: SPhantomData<K>,
    _v: SPhantomData<V>,
}

impl<K: Copy + NotReference + Hash + Eq, V: Copy + NotReference> Default for SHashMapDirect<K, V>
where
    [(); size_of::<K>()]: Sized,
    [(); size_of::<V>()]: Sized,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Copy + NotReference + Hash + Eq, V: Copy + NotReference> SHashMapDirect<K, V>
where
    [(); size_of::<K>()]: Sized,
    [(); size_of::<V>()]: Sized,
{
    pub fn new() -> Self {
        Self::new_with_capacity(STABLE_HASH_MAP_DEFAULT_CAPACITY)
    }

    pub fn new_with_capacity(capacity: u32) -> Self {
        Self {
            _len: 0,
            _table_capacity: capacity,
            _table: None,
            _k: SPhantomData::new(),
            _v: SPhantomData::new(),
        }
    }

    pub fn insert(&mut self, key: &K, value: &V) -> Option<V> {
        self.init_table();

        let idx = self.find_bucket_idx(key);
        let key_bucket_box_opt = self.read_key_bucket(idx);

        let (mut key_bucket_box, mut key_bucket, mut value_bucket_box, mut value_bucket) =
            if let Some(key_bucket_box) = key_bucket_box_opt {
                let key_bucket = key_bucket_box.get_cloned();

                let value_bucket_box = self.read_value_bucket(idx).unwrap();
                let value_bucket = value_bucket_box.get_cloned();

                (key_bucket_box, key_bucket, value_bucket_box, value_bucket)
            } else {
                let key_bucket = SVecDirect::<K>::new();
                let key_bucket_box = HashMapKeyBucket::<K>::new(&key_bucket);

                self.set_key_bucket(idx, &key_bucket_box);

                let value_bucket = SVecDirect::<V>::new();
                let value_bucket_box = HashMapValueBucket::<V>::new(&value_bucket);

                self.set_value_bucket(idx, &value_bucket_box);

                (key_bucket_box, key_bucket, value_bucket_box, value_bucket)
            };

        let mut found = false;
        let mut prev = None;

        for i in 0..key_bucket.len() {
            let prev_entry_key = key_bucket.get_cloned(i).unwrap();

            if prev_entry_key.eq(key) {
                prev = Some(value_bucket.replace(i, value));

                found = true;
                break;
            }
        }

        if !found {
            key_bucket.push(key);
            value_bucket.push(value);

            self._len += 1;

            unsafe {
                let should_update = key_bucket_box.set(&key_bucket);

                if should_update {
                    self.set_key_bucket(idx, &key_bucket_box);
                }

                let should_update = value_bucket_box.set(&value_bucket);

                if should_update {
                    self.set_value_bucket(idx, &value_bucket_box);
                }
            }
        }

        prev
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        if self.is_empty() {
            return None;
        }

        let idx = self.find_bucket_idx(key);
        let mut key_bucket_box = self.read_key_bucket(idx)?;
        let mut key_bucket = key_bucket_box.get_cloned();

        let mut value_bucket_box_opt = None;
        let mut value_bucket_opt = None;
        let mut value_opt = None;

        for i in 0..key_bucket.len() {
            let elem_key = key_bucket.get_cloned(i).unwrap();

            if elem_key.eq(key) {
                let value_bucket_box = self.read_value_bucket(idx).unwrap();
                let mut value_bucket = value_bucket_box.get_cloned();

                if i < key_bucket.len() - 1 {
                    key_bucket.swap(i, key_bucket.len() - 1);
                    value_bucket.swap(i, value_bucket.len() - 1);
                }

                key_bucket.pop().unwrap();

                value_opt = Some(value_bucket.pop().unwrap());
                value_bucket_box_opt = Some(value_bucket_box);
                value_bucket_opt = Some(value_bucket);

                self._len -= 1;
                break;
            }
        }

        if value_opt.is_some() {
            unsafe {
                let should_update = key_bucket_box.set(&key_bucket);

                // yea, this won't trigger with current vec's implementation
                if should_update {
                    self.set_key_bucket(idx, &key_bucket_box);
                }

                let mut value_bucket_box = value_bucket_box_opt.unwrap();
                let should_update = value_bucket_box.set(&value_bucket_opt.unwrap());

                // yea, this won't trigger with current vec's implementation
                if should_update {
                    self.set_value_bucket(idx, &value_bucket_box);
                }
            }
        }

        value_opt
    }

    // TODO: optimize - it can do a little less work than get_cloned()
    pub fn contains_key(&self, key: &K) -> bool {
        self.get_cloned(key).is_some()
    }

    pub fn get_cloned(&self, key: &K) -> Option<V> {
        if self.is_empty() {
            return None;
        }

        let idx = self.find_bucket_idx(key);
        let key_bucket = self.read_key_bucket(idx)?.get_cloned();

        for i in 0..key_bucket.len() {
            let elem_key = key_bucket.get_cloned(i).unwrap();

            if elem_key.eq(key) {
                let value_bucket = self.read_value_bucket(idx).unwrap().get_cloned();

                return Some(value_bucket.get_cloned(i).unwrap());
            }
        }

        None
    }

    pub fn len(&self) -> u64 {
        self._len
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn drop(self) {
        if self._table.is_none() {
            return;
        }

        for i in 0..self._table_capacity {
            let bucket_box_opt = self.read_key_bucket(i as usize);
            if let Some(bb) = bucket_box_opt {
                bb.drop();
            }

            let bucket_box_opt = self.read_value_bucket(i as usize);
            if let Some(bb) = bucket_box_opt {
                bb.drop();
            }
        }

        deallocate(self.table());
    }

    fn init_table(&mut self) {
        if self._table.is_none() {
            let capacity_bytes = self._table_capacity as usize * PTR_SIZE * 2;
            let table = allocate(capacity_bytes);

            // we have to initialize this memory
            table.write_bytes(0, &vec![0u8; table.get_size_bytes()]);

            self._table = Some(table);
        }
    }

    fn find_bucket_idx(&self, key: &K) -> usize {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let hash = hasher.finish();

        (hash % self._table_capacity as u64) as usize
    }

    fn set_key_bucket(&mut self, idx: usize, bucket_value: &HashMapKeyBucket<K>) {
        let offset = idx * PTR_SIZE * 2;
        self.table()
            .write_word(offset, unsafe { bucket_value.as_ptr() });
    }

    fn set_value_bucket(&mut self, idx: usize, bucket_value: &HashMapValueBucket<V>) {
        let offset = idx * PTR_SIZE * 2 + PTR_SIZE;
        self.table()
            .write_word(offset, unsafe { bucket_value.as_ptr() });
    }

    fn read_key_bucket(&self, idx: usize) -> Option<HashMapKeyBucket<K>> {
        let offset = idx * PTR_SIZE * 2;
        let ptr = self.table().read_word(offset);

        if ptr == 0 || ptr == EMPTY_PTR {
            None
        } else {
            Some(unsafe { HashMapKeyBucket::<K>::from_ptr(ptr) })
        }
    }

    fn read_value_bucket(&self, idx: usize) -> Option<HashMapValueBucket<V>> {
        let offset = idx * PTR_SIZE * 2 + PTR_SIZE;
        let ptr = self.table().read_word(offset);

        if ptr == 0 || ptr == EMPTY_PTR {
            None
        } else {
            Some(unsafe { HashMapValueBucket::<V>::from_ptr(ptr) })
        }
    }

    fn table(&self) -> SSlice {
        *self._table.as_ref().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::hash_map::hash_map_direct::SHashMapDirect;
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

        let mut map = SHashMapDirect::<u64, u64>::default();

        assert!(map.remove(&10).is_none());
        assert!(map.get_cloned(&10).is_none());

        assert!(map.insert(&1, &1).is_none());
        assert!(map.insert(&2, &2).is_none());
        assert!(map.insert(&3, &3).is_none());
        assert_eq!(map.insert(&1, &10).unwrap(), 1);

        assert!(map.remove(&5).is_none());
        assert_eq!(map.remove(&1).unwrap(), 10);

        assert!(map.contains_key(&2));
        assert!(!map.contains_key(&5));

        map.drop();

        let mut map = SHashMapDirect::<u64, u64>::new_with_capacity(3);
        for i in 0..100 {
            map.insert(&i, &i);
        }

        for i in 0..100 {
            assert_eq!(map.remove(&(99 - i)).unwrap(), 99 - i);
        }

        map.drop();
    }
}
