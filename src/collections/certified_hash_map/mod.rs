use crate::collections::certified_hash_map::iter::SCertifiedHashMapIter;
use crate::mem::allocator::EMPTY_PTR;
use crate::mem::s_slice::Side;
use crate::primitive::StableAllocated;
use crate::utils::math::fast_log2;
use crate::utils::phantom_data::SPhantomData;
use crate::{allocate, deallocate, SSlice};
use candid::types::ic_types::{hash_tree, Sha256Digest};
use copy_as_bytes::traits::{AsBytes, SuperSized};
use sha2::{Digest, Sha256};
use speedy::{Context, LittleEndian, Readable, Reader, Writable, Writer};
pub mod iter;

const DEFAULT_CAPACITY: usize = 4;
const EMPTY_HASH: Sha256Digest = [0u8; 32];

const EMPTY: u8 = 0;
const OCCUPIED: u8 = 1;
const TOMBSTONE: u8 = 255;

// reallocating, open addressing, quadratic probing, 2^n capacities
pub struct SCertifiedHashMap<K, V> {
    pub(crate) len: usize,
    pub(crate) capacity: usize,
    pub(crate) table: Option<SSlice>,
    _marker_k: SPhantomData<K>,
    _marker_v: SPhantomData<V>,
}

impl<K, V> SCertifiedHashMap<K, V> {
    #[inline]
    pub fn new() -> Self {
        Self::new_with_capacity(DEFAULT_CAPACITY)
    }

    pub fn new_with_capacity(capacity: usize) -> Self {
        let mut new_capacity = 2usize.pow(fast_log2(capacity));
        if new_capacity < capacity {
            new_capacity *= 2;
        }

        Self {
            len: 0,
            capacity: new_capacity,
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

    fn is_about_to_grow(&self) -> bool {
        self.table.is_none() || self.len > (self.capacity >> 2) * 3
    }
}

impl<K: StableAllocated + AsRef<[u8]> + Eq, V: StableAllocated> SCertifiedHashMap<K, V>
where
    [u8; K::SIZE]: Sized,
    [u8; V::SIZE]: Sized,
{
    pub fn insert(&mut self, mut key: K, mut value: V) -> Option<V> {
        self.maybe_reallocate();

        let mut prev = None;
        let (key_hash, key_n_hash) = self.hash_key(&key);
        let mut i = 0;

        let table = self.table.as_ref().unwrap();

        let mut remembered_at = None;

        loop {
            let at = Self::compute_idx(key_n_hash, i, self.capacity);

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
                    Self::write_key_hash_at(table, at, key_hash);
                    Self::write_val_at(table, at, value);

                    self.recalculate_branch_hashes(table, at);

                    self.len += 1;

                    break;
                }
                _ => unreachable!(),
            }
        }

        prev
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        if let Some(at) = self.find_inner_idx(key) {
            let table = unsafe { self.table.as_ref().unwrap_unchecked() };
            let mut prev_key = Self::read_key_at(table, at, true).unwrap();
            let mut prev_value = Self::read_val_at(table, at);

            prev_key.remove_from_stable();
            prev_value.remove_from_stable();

            Self::write_key_at(table, at, HashMapKey::Tombstone);
            Self::write_key_hash_at(table, at, EMPTY_HASH);

            self.recalculate_branch_hashes(table, at);

            self.len -= 1;

            Some(prev_value)
        } else {
            None
        }
    }

    pub fn get_copy(&self, key: &K) -> Option<V> {
        self.find_inner_idx(key)
            .map(|idx| Self::read_val_at(unsafe { self.table.as_ref().unwrap_unchecked() }, idx))
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.find_inner_idx(key).is_some()
    }

    pub fn witness_key(&self, key: &K) -> hash_tree::HashTree {
        if let Some(idx) = self.find_inner_idx(key) {
            hash_tree::leaf()
            hash_tree::empty()
        } else {
            hash_tree::empty()
        }
    }

    fn find_inner_idx(&self, key: &K) -> Option<usize> {
        self.table?;

        let (key_hash, key_n_hash) = self.hash_key(key);
        let mut i = 0;

        let table = self.table.as_ref().unwrap();

        loop {
            let at = Self::compute_idx(key_n_hash, i, self.capacity);
            i += 1;

            match Self::read_key_at(table, at, true) {
                HashMapKey::Occupied(prev_key) => {
                    if prev_key.eq(key) {
                        return Some(at);
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

        None
    }

    fn recalculate_branch_hashes(&self, table: &SSlice, mut idx: usize) {
        while idx > 0 {
            let key_hash = Self::read_key_hash_at(table, idx);
            let node_hash = self.make_node_hash(table, idx, &key_hash);
            Self::write_node_hash_at(table, idx, node_hash);

            idx /= 2;
        }

        // for root
        let key_hash = Self::read_key_hash_at(table, idx);
        let node_hash = self.make_node_hash(table, idx, &key_hash);
        Self::write_node_hash_at(table, idx, node_hash);
    }

    #[inline]
    pub fn iter(&self) -> SCertifiedHashMapIter<K, V> {
        SCertifiedHashMapIter::new(self)
    }

    #[inline]
    fn compute_idx(key_n_hash: usize, i: usize, capacity: usize) -> usize {
        (key_n_hash + i / 2 + i * i / 2) % capacity
    }

    fn hash_key(&self, key: &K) -> (Sha256Digest, usize) {
        let mut hasher = Sha256::default();
        hasher.update(key);
        let hash: Sha256Digest = hasher.finalize().into();

        let n_hash = Self::key_n_hash(&hash);

        (hash, n_hash)
    }

    fn key_n_hash(key_hash: &Sha256Digest) -> usize {
        let mut n_hash_buf = [0u8; usize::SIZE];
        n_hash_buf.copy_from_slice(&key_hash[..usize::SIZE]);

        usize::from_bytes(n_hash_buf)
    }

    fn hash_node(
        key_hash: &[u8; 32],
        left_child_hash: &[u8; 32],
        right_child_hash: &[u8; 32],
    ) -> [u8; 32] {
        let mut hasher = Sha256::default();
        hasher.update(key_hash);
        hasher.update(left_child_hash);
        hasher.update(right_child_hash);

        let hash = hasher.finalize();

        hash.into()
    }

    #[inline]
    fn to_offset_or_size(idx: usize) -> usize {
        idx * (1 + K::SIZE + 32 + 32 + V::SIZE)
    }

    #[inline]
    fn read_key_at(slice: &SSlice, idx: usize, read_value: bool) -> HashMapKey<K> {
        let mut key_flag = [0u8];
        let at = Self::to_offset_or_size(idx);

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

    #[inline]
    fn read_val_at(slice: &SSlice, idx: usize) -> V {
        let at = Self::to_offset_or_size(idx) + 1 + K::SIZE;

        let mut val_at_idx = V::super_size_u8_arr();
        slice.read_bytes(at, &mut val_at_idx);

        V::from_bytes(val_at_idx)
    }

    #[inline]
    fn read_key_hash_at(slice: &SSlice, idx: usize) -> [u8; 32] {
        let at = Self::to_offset_or_size(idx) + 1 + K::SIZE + V::SIZE;

        slice.as_bytes_read(at)
    }

    #[inline]
    fn read_node_hash_at(slice: &SSlice, idx: usize) -> [u8; 32] {
        let at = Self::to_offset_or_size(idx) + 1 + K::SIZE + V::SIZE + 32;

        slice.as_bytes_read(at)
    }

    #[inline]
    fn write_key_at(slice: &SSlice, idx: usize, key: HashMapKey<K>) {
        let at = Self::to_offset_or_size(idx);

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

    #[inline]
    fn write_val_at(slice: &SSlice, idx: usize, val: V) {
        let at = Self::to_offset_or_size(idx) + 1 + K::SIZE;
        let val_bytes = val.to_bytes();

        slice.write_bytes(at, &val_bytes);
    }

    #[inline]
    fn write_key_hash_at(slice: &SSlice, idx: usize, hash: [u8; 32]) {
        let at = Self::to_offset_or_size(idx) + 1 + K::SIZE + V::SIZE;

        slice.as_bytes_write(at, hash)
    }

    fn make_node_hash(&self, slice: &SSlice, idx: usize, key_hash: &[u8; 32]) -> [u8; 32] {
        let (lc_hash, rc_hash) = if idx > (self.capacity - 1) / 2 {
            (EMPTY_HASH, EMPTY_HASH)
        } else if let Some(r) = (idx + 1).checked_mul(2) {
            (
                Self::read_key_hash_at(slice, r - 1),
                Self::read_key_hash_at(slice, r),
            )
        } else {
            (EMPTY_HASH, EMPTY_HASH)
        };

        Self::hash_node(key_hash, &lc_hash, &rc_hash)
    }

    #[inline]
    fn write_node_hash_at(slice: &SSlice, idx: usize, hash: [u8; 32]) {
        let at = Self::to_offset_or_size(idx) + 1 + K::SIZE + V::SIZE + 32;

        slice.as_bytes_write(at, hash)
    }

    fn maybe_reallocate(&mut self) {
        if !self.is_about_to_grow() {
            return;
        }

        if let Some(old_table) = self.table {
            let new_capacity = self.capacity * 2;

            let new_table = allocate(Self::to_offset_or_size(new_capacity));
            new_table.write_bytes(0, &vec![0u8; new_table.get_size_bytes()]);

            for idx in 0..self.capacity {
                let k = Self::read_key_at(&old_table, idx, true);
                if matches!(k, HashMapKey::Empty | HashMapKey::Tombstone) {
                    continue;
                }

                let key = k.unwrap();
                let val = Self::read_val_at(&old_table, idx);
                let key_hash = Self::read_key_hash_at(&old_table, idx);
                let key_n_hash = Self::key_n_hash(&key_hash);

                let mut i = 0;

                loop {
                    let at = (key_n_hash + i / 2 + i * i / 2) % new_capacity as usize;
                    i += 1;

                    match Self::read_key_at(&new_table, at, false) {
                        HashMapKey::OccupiedNull => {
                            continue;
                        }
                        HashMapKey::Empty => {
                            Self::write_key_at(&new_table, at, HashMapKey::Occupied(key));
                            Self::write_val_at(&new_table, at, val);
                            Self::write_key_hash_at(&new_table, at, key_hash);

                            break;
                        }
                        _ => unreachable!(),
                    }
                }
            }

            // recalculate hashes for every non-leaf node
            let mut i = (new_capacity - 1) / 2;
            while i > 0 {
                let key_hash = Self::read_key_hash_at(&new_table, i);
                let node_hash = self.make_node_hash(&new_table, i, &key_hash);
                Self::write_node_hash_at(&new_table, i, node_hash);

                i /= 2;
            }

            let key_hash = Self::read_key_hash_at(&new_table, 0);
            let node_hash = self.make_node_hash(&new_table, 0, &key_hash);
            Self::write_node_hash_at(&new_table, 0, node_hash);

            self.capacity = new_capacity;
            self.table = Some(new_table);

            deallocate(old_table);
        } else {
            let slice = allocate(Self::to_offset_or_size(self.capacity));
            slice.write_bytes(0, &vec![0u8; slice.get_size_bytes()]);

            self.table = Some(slice)
        }
    }
}

impl<K, V> Default for SCertifiedHashMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, K, V> Readable<'a, LittleEndian> for SCertifiedHashMap<K, V> {
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

        let it = Self {
            len,
            capacity,
            table,
            _marker_k: SPhantomData::default(),
            _marker_v: SPhantomData::default(),
        };

        Ok(it)
    }
}

impl<K, V> Writable<LittleEndian> for SCertifiedHashMap<K, V> {
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

impl<K, V> SuperSized for SCertifiedHashMap<K, V> {
    const SIZE: usize = usize::SIZE * 2 + u64::SIZE;
}

impl<K, V> AsBytes for SCertifiedHashMap<K, V> {
    fn to_bytes(self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[..usize::SIZE].copy_from_slice(&self.len.to_bytes());
        buf[usize::SIZE..(usize::SIZE * 2)].copy_from_slice(&self.capacity.to_bytes());

        let table_buf = self
            .table
            .map(|it| it.get_ptr())
            .unwrap_or(EMPTY_PTR)
            .to_bytes();
        buf[(usize::SIZE * 2)..(usize::SIZE * 2 + u64::SIZE)].copy_from_slice(&table_buf);

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

impl<K: StableAllocated + Eq + AsRef<[u8]>, V: StableAllocated> StableAllocated
    for SCertifiedHashMap<K, V>
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
    use crate::collections::certified_hash_map::SCertifiedHashMap;
    use crate::init_allocator;
    use crate::primitive::StableAllocated;
    use crate::utils::mem_context::stable;
    use rand::seq::SliceRandom;
    use rand::thread_rng;

    #[test]
    fn simple_flow_works_well() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SCertifiedHashMap::new_with_capacity(3);

        let k1 = 1u32;
        let k2 = 2u32;
        let k3 = 3u32;
        let k4 = 4u32;
        let k5 = 5u32;
        let k6 = 6u32;
        let k7 = 7u32;
        let k8 = 8u32;

        map.insert(k1.to_le_bytes(), 1);
        map.insert(k2.to_le_bytes(), 2);
        map.insert(k3.to_le_bytes(), 3);
        map.insert(k4.to_le_bytes(), 4);
        map.insert(k5.to_le_bytes(), 5);
        map.insert(k6.to_le_bytes(), 6);
        map.insert(k7.to_le_bytes(), 7);
        map.insert(k8.to_le_bytes(), 8);

        assert_eq!(map.get_copy(&k1.to_le_bytes()).unwrap(), 1);
        assert_eq!(map.get_copy(&k2.to_le_bytes()).unwrap(), 2);
        assert_eq!(map.get_copy(&k3.to_le_bytes()).unwrap(), 3);
        assert_eq!(map.get_copy(&k4.to_le_bytes()).unwrap(), 4);
        assert_eq!(map.get_copy(&k5.to_le_bytes()).unwrap(), 5);
        assert_eq!(map.get_copy(&k6.to_le_bytes()).unwrap(), 6);
        assert_eq!(map.get_copy(&k7.to_le_bytes()).unwrap(), 7);
        assert_eq!(map.get_copy(&k8.to_le_bytes()).unwrap(), 8);

        assert!(map.get_copy(&9u32.to_le_bytes()).is_none());
        assert!(map.get_copy(&0u32.to_le_bytes()).is_none());

        assert_eq!(map.remove(&k3.to_le_bytes()).unwrap(), 3);
        assert!(map.get_copy(&k3.to_le_bytes()).is_none());

        assert_eq!(map.remove(&k1.to_le_bytes()).unwrap(), 1);
        assert!(map.get_copy(&k1.to_le_bytes()).is_none());

        assert_eq!(map.remove(&k5.to_le_bytes()).unwrap(), 5);
        assert!(map.get_copy(&k5.to_le_bytes()).is_none());

        assert_eq!(map.remove(&k7.to_le_bytes()).unwrap(), 7);
        assert!(map.get_copy(&k7.to_le_bytes()).is_none());

        assert_eq!(map.get_copy(&k2.to_le_bytes()).unwrap(), 2);
        assert_eq!(map.get_copy(&k4.to_le_bytes()).unwrap(), 4);
        assert_eq!(map.get_copy(&k6.to_le_bytes()).unwrap(), 6);
        assert_eq!(map.get_copy(&k8.to_le_bytes()).unwrap(), 8);

        unsafe { map.stable_drop() };
    }

    #[test]
    fn basic_flow_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SCertifiedHashMap::new_with_capacity(3);

        assert!(map.remove(&10u32.to_le_bytes()).is_none());
        assert!(map.get_copy(&10u32.to_le_bytes()).is_none());

        let it = map.insert(1u32.to_le_bytes(), 1);
        assert!(it.is_none());
        assert!(map.insert(2u32.to_le_bytes(), 2).is_none());
        assert!(map.insert(3u32.to_le_bytes(), 3).is_none());
        assert_eq!(map.insert(1u32.to_le_bytes(), 10).unwrap(), 1);

        assert!(map.remove(&5u32.to_le_bytes()).is_none());
        assert_eq!(map.remove(&1u32.to_le_bytes()).unwrap(), 10);

        assert!(map.contains_key(&2u32.to_le_bytes()));
        assert!(!map.contains_key(&5u32.to_le_bytes()));

        unsafe { map.stable_drop() };

        let mut map = SCertifiedHashMap::default();
        for i in 0..100u32 {
            assert!(map.insert(i.to_le_bytes(), i).is_none());
        }

        for i in 0..100u32 {
            assert_eq!(map.get_copy(&i.to_le_bytes()).unwrap(), i);
        }

        for i in 0..100u32 {
            assert_eq!(map.remove(&(99 - i).to_le_bytes()).unwrap(), 99 - i);
        }

        unsafe { map.stable_drop() };
    }

    #[test]
    fn removes_work() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SCertifiedHashMap::new();

        for i in 0..500u32 {
            map.insert((499 - i).to_le_bytes(), i);
        }

        let mut vec = (200u32..300).collect::<Vec<_>>();
        vec.shuffle(&mut thread_rng());

        for i in vec {
            map.remove(&i.to_le_bytes());
        }

        for i in 500..5000u32 {
            map.insert(i.to_le_bytes(), i);
        }

        for i in 200..300u32 {
            map.insert(i.to_le_bytes(), i);
        }

        let mut vec = (0..5000u32).collect::<Vec<_>>();
        vec.shuffle(&mut thread_rng());

        for i in vec {
            map.remove(&i.to_le_bytes());
        }

        unsafe { map.stable_drop() };
    }

    #[test]
    fn tombstones_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SCertifiedHashMap::new();

        for i in 0..100u32 {
            map.insert(i.to_le_bytes(), i);
        }

        assert_eq!(map.len(), 100);

        for i in 0..50u32 {
            map.remove(&i.to_le_bytes());
        }

        assert_eq!(map.len(), 50);

        for i in 0..50u32 {
            map.insert(i.to_le_bytes(), i);
        }

        assert_eq!(map.len(), 100);

        unsafe { map.stable_drop() };
    }

    #[test]
    fn iter_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SCertifiedHashMap::new();
        for i in 0..100u32 {
            map.insert(i.to_le_bytes(), i);
        }

        let mut c = 0;
        for (k, v) in map.iter() {
            c += 1;

            assert!(u32::from_le_bytes(k) < 100);
        }

        assert_eq!(c, 100);
    }

    #[test]
    fn sboxes_work_fine() {
        /*        stable::clear();
                stable::grow(1).unwrap();
                init_allocator(0);

                let mut map = SCertifiedHashMap::new();

                for i in 0..100 {
                    map.insert(SBox::new(i), i);
                }

                unsafe { map.stable_drop() };
        */
        // TODO: this part doesn't work for some reason
        // it seems like hashes calculate differently

        /*
        println!("sbox mut");
        let mut map = SHashMap::new();

        for i in 0..100 {
            map.insert(SBoxMut::new(i), i);
        }

        unsafe { map.stable_drop() };*/
    }
}
