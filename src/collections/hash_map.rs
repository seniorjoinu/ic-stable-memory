use crate::collections::vec::SVec;
use crate::mem::allocator::EMPTY_PTR;
use crate::primitive::s_cellbox::SCellBox;
use crate::primitive::s_unsafe_cell::SUnsafeCell;
use crate::utils::encode::AsBytes;
use crate::{allocate, deallocate, OutOfMemory, SSlice};
use candid::types::{Field, Label, Serializer, Type};
use candid::{CandidType, Deserialize};
use serde::Deserializer;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::mem::size_of;

const STABLE_HASH_MAP_DEFAULT_CAPACITY: u32 = 9973;

struct HashMapEntry<K, V> {
    key: K,
    val: V,
}

impl<K, V> HashMapEntry<K, V> {
    pub fn new(key: K, val: V) -> Self {
        Self { key, val }
    }
}

impl<K: AsBytes, V: AsBytes> AsBytes for HashMapEntry<K, V> {
    unsafe fn as_bytes(&self) -> Vec<u8> {
        let mut result = Vec::new();
        result.extend(self.key.as_bytes());
        result.extend(self.val.as_bytes());

        result
    }

    unsafe fn from_bytes(bytes: &[u8]) -> Self {
        let key_size = size_of::<K>();
        let val_size = size_of::<V>();

        let key = K::from_bytes(&bytes[0..key_size]);
        let val = V::from_bytes(&bytes[key_size..key_size + val_size]);

        Self { key, val }
    }
}

struct SMapTable;
type SHashMapBucketPtr<K, V> = SUnsafeCell<SVec<HashMapEntry<K, V>>>;

#[derive(CandidType, Deserialize)]
struct SHashMapInfo {
    _len: u64,
    _table_capacity: u32,
    _table: SSlice<SMapTable>,
}

pub struct SHashMap<K, V> {
    _info: SHashMapInfo,
    _k: PhantomData<K>,
    _v: PhantomData<V>,
}

impl<K, V> CandidType for SHashMap<K, V> {
    fn _ty() -> Type {
        SHashMapInfo::ty()
    }

    fn idl_serialize<S>(&self, serializer: S) -> Result<(), S::Error>
    where
        S: Serializer,
    {
        self._info.idl_serialize(serializer)
    }
}

impl<'de, K, V> Deserialize<'de> for SHashMap<K, V> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let _info = SHashMapInfo::deserialize(deserializer)?;
        Ok(Self {
            _info,
            _k: PhantomData::default(),
            _v: PhantomData::default(),
        })
    }
}

impl<K: Hash + Eq + Sized + AsBytes, V: Sized + AsBytes> SHashMap<K, V> {
    pub fn new() -> Result<Self, OutOfMemory> {
        Self::with_capacity(STABLE_HASH_MAP_DEFAULT_CAPACITY)
    }

    pub fn with_capacity(capacity: u32) -> Result<Self, OutOfMemory> {
        let capacity_bytes = capacity as usize * size_of::<SHashMapBucketPtr<K, V>>();
        let slice = allocate(capacity_bytes)?;

        let mut table_bytes = vec![0u8; capacity_bytes];
        slice._read_bytes(0, &mut table_bytes);

        let _info = SHashMapInfo {
            _len: 0,
            _table_capacity: capacity,
            _table: slice,
        };

        Ok(Self {
            _info,
            _k: PhantomData::default(),
            _v: PhantomData::default(),
        })
    }

    fn find_bucket(&self, key: &K) -> (usize, Option<SHashMapBucketPtr<K, V>>) {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let hash = hasher.finish();
        let idx = (hash % self._info._table_capacity as u64) as usize;

        self.read_bucket(idx)
    }

    fn read_bucket(&self, idx: usize) -> (usize, Option<SHashMapBucketPtr<K, V>>) {
        let offset = idx * size_of::<SHashMapBucketPtr<K, V>>();
        let ptr = self._info._table._read_word(offset);

        if ptr == 0 || ptr == EMPTY_PTR {
            (offset, None)
        } else {
            (
                offset,
                Some(unsafe { SHashMapBucketPtr::<K, V>::from_ptr(ptr) }),
            )
        }
    }

    pub fn insert(&mut self, key: K, value: V) -> Result<Option<V>, OutOfMemory> {
        let entry = HashMapEntry::new(key, value);
        let (offset, bucket_box_opt) = self.find_bucket(&entry.key);

        let (mut bucket_box, mut bucket) = if let Some(bb) = bucket_box_opt {
            let bucket = bb.get_cloned();

            (bb, bucket)
        } else {
            let bucket = SVec::new();
            let bb = SHashMapBucketPtr::new(&bucket)?;

            (bb, bucket)
        };

        let mut found = false;
        let mut prev = None;

        for i in 0..bucket.len() {
            let elem = bucket.get(i).unwrap();
            if elem.key == entry.key {
                bucket.replace(i, &entry);
                prev = Some(elem.val);
                found = true;
                break;
            }
        }

        if !found {
            bucket.push(&entry)?;
        }

        self._info._len += 1;

        let should_update = unsafe { bucket_box.set(&bucket)? };
        if should_update {
            self._info
                ._table
                ._write_word(offset, unsafe { bucket_box.to_ptr() });
        }

        Ok(prev)
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        if self.is_empty() {
            return None;
        }

        let (_, bucket_box_opt) = self.find_bucket(key);
        let mut bucket_box = bucket_box_opt?;
        let mut bucket = bucket_box.get_cloned();

        let mut prev = None;

        for i in 0..bucket.len() {
            let elem = bucket.get(i).unwrap();

            if elem.key.eq(key) {
                bucket.swap(i, bucket.len() - 1);
                let elem = bucket.pop().unwrap();

                prev = Some(elem.val);
                break;
            }
        }

        unsafe { bucket_box.set(&bucket).expect("Should not reallocate") };
        self._info._len -= 1;

        prev
    }

    pub fn get(&self, key: &K) -> Option<V> {
        if self.is_empty() {
            return None;
        }

        let (_, bucket_box) = self.find_bucket(key);
        let bucket = bucket_box?.get_cloned();

        for i in 0..bucket.len() {
            let elem = bucket.get(i).unwrap();
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

        deallocate(self._info._table);
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::hash_map::SHashMap;
    use crate::init_allocator;
    use crate::utils::mem_context::stable;

    fn test_body(mut map: SHashMap<&str, i32>) {
        map.insert("key1", 1).unwrap();
        map.insert("key2", 2).unwrap();
        map.insert("key3", 3).unwrap();
        map.insert("key4", 4).unwrap();
        map.insert("key5", 5).unwrap();
        map.insert("key6", 6).unwrap();
        map.insert("key7", 7).unwrap();
        map.insert("key8", 8).unwrap();

        assert_eq!(map.get(&"key1").unwrap(), 1);
        assert_eq!(map.get(&"key2").unwrap(), 2);
        assert_eq!(map.get(&"key3").unwrap(), 3);
        assert_eq!(map.get(&"key4").unwrap(), 4);
        assert_eq!(map.get(&"key5").unwrap(), 5);
        assert_eq!(map.get(&"key6").unwrap(), 6);
        assert_eq!(map.get(&"key7").unwrap(), 7);
        assert_eq!(map.get(&"key8").unwrap(), 8);

        assert!(map.get(&"key9").is_none());
        assert!(map.get(&"key0").is_none());

        assert_eq!(map.remove(&"key3").unwrap(), 3);
        assert!(map.get(&"key3").is_none());

        assert_eq!(map.remove(&"key1").unwrap(), 1);
        assert!(map.get(&"key1").is_none());

        assert_eq!(map.remove(&"key5").unwrap(), 5);
        assert!(map.get(&"key5").is_none());

        assert_eq!(map.remove(&"key7").unwrap(), 7);
        assert!(map.get(&"key7").is_none());

        assert_eq!(map.get(&"key2").unwrap(), 2);
        assert_eq!(map.get(&"key4").unwrap(), 4);
        assert_eq!(map.get(&"key6").unwrap(), 6);
        assert_eq!(map.get(&"key8").unwrap(), 8);

        map.drop();
    }

    #[test]
    fn simple_flow_works_well_for_big() {
        stable::grow(1).unwrap();
        init_allocator(0);

        let map = SHashMap::new().unwrap();
        test_body(map);
    }

    #[test]
    fn simple_flow_works_well_for_small() {
        stable::grow(1).unwrap();
        init_allocator(0);

        let map = SHashMap::with_capacity(3).unwrap();
        test_body(map);
    }
}
