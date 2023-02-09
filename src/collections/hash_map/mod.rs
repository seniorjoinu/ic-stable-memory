use crate::collections::hash_map::iter::SHashMapIter;
use crate::encoding::{AsFixedSizeBytes, Buffer};
use crate::mem::allocator::EMPTY_PTR;
use crate::mem::StablePtr;
use crate::primitive::s_ref::SRef;
use crate::primitive::s_ref_mut::SRefMut;
use crate::primitive::StableType;
use crate::{allocate, deallocate, OutOfMemory, SSlice};
use std::borrow::Borrow;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use zwohash::ZwoHasher;

pub mod iter;

// Layout:
// KEYS: [K; CAPACITY] = [zeroed(K); CAPACITY]
// VALUES: [V; CAPACITY] = [zeroed(V); CAPACITY]

const KEYS_OFFSET: usize = 0;

#[inline]
const fn values_offset<K: AsFixedSizeBytes>(capacity: usize) -> usize {
    KEYS_OFFSET + (1 + K::SIZE) * capacity
}

const DEFAULT_CAPACITY: usize = 7;

const EMPTY: u8 = 0;
const OCCUPIED: u8 = 255;

type KeyHash = usize;

// all for maximum cache-efficiency
// fixed-size, open addressing, linear probing, 3/4 load factor, no tombstones / non-lazy removal (https://stackoverflow.com/a/60709252/7171515)
pub struct SHashMap<K: StableType + AsFixedSizeBytes + Hash + Eq, V: StableType + AsFixedSizeBytes>
{
    table_ptr: u64,
    len: usize,
    cap: usize,
    is_owned: bool,
    _marker_k: PhantomData<K>,
    _marker_v: PhantomData<V>,
}

impl<K: StableType + AsFixedSizeBytes + Hash + Eq, V: StableType + AsFixedSizeBytes>
    SHashMap<K, V>
{
    #[inline]
    pub const fn max_capacity() -> usize {
        u32::MAX as usize / (K::SIZE + V::SIZE)
    }

    fn hash<T: Hash>(val: &T) -> KeyHash {
        let mut hasher = ZwoHasher::default();
        val.hash(&mut hasher);

        hasher.finish() as KeyHash
    }

    #[inline]
    pub fn new() -> Self {
        Self::new_with_capacity(DEFAULT_CAPACITY)
    }

    pub fn new_with_capacity(capacity: usize) -> Self {
        assert!(capacity <= Self::max_capacity());

        Self {
            table_ptr: EMPTY_PTR,
            len: 0,
            cap: capacity,
            is_owned: false,
            _marker_k: PhantomData::default(),
            _marker_v: PhantomData::default(),
        }
    }

    pub fn insert(&mut self, key: K, value: V) -> Result<Option<V>, OutOfMemory> {
        if self.table_ptr == EMPTY_PTR {
            let size = (1 + K::SIZE + V::SIZE) * self.capacity();
            let table = allocate(size as u64)?;

            let zeroed = vec![0u8; size];
            unsafe { crate::mem::write_bytes(table.offset(0), &zeroed) };

            self.table_ptr = table.as_ptr();
        }

        let key_hash = Self::hash(&key);
        let mut i = key_hash % self.capacity();

        loop {
            match self.get_key(i) {
                Some(prev_key) => {
                    if prev_key.eq(&key) {
                        let prev_value = self.read_and_disown_val(i);
                        self.write_and_own_val(i, value);

                        return Ok(Some(prev_value));
                    } else {
                        i = (i + 1) % self.capacity();

                        continue;
                    }
                }
                None => {
                    if self.is_full() {
                        let mut new =
                            Self::new_with_capacity(self.capacity().checked_mul(2).unwrap() - 1);

                        for i in 0..self.cap {
                            if let Some(k) = self.read_and_disown_key(i) {
                                let v = self.read_and_disown_val(i);

                                new.insert(k, v).unwrap();
                            }
                        }

                        let res = new.insert(key, value).unwrap();

                        let slice = SSlice::from_ptr(self.table_ptr).unwrap();
                        deallocate(slice);

                        // dirty hack to make it not call stable_drop() when it is dropped
                        // it is safe to use, since we've moved all the data inside into the new map
                        // and deallocated the underlying slice
                        unsafe { self.assume_owned_by_stable_memory() };

                        *self = new;

                        return Ok(res);
                    }

                    self.write_and_own_key(i, Some(key));
                    self.write_and_own_val(i, value);

                    self.len += 1;

                    return Ok(None);
                }
            }
        }
    }

    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        Some(self.remove_by_idx(self.find_inner_idx(key)?))
    }

    #[inline]
    pub fn get<Q>(&self, key: &Q) -> Option<SRef<V>>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        Some(self.get_val(self.find_inner_idx(key)?))
    }

    #[inline]
    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<SRefMut<V>>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        Some(self.get_val_mut(self.find_inner_idx(key)?))
    }

    #[inline]
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.find_inner_idx(key).is_some()
    }

    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub const fn capacity(&self) -> usize {
        self.cap
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub const fn is_full(&self) -> bool {
        self.len() == (self.capacity() >> 2) * 3
    }

    #[inline]
    pub fn iter(&self) -> SHashMapIter<K, V> {
        SHashMapIter::new(self)
    }

    pub fn clear(&mut self) {
        if self.is_empty() {
            return;
        }

        for i in 0..self.cap {
            if let Some(k) = self.read_and_disown_key(i) {
                let v = self.read_and_disown_val(i);

                self.write_and_own_key(i, None);
            }
        }

        self.len = 0;
    }

    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&K, &V) -> bool,
    {
        if self.is_empty() {
            return;
        }

        for i in 0..self.cap {
            if let Some(mut k) = self.read_and_disown_key(i) {
                let mut v = self.read_and_disown_val(i);
                if f(&k, &v) {
                    unsafe {
                        k.assume_owned_by_stable_memory();
                        v.assume_owned_by_stable_memory();
                    }

                    continue;
                }

                self.write_and_own_key(i, None);
                self.len -= 1;
            }

            continue;
        }
    }

    fn remove_by_idx(&mut self, mut i: usize) -> V {
        let prev_value = self.read_and_disown_val(i);
        self.read_and_disown_key(i).unwrap();

        let mut j = i;

        loop {
            j = (j + 1) % self.capacity();
            if j == i {
                break;
            }

            if let Some(next_key) = self.read_and_disown_key(j) {
                let k = Self::hash(&next_key) % self.capacity();
                if (j < i) ^ (k <= i) ^ (k > j) {
                    self.write_and_own_key(i, Some(next_key));
                    self.write_and_own_val(i, self.read_and_disown_val(j));

                    i = j;
                }

                continue;
            }

            break;
        }

        self.write_and_own_key(i, None);
        self.len -= 1;

        prev_value
    }

    fn find_inner_idx<Q>(&self, key: &Q) -> Option<usize>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        if self.is_empty() {
            return None;
        }

        let key_hash = Self::hash(key);
        let mut i = key_hash % self.capacity();

        loop {
            if (*self.get_key(i)?).borrow().eq(key) {
                return Some(i);
            } else {
                i = (i + 1) % self.capacity();
            }
        }
    }

    fn get_key(&self, idx: usize) -> Option<SRef<K>> {
        let ptr = self.get_key_flag_ptr(idx);
        let flag: u8 = unsafe { crate::mem::read_fixed_for_reference(ptr) };

        match flag {
            EMPTY => None,
            OCCUPIED => Some(SRef::new(ptr + 1)),
            _ => unreachable!(),
        }
    }

    fn read_and_disown_key(&self, idx: usize) -> Option<K> {
        let ptr = self.get_key_flag_ptr(idx);
        let flag: u8 = unsafe { crate::mem::read_fixed_for_reference(ptr) };

        match flag {
            EMPTY => None,
            OCCUPIED => Some(unsafe { crate::mem::read_and_disown_fixed(ptr + 1) }),
            _ => unreachable!(),
        }
    }

    fn write_and_own_key(&mut self, idx: usize, key: Option<K>) {
        let ptr = self.get_key_flag_ptr(idx);

        if let Some(mut k) = key {
            unsafe { crate::mem::write_and_own_fixed(ptr, &mut OCCUPIED) };
            unsafe { crate::mem::write_and_own_fixed(ptr + 1, &mut k) };

            return;
        }

        unsafe { crate::mem::write_and_own_fixed(ptr, &mut EMPTY) };
    }

    #[inline]
    fn get_val(&self, idx: usize) -> SRef<V> {
        SRef::new(self.get_value_ptr(idx))
    }

    #[inline]
    fn get_val_mut(&self, idx: usize) -> SRefMut<V> {
        SRefMut::new(self.get_value_ptr(idx))
    }

    #[inline]
    fn read_and_disown_val(&self, idx: usize) -> V {
        unsafe { crate::mem::read_and_disown_fixed(self.get_value_ptr(idx)) }
    }

    #[inline]
    fn write_and_own_val(&mut self, idx: usize, mut val: V) {
        unsafe { crate::mem::write_and_own_fixed(self.get_value_ptr(idx), &mut val) }
    }

    #[inline]
    fn get_value_ptr(&self, idx: usize) -> StablePtr {
        SSlice::_offset(
            self.table_ptr,
            (values_offset::<K>(self.capacity()) + V::SIZE * idx) as u64,
        )
    }

    #[inline]
    fn get_key_flag_ptr(&self, idx: usize) -> StablePtr {
        SSlice::_offset(self.table_ptr, (KEYS_OFFSET + (1 + K::SIZE) * idx) as u64)
    }

    #[inline]
    fn get_key_data_ptr(&self, idx: usize) -> StablePtr {
        SSlice::_offset(
            self.table_ptr,
            (KEYS_OFFSET + (1 + K::SIZE) * idx + 1) as u64,
        )
    }

    pub fn debug_print(&self) {
        print!("Node({}, {})[", self.len(), self.capacity());
        for i in 0..self.capacity() {
            let k_flag: u8 =
                unsafe { crate::mem::read_fixed_for_reference(self.get_key_flag_ptr(i)) };
            let mut k_buf = K::Buf::new(K::SIZE);
            let mut v_buf = V::Buf::new(V::SIZE);

            unsafe { crate::mem::read_bytes(self.get_key_data_ptr(i), k_buf._deref_mut()) };
            unsafe { crate::mem::read_bytes(self.get_value_ptr(i), v_buf._deref_mut()) };

            print!("(");

            match k_flag {
                EMPTY => print!("<empty> = "),
                OCCUPIED => print!("<occupied> = "),
                _ => unreachable!(),
            };

            print!("{:?}, {:?})", k_buf._deref(), v_buf._deref());

            if i < self.capacity() - 1 {
                print!(", ");
            }
        }
        println!("]");
    }
}

impl<K: StableType + AsFixedSizeBytes + Hash + Eq, V: StableType + AsFixedSizeBytes> Default
    for SHashMap<K, V>
{
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<K: StableType + AsFixedSizeBytes + Hash + Eq, V: StableType + AsFixedSizeBytes>
    AsFixedSizeBytes for SHashMap<K, V>
{
    const SIZE: usize = u64::SIZE + usize::SIZE * 2;
    type Buf = [u8; u64::SIZE + usize::SIZE * 2];

    fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
        self.table_ptr.as_fixed_size_bytes(&mut buf[0..u64::SIZE]);
        self.len
            .as_fixed_size_bytes(&mut buf[u64::SIZE..(usize::SIZE + u64::SIZE)]);
        self.cap.as_fixed_size_bytes(
            &mut buf[(usize::SIZE + u64::SIZE)..(usize::SIZE * 2 + u64::SIZE)],
        );
    }

    fn from_fixed_size_bytes(buf: &[u8]) -> Self {
        let table_ptr = u64::from_fixed_size_bytes(&buf[0..u64::SIZE]);
        let len = usize::from_fixed_size_bytes(&buf[u64::SIZE..(usize::SIZE + u64::SIZE)]);
        let cap = usize::from_fixed_size_bytes(
            &buf[(usize::SIZE + u64::SIZE)..(usize::SIZE * 2 + u64::SIZE)],
        );

        Self {
            table_ptr,
            len,
            cap,
            is_owned: false,
            _marker_k: PhantomData::default(),
            _marker_v: PhantomData::default(),
        }
    }
}

impl<K: StableType + AsFixedSizeBytes + Hash + Eq, V: StableType + AsFixedSizeBytes> StableType
    for SHashMap<K, V>
{
    #[inline]
    unsafe fn assume_owned_by_stable_memory(&mut self) {
        self.is_owned = true;
    }

    #[inline]
    unsafe fn assume_not_owned_by_stable_memory(&mut self) {
        self.is_owned = false;
    }

    #[inline]
    fn is_owned_by_stable_memory(&self) -> bool {
        self.is_owned
    }

    unsafe fn stable_drop(&mut self) {
        if self.table_ptr != EMPTY_PTR {
            self.clear();

            let slice = SSlice::from_ptr(self.table_ptr).unwrap();
            deallocate(slice);
        }
    }
}

impl<K: StableType + AsFixedSizeBytes + Hash + Eq, V: StableType + AsFixedSizeBytes> Drop
    for SHashMap<K, V>
{
    fn drop(&mut self) {
        if !self.is_owned_by_stable_memory() {
            unsafe {
                self.stable_drop();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::hash_map::SHashMap;
    use crate::encoding::AsFixedSizeBytes;
    use crate::primitive::s_box::SBox;
    use crate::primitive::StableType;
    use crate::stable_memory_init;
    use crate::utils::mem_context::stable;
    use rand::seq::SliceRandom;
    use rand::thread_rng;

    #[test]
    fn simple_flow_works_well() {
        stable::clear();
        stable_memory_init();

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

        assert_eq!(*map.get(&k1).unwrap(), 1);
        assert_eq!(*map.get(&k2).unwrap(), 2);
        assert_eq!(*map.get(&k3).unwrap(), 3);
        assert_eq!(*map.get(&k4).unwrap(), 4);
        assert_eq!(*map.get(&k5).unwrap(), 5);
        assert_eq!(*map.get(&k6).unwrap(), 6);
        assert_eq!(*map.get(&k7).unwrap(), 7);
        assert_eq!(*map.get(&k8).unwrap(), 8);

        assert!(map.get(&9).is_none());
        assert!(map.get(&0).is_none());

        assert_eq!(map.remove(&k3).unwrap(), 3);
        assert!(map.get(&k3).is_none());

        assert_eq!(map.remove(&k1).unwrap(), 1);
        assert!(map.get(&k1).is_none());

        assert_eq!(map.remove(&k5).unwrap(), 5);
        assert!(map.get(&k5).is_none());

        assert_eq!(map.remove(&k7).unwrap(), 7);
        assert!(map.get(&k7).is_none());

        assert_eq!(*map.get(&k2).unwrap(), 2);
        assert_eq!(*map.get(&k4).unwrap(), 4);
        assert_eq!(*map.get(&k6).unwrap(), 6);
        assert_eq!(*map.get(&k8).unwrap(), 8);
    }

    #[test]
    fn basic_flow_works_fine() {
        stable::clear();
        stable_memory_init();

        let mut map = SHashMap::new_with_capacity(3);

        assert!(map.remove(&10).is_none());
        assert!(map.get(&10).is_none());

        let it = map.insert(1, 1).unwrap();
        assert!(it.is_none());
        assert!(map.insert(2, 2).unwrap().is_none());
        assert!(map.insert(3, 3).unwrap().is_none());
        assert_eq!(map.insert(1, 10).unwrap().unwrap(), 1);

        assert!(map.remove(&5).is_none());
        assert_eq!(map.remove(&1).unwrap(), 10);

        assert!(map.contains_key(&2));
        assert!(!map.contains_key(&5));

        map.debug_print();

        let mut map = SHashMap::default();
        for i in 0..100 {
            assert!(map.insert(i, i).unwrap().is_none());
        }

        for i in 0..100 {
            assert_eq!(*map.get(&i).unwrap(), i);
        }

        for i in 0..100 {
            assert_eq!(map.remove(&(99 - i)).unwrap(), 99 - i);
        }
    }

    #[test]
    fn removes_work() {
        stable::clear();
        stable_memory_init();

        let mut map = SHashMap::new();

        for i in 0..500 {
            map.insert(499 - i, i);
        }

        let mut vec = (200..300).collect::<Vec<_>>();
        vec.shuffle(&mut thread_rng());

        for i in vec {
            map.remove(&i);
        }

        for i in 500..5000 {
            map.insert(i, i);
        }

        for i in 200..300 {
            map.insert(i, i);
        }

        let mut vec = (0..5000).collect::<Vec<_>>();
        vec.shuffle(&mut thread_rng());

        for i in vec {
            map.remove(&i);
        }
    }

    #[test]
    fn tombstones_work_fine() {
        stable::clear();
        stable_memory_init();

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
    }

    #[test]
    fn serialization_work_fine() {
        stable::clear();
        stable_memory_init();

        let mut map = SHashMap::new();
        map.insert(0, 0);

        let len = map.len();
        let cap = map.capacity();
        let ptr = map.table_ptr;

        let buf = map.as_new_fixed_size_bytes();

        // emulating stable memory save
        unsafe { map.assume_owned_by_stable_memory() };

        let map1 = SHashMap::<i32, i32>::from_fixed_size_bytes(&buf);

        assert_eq!(len, map1.len());
        assert_eq!(cap, map1.capacity());
        assert_eq!(ptr, map1.table_ptr);
    }

    #[test]
    fn iter_works_fine() {
        stable::clear();
        stable_memory_init();

        let mut map = SHashMap::new();
        for i in 0..100 {
            map.insert(i, i);
        }

        let mut c = 0;
        for (mut k, _) in map.iter() {
            c += 1;

            assert!(*k < 100);
        }

        assert_eq!(c, 100);
    }

    #[test]
    fn sboxes_work_fine() {
        stable::clear();
        stable_memory_init();

        let mut map = SHashMap::new();

        for i in 0..100 {
            map.insert(SBox::new(i).unwrap(), i).unwrap();
        }

        println!("sbox mut");
        let mut map = SHashMap::new();

        for i in 0..100 {
            map.insert(SBox::new(i).unwrap(), i).unwrap();
        }
    }
}
