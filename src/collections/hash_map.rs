use crate::collections::vec::SVec;
use crate::primitive::s_cellbox::SCellBox;
use crate::utils::encode::AsBytes;
use crate::OutOfMemory;
use candid::types::{Field, Label, Serializer, Type};
use candid::{CandidType, Deserialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::mem::size_of;

const STABLE_HASH_MAP_DEFAULT_CAPACITY: u64 = 11;

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

#[derive(CandidType, Deserialize)]
pub struct SHashMap<K, V> {
    _len: u64,
    _table: SVec<SCellBox<SVec<HashMapEntry<K, V>>>>,
}

impl<K: Hash + Eq + Sized + AsBytes, V: Sized + AsBytes> Default for SHashMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Hash + Eq + Sized + AsBytes, V: Sized + AsBytes> SHashMap<K, V> {
    pub fn new() -> Self {
        Self::with_capacity(STABLE_HASH_MAP_DEFAULT_CAPACITY)
    }

    pub fn with_capacity(capacity: u64) -> Self {
        Self {
            _len: 0,
            _table: SVec::new_with_capacity(capacity),
        }
    }

    fn init_buckets(&mut self) -> Result<(), OutOfMemory> {
        if self._table.is_empty() {
            for _ in 0..self._table.capacity() {
                let bucket = SVec::new_with_capacity(1);
                let bucket_box = SCellBox::new(&bucket)?;
                self._table.push(&bucket_box)?;
            }
        }

        Ok(())
    }

    fn find_bucket(&self, key: &K) -> SCellBox<SVec<HashMapEntry<K, V>>> {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let hash = hasher.finish();
        let idx = hash % self._table.len();

        self._table.get(idx).unwrap()
    }

    pub fn insert(&mut self, key: K, value: V) -> Result<Option<V>, OutOfMemory> {
        self.init_buckets()?;

        let entry = HashMapEntry::new(key, value);
        let mut bucket_box = self.find_bucket(&entry.key);
        let mut bucket = bucket_box.get_cloned();

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

        bucket_box.set(&bucket)?;
        self._len += 1;

        Ok(prev)
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        if self.is_empty() {
            return None;
        }

        let mut bucket_box = self.find_bucket(key);
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

        bucket_box.set(&bucket).expect("Should not reallocate");
        self._len -= 1;

        prev
    }

    pub fn get(&self, key: &K) -> Option<V> {
        if self.is_empty() {
            return None;
        }

        let bucket_box = self.find_bucket(key);
        let bucket = bucket_box.get_cloned();

        for i in 0..bucket.len() {
            let elem = bucket.get(i).unwrap();
            if elem.key.eq(key) {
                return Some(elem.val);
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
        for i in 0..self._table.len() {
            self._table.get(i).unwrap().drop()
        }

        self._table.drop();
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::hash_map::SHashMap;
    use crate::init_allocator;
    use crate::utils::mem_context::stable;

    #[test]
    fn simple_flow_works_well() {
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SHashMap::new();

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
    }
}
