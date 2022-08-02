use crate::collections::vec::SVec;
use crate::mem::allocator::EMPTY_PTR;
use crate::primitive::s_slice::PTR_SIZE;
use crate::primitive::s_unsafe_cell::SUnsafeCell;
use crate::utils::phantom_data::SPhantomData;
use crate::{allocate, deallocate, SSlice};
use speedy::{LittleEndian, Readable, Writable};
use std::collections::hash_map::DefaultHasher;
use std::fmt::Debug;
use std::hash::{Hash, Hasher};

// TODO: make entry store value more efficiently
// FIXME: there is an endless loop somewhere, run benchmarks with a lot of iterations to spot it

const STABLE_HASH_MAP_DEFAULT_CAPACITY: u32 = 9973;
type HashMapBucket<K, V> = SUnsafeCell<SVec<HashMapEntry<K, V>>>;

#[derive(Readable, Writable, Debug)]
struct HashMapEntry<K, V> {
    key: K,
    val: V,
}

impl<K, V> HashMapEntry<K, V> {
    pub fn new(k: K, v: V) -> Self {
        Self { key: k, val: v }
    }
}

#[derive(Copy, Clone)]
struct SMapTable;

#[derive(Readable, Writable)]
struct SHashMapInfo {
    _len: u64,
    _table_capacity: u32,
    _table: Option<SSlice<SMapTable>>,
}

#[derive(Readable, Writable)]
pub struct SHashMap<K, V> {
    _info: SHashMapInfo,
    _k: SPhantomData<K>,
    _v: SPhantomData<V>,
}

impl<
        'a,
        K: Hash + Eq + Readable<'a, LittleEndian> + Writable<LittleEndian>,
        V: Readable<'a, LittleEndian> + Writable<LittleEndian>,
    > Default for SHashMap<K, V>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<
        'a,
        K: Hash + Eq + Readable<'a, LittleEndian> + Writable<LittleEndian>,
        V: Readable<'a, LittleEndian> + Writable<LittleEndian>,
    > SHashMap<K, V>
{
    pub fn new() -> Self {
        Self::new_with_capacity(STABLE_HASH_MAP_DEFAULT_CAPACITY)
    }

    pub fn new_with_capacity(capacity: u32) -> Self {
        let _info = SHashMapInfo {
            _len: 0,
            _table_capacity: capacity,
            _table: None,
        };

        Self {
            _info,
            _k: SPhantomData::default(),
            _v: SPhantomData::default(),
        }
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.init_table();

        let idx = self.find_bucket_idx(&key);
        let (offset, bucket_box_opt) = self.read_bucket(idx);

        let (mut bucket_box, mut bucket) = if let Some(bb) = bucket_box_opt {
            let bucket = bb.get_cloned();

            (bb, bucket)
        } else {
            let bucket = SVec::<HashMapEntry<K, V>>::new();
            let bb = HashMapBucket::<K, V>::new(&bucket);

            self.table()._write_word(offset, unsafe { bb.as_ptr() });

            (bb, bucket)
        };

        let mut found = false;
        let mut prev = None;

        let new_entry = HashMapEntry::new(key, value);

        for i in 0..bucket.len() {
            let prev_entry = bucket.get_cloned(i).unwrap();

            if prev_entry.key.eq(&new_entry.key) {
                bucket.replace(i, &new_entry);
                prev = Some(prev_entry.val);
                found = true;
                break;
            }
        }

        if !found {
            bucket.push(&new_entry);
        }

        self._info._len += 1;

        unsafe {
            let should_update = bucket_box.set(&bucket);

            if should_update {
                self.set_bucket(idx, &bucket_box);
            }
        }

        prev
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        if self.is_empty() {
            return None;
        }

        let idx = self.find_bucket_idx(key);
        let (_, bucket_box_opt) = self.read_bucket(idx);
        let mut bucket_box = bucket_box_opt?;
        let mut bucket = bucket_box.get_cloned();

        let mut prev = None;

        for i in 0..bucket.len() {
            let elem = bucket.get_cloned(i).unwrap();

            if elem.key.eq(key) {
                bucket.swap(i, bucket.len() - 1);
                let elem = bucket.pop().unwrap();

                prev = Some(elem.val);
                break;
            }
        }

        unsafe {
            let should_update = bucket_box.set(&bucket);

            if should_update {
                self.set_bucket(idx, &bucket_box);
            }
        }
        self._info._len -= 1;

        prev
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.get_cloned(key).is_some()
    }

    pub fn get_cloned(&self, key: &K) -> Option<V> {
        if self.is_empty() {
            return None;
        }

        let idx = self.find_bucket_idx(key);
        let (_, bucket_box) = self.read_bucket(idx);
        let bucket = bucket_box?.get_cloned();

        for i in 0..bucket.len() {
            let elem = bucket.get_cloned(i).unwrap();
            if elem.key.eq(key) {
                return Some(elem.val);
            }
        }

        None
    }

    pub fn len(&self) -> u64 {
        self._info._len
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn drop(self) {
        for i in 0..self._info._table_capacity {
            let (_, bucket_box_opt) = self.read_bucket(i as usize);
            if let Some(bb) = bucket_box_opt {
                bb.drop();
            }
        }

        deallocate(self.table());
    }

    fn init_table(&mut self) {
        if self._info._table.is_none() {
            let capacity_bytes = self._info._table_capacity as usize * PTR_SIZE;
            let table = allocate(capacity_bytes);

            self._info._table = Some(table);
        }
    }

    fn find_bucket_idx(&self, key: &K) -> usize {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let hash = hasher.finish();

        (hash % self._info._table_capacity as u64) as usize
    }

    fn set_bucket(&mut self, idx: usize, bucket_value: &HashMapBucket<K, V>) {
        let offset = idx * PTR_SIZE;
        self.table()
            ._write_word(offset, unsafe { bucket_value.as_ptr() });
    }

    fn read_bucket(&self, idx: usize) -> (usize, Option<HashMapBucket<K, V>>) {
        let offset = idx * PTR_SIZE;
        let ptr = self.table()._read_word(offset);

        if ptr == 0 || ptr == EMPTY_PTR {
            (offset, None)
        } else {
            (
                offset,
                Some(unsafe { HashMapBucket::<K, V>::from_ptr(ptr) }),
            )
        }
    }

    fn table(&self) -> SSlice<SMapTable> {
        unsafe { self._info._table.as_ref().unwrap().clone() }
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::hash_map::SHashMap;
    use crate::init_allocator;
    use crate::utils::mem_context::stable;

    fn test_body(mut map: SHashMap<String, i32>) {
        let k1 = "key1".to_string();
        let k2 = "key2".to_string();
        let k3 = "key3".to_string();
        let k4 = "key4".to_string();
        let k5 = "key5".to_string();
        let k6 = "key6".to_string();
        let k7 = "key7".to_string();
        let k8 = "key8".to_string();

        map.insert(k1.clone(), 1);
        map.insert(k2.clone(), 2);
        map.insert(k3.clone(), 3);
        map.insert(k4.clone(), 4);
        map.insert(k5.clone(), 5);
        map.insert(k6.clone(), 6);
        map.insert(k7.clone(), 7);
        map.insert(k8.clone(), 8);

        assert_eq!(map.get_cloned(&k1).unwrap(), 1);
        assert_eq!(map.get_cloned(&k2).unwrap(), 2);
        assert_eq!(map.get_cloned(&k3).unwrap(), 3);
        assert_eq!(map.get_cloned(&k4).unwrap(), 4);
        assert_eq!(map.get_cloned(&k5).unwrap(), 5);
        assert_eq!(map.get_cloned(&k6).unwrap(), 6);
        assert_eq!(map.get_cloned(&k7).unwrap(), 7);
        assert_eq!(map.get_cloned(&k8).unwrap(), 8);

        assert!(map.get_cloned(&String::from("key9")).is_none());
        assert!(map.get_cloned(&String::from("key0")).is_none());

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

        let map = SHashMap::new();
        test_body(map);
    }

    #[test]
    fn simple_flow_works_well_for_small() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let map = SHashMap::new_with_capacity(3);
        test_body(map);
    }
}
