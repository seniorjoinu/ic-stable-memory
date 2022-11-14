use crate::mem::s_slice::Side;
use crate::primitive::StableAllocated;
use crate::utils::phantom_data::SPhantomData;
use crate::{allocate, deallocate, SSlice};
use copy_as_bytes::traits::{AsBytes, SuperSized};
use speedy::{Context, LittleEndian, Readable, Reader, Writable, Writer};
use std::hash::{Hash, Hasher};
use zwohash::ZwoHasher;

// BY DEFAULT:
// LEN: usize = 0
// LEFT, RIGHT, PARENT: u64 = 0
// KEYS: [K; CAPACITY] = [zeroed(K); CAPACITY]
// VALUES: [V; CAPACITY] = [zeroed(V); CAPACITY]

const LEN_OFFSET: usize = 0;
const LEFT_OFFSET: usize = LEN_OFFSET + usize::SIZE;
const RIGHT_OFFSET: usize = LEFT_OFFSET + u64::SIZE;
const PARENT_OFFSET: usize = RIGHT_OFFSET + u64::SIZE;
const KEYS_OFFSET: usize = PARENT_OFFSET + u64::SIZE;

pub const fn values_offset<K: SuperSized>() -> usize {
    KEYS_OFFSET + (1 + K::SIZE) * CAPACITY
}

pub const CAPACITY: usize = 12;
pub const HALF_CAPACITY: usize = 6;
pub const THREE_QUARTERS_CAPACITY: usize = 9;
const LAST_IDX: usize = CAPACITY - 1;

const EMPTY: u8 = 0;
const OCCUPIED: u8 = 1;
const TOMBSTONE: u8 = 255;

pub type KeyHash = usize;

// reallocating, open addressing, quadratic probing small hashmap
pub struct SHashTreeNode<K, V> {
    pub(crate) table_ptr: u64,
    _marker_k: SPhantomData<K>,
    _marker_v: SPhantomData<V>,
}

impl<K, V> SHashTreeNode<K, V> {
    #[inline]
    pub unsafe fn from_ptr(table_ptr: u64) -> Self {
        Self {
            table_ptr,
            _marker_k: SPhantomData::default(),
            _marker_v: SPhantomData::default(),
        }
    }

    #[inline]
    pub unsafe fn copy(&self) -> Self {
        Self {
            table_ptr: self.table_ptr,
            _marker_k: SPhantomData::default(),
            _marker_v: SPhantomData::default(),
        }
    }

    #[inline]
    pub unsafe fn stable_drop_collection(&mut self) {
        let slice = SSlice::from_ptr(self.table_ptr, Side::Start).unwrap();
        deallocate(slice);
    }

    #[inline]
    pub fn hash<T: Hash>(&self, val: &T, level: u64) -> KeyHash {
        let mut hasher = ZwoHasher::default();
        val.hash(&mut hasher);
        level.hash(&mut hasher);

        hasher.finish() as KeyHash
    }
}

impl<K: StableAllocated + Hash + Eq, V: StableAllocated> SHashTreeNode<K, V>
where
    [u8; K::SIZE]: Sized,
    [u8; V::SIZE]: Sized,
    [u8; values_offset::<K>() + V::SIZE * CAPACITY]: Sized,
{
    #[inline]
    pub fn new() -> Self {
        let size = values_offset::<K>() + V::SIZE * CAPACITY;
        let table = allocate(size);

        let zeroed = [0u8; values_offset::<K>() + V::SIZE * CAPACITY];
        table.write_bytes(0, &zeroed);

        Self {
            table_ptr: table.get_ptr(),
            _marker_k: SPhantomData::default(),
            _marker_v: SPhantomData::default(),
        }
    }

    pub fn insert(
        &mut self,
        mut key: K,
        mut value: V,
        level: u64,
    ) -> Result<(Option<V>, bool), (K, V, KeyHash)> {
        let key_hash = self.hash(&key, level);

        let mut i = 0;

        let mut remembered_at = None;

        loop {
            let at = Self::calculate_next_index(key_hash, i, CAPACITY);

            i += 1;

            match self.read_key_at(at, true) {
                HashMapKey::Occupied(prev_key) => {
                    if prev_key.eq(&key) {
                        let mut prev_value = self.read_val_at(at);
                        prev_value.remove_from_stable();

                        value.move_to_stable();
                        self.write_val_at(at, value);

                        return Ok((Some(prev_value), false));
                    } else {
                        if i == LAST_IDX {
                            let len = self.len();
                            if self.is_full(len) {
                                return Err((key, value, key_hash));
                            }

                            let at = remembered_at.unwrap();

                            key.move_to_stable();
                            value.move_to_stable();

                            self.write_key_at(at, HashMapKey::Occupied(key));
                            self.write_val_at(at, value);

                            self.write_len(len + 1);

                            return Ok((None, true));
                        }

                        continue;
                    }
                }
                HashMapKey::Tombstone => {
                    if remembered_at.is_none() {
                        remembered_at = Some(at);
                    }

                    if i == LAST_IDX {
                        let len = self.len();
                        if self.is_full(len) {
                            return Err((key, value, key_hash));
                        }

                        let at = remembered_at.unwrap();

                        key.move_to_stable();
                        value.move_to_stable();

                        self.write_key_at(at, HashMapKey::Occupied(key));
                        self.write_val_at(at, value);

                        self.write_len(len + 1);

                        return Ok((None, true));
                    }

                    continue;
                }
                HashMapKey::Empty => {
                    let len = self.len();
                    if self.is_full(len) {
                        return Err((key, value, key_hash));
                    }

                    let at = if let Some(a) = remembered_at { a } else { at };

                    key.move_to_stable();
                    value.move_to_stable();

                    self.write_key_at(at, HashMapKey::Occupied(key));
                    self.write_val_at(at, value);

                    self.write_len(len + 1);

                    return Ok((None, true));
                }
                _ => unreachable!(),
            }
        }
    }

    pub fn replace_internal_not_full(&mut self, key: K, value: V, level: u64) {
        let key_hash = self.hash(&key, level);

        let mut i = 0;

        loop {
            let at = Self::calculate_next_index(key_hash, i, CAPACITY);

            i += 1;

            match self.read_key_at(at, false) {
                HashMapKey::OccupiedNull => {
                    continue;
                }
                HashMapKey::Occupied(_) => unreachable!(),
                _ => {
                    self.write_key_at(at, HashMapKey::Occupied(key));
                    self.write_val_at(at, value);

                    break;
                }
            }
        }
    }

    pub fn remove_internal_no_len_mod(&mut self, prev_key: &mut K, idx: usize) -> V {
        let mut prev_value = self.read_val_at(idx);

        prev_key.remove_from_stable();
        prev_value.remove_from_stable();

        self.write_key_at(idx, HashMapKey::Tombstone);

        prev_value
    }

    pub fn take_any_leaf_non_empty_no_len_mod(&mut self) -> (K, V) {
        for i in 0..CAPACITY {
            let k = self.read_key_at(i, true);
            match k {
                HashMapKey::Empty => continue,
                HashMapKey::Tombstone => continue,
                HashMapKey::Occupied(key) => {
                    let value = self.read_val_at(i);
                    self.write_key_at(i, HashMapKey::Tombstone);

                    return (key, value);
                }
                HashMapKey::OccupiedNull => unreachable!(),
            }
        }

        unreachable!();
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.read_len()
    }

    #[inline]
    pub const fn is_full(&self, len: usize) -> bool {
        len == THREE_QUARTERS_CAPACITY
    }

    pub fn find_inner_idx(&self, key: &K, level: u64) -> Result<(usize, K, KeyHash), KeyHash> {
        let key_hash = self.hash(key, level);
        let mut i = 0;

        loop {
            let at = Self::calculate_next_index(key_hash, i, CAPACITY);

            i += 1;

            match self.read_key_at(at, true) {
                HashMapKey::Occupied(prev_key) => {
                    if prev_key.eq(key) {
                        return Ok((at, prev_key, key_hash));
                    } else {
                        if i == LAST_IDX {
                            return Err(key_hash);
                        }

                        continue;
                    }
                }
                HashMapKey::Tombstone => {
                    if i == LAST_IDX {
                        return Err(key_hash);
                    }

                    continue;
                }
                HashMapKey::Empty => {
                    return Err(key_hash);
                }
                _ => unreachable!(),
            };
        }
    }

    #[inline]
    pub fn read_len(&self) -> usize {
        SSlice::_as_bytes_read(self.table_ptr, LEN_OFFSET)
    }

    #[inline]
    pub fn read_left(&self) -> u64 {
        SSlice::_as_bytes_read(self.table_ptr, LEFT_OFFSET)
    }

    #[inline]
    pub fn read_right(&self) -> u64 {
        SSlice::_as_bytes_read(self.table_ptr, RIGHT_OFFSET)
    }

    #[inline]
    pub fn read_parent(&self) -> u64 {
        SSlice::_as_bytes_read(self.table_ptr, PARENT_OFFSET)
    }

    fn read_key_at(&self, idx: usize, read_value: bool) -> HashMapKey<K> {
        let mut key_flag = [0u8];
        let offset = KEYS_OFFSET + (1 + K::SIZE) * idx;

        SSlice::_read_bytes(self.table_ptr, offset, &mut key_flag);

        match key_flag[0] {
            EMPTY => HashMapKey::Empty,
            TOMBSTONE => HashMapKey::Tombstone,
            OCCUPIED => {
                if read_value {
                    let k = SSlice::_as_bytes_read(self.table_ptr, offset + 1);

                    HashMapKey::Occupied(k)
                } else {
                    HashMapKey::OccupiedNull
                }
            }
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn read_val_at(&self, idx: usize) -> V {
        let offset = values_offset::<K>() + V::SIZE * idx;

        SSlice::_as_bytes_read(self.table_ptr, offset)
    }

    #[inline]
    pub fn write_len(&mut self, len: usize) {
        SSlice::_as_bytes_write(self.table_ptr, LEN_OFFSET, len)
    }

    #[inline]
    pub fn write_left(&mut self, left: u64) {
        SSlice::_as_bytes_write(self.table_ptr, LEFT_OFFSET, left)
    }

    #[inline]
    pub fn write_right(&mut self, right: u64) {
        SSlice::_as_bytes_write(self.table_ptr, RIGHT_OFFSET, right)
    }

    #[inline]
    pub fn write_parent(&mut self, parent: u64) {
        SSlice::_as_bytes_write(self.table_ptr, PARENT_OFFSET, parent)
    }

    fn write_key_at(&mut self, idx: usize, key: HashMapKey<K>) {
        let offset = KEYS_OFFSET + (1 + K::SIZE) * idx;

        let key_flag = match key {
            HashMapKey::Empty => [EMPTY],
            HashMapKey::Tombstone => [TOMBSTONE],
            HashMapKey::Occupied(k) => {
                SSlice::_as_bytes_write(self.table_ptr, offset + 1, k);

                [OCCUPIED]
            }
            _ => unreachable!(),
        };

        SSlice::_write_bytes(self.table_ptr, offset, &key_flag);
    }

    #[inline]
    fn write_val_at(&mut self, idx: usize, val: V) {
        let offset = values_offset::<K>() + V::SIZE * idx;

        SSlice::_as_bytes_write(self.table_ptr, offset, val);
    }

    #[inline]
    const fn calculate_next_index(key_n_hash: usize, i: usize, capacity: usize) -> usize {
        (key_n_hash + i) % capacity
    }

    pub fn debug_print(&self) {
        print!(
            "Node({}, {}, {}, {})[",
            self.read_len(),
            self.read_left(),
            self.read_right(),
            self.read_parent()
        );
        for i in 0..CAPACITY {
            let mut k_flag = [0u8];
            let mut k = [0u8; K::SIZE];
            let mut v = [0u8; V::SIZE];

            SSlice::_read_bytes(self.table_ptr, KEYS_OFFSET + (1 + K::SIZE) * i, &mut k_flag);
            SSlice::_read_bytes(self.table_ptr, KEYS_OFFSET + (1 + K::SIZE) * i + 1, &mut k);
            SSlice::_read_bytes(self.table_ptr, values_offset::<K>() + V::SIZE * i, &mut v);

            print!("(");

            match k_flag[0] {
                EMPTY => print!("<empty> = "),
                TOMBSTONE => print!("<tombstone> = "),
                OCCUPIED => print!("<occupied> = "),
                _ => unreachable!(),
            };

            print!("{:?}, {:?})", k, v);

            if i < CAPACITY - 1 {
                print!(", ");
            }
        }
        println!("]");
    }
}

impl<K: StableAllocated + Hash + Eq, V: StableAllocated> Default for SHashTreeNode<K, V>
where
    [u8; K::SIZE]: Sized,
    [u8; V::SIZE]: Sized,
    [u8; values_offset::<K>() + V::SIZE * CAPACITY]: Sized,
{
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, K, V> Readable<'a, LittleEndian> for SHashTreeNode<K, V> {
    fn read_from<R: Reader<'a, LittleEndian>>(
        reader: &mut R,
    ) -> Result<Self, <speedy::LittleEndian as Context>::Error> {
        let table_ptr = reader.read_u64()?;

        let it = Self {
            table_ptr,
            _marker_k: SPhantomData::default(),
            _marker_v: SPhantomData::default(),
        };

        Ok(it)
    }
}

impl<K, V> Writable<LittleEndian> for SHashTreeNode<K, V> {
    fn write_to<T: ?Sized + Writer<LittleEndian>>(
        &self,
        writer: &mut T,
    ) -> Result<(), <speedy::LittleEndian as Context>::Error> {
        writer.write_u64(self.table_ptr)
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

impl<K, V> SuperSized for SHashTreeNode<K, V> {
    const SIZE: usize = u64::SIZE;
}

impl<K, V> AsBytes for SHashTreeNode<K, V> {
    fn to_bytes(self) -> [u8; Self::SIZE] {
        self.table_ptr.to_bytes()
    }

    fn from_bytes(arr: [u8; Self::SIZE]) -> Self {
        let table_ptr = u64::from_bytes(arr);

        Self {
            table_ptr,
            _marker_k: SPhantomData::default(),
            _marker_v: SPhantomData::default(),
        }
    }
}

/*impl<K: StableAllocated + Eq + Hash, V: StableAllocated> StableAllocated for SHashMapNode<K, V>
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
}*/

#[cfg(test)]
mod tests {
    // TODO:
}
