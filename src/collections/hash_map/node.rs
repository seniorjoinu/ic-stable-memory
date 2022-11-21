use crate::mem::s_slice::Side;
use crate::primitive::StableAllocated;
use crate::utils::phantom_data::SPhantomData;
use crate::{allocate, deallocate, SSlice};
use copy_as_bytes::traits::{AsBytes, SuperSized};
use speedy::{Context, LittleEndian, Readable, Reader, Writable, Writer};
use std::hash::{Hash, Hasher};
use zwohash::ZwoHasher;

// BY DEFAULT:
// LEN, CAPACITY: usize = 0
// NEXT: u64 = 0
// KEYS: [K; CAPACITY] = [zeroed(K); CAPACITY]
// VALUES: [V; CAPACITY] = [zeroed(V); CAPACITY]

const LEN_OFFSET: usize = 0;
const CAPACITY_OFFSET: usize = LEN_OFFSET + usize::SIZE;
const NEXT_OFFSET: usize = CAPACITY_OFFSET + usize::SIZE;
const KEYS_OFFSET: usize = NEXT_OFFSET + u64::SIZE;

#[inline]
pub const fn values_offset<K: SuperSized>(capacity: usize) -> usize {
    KEYS_OFFSET + (1 + K::SIZE) * capacity
}

pub const DEFAULT_CAPACITY: usize = 7;

const EMPTY: u8 = 0;
const OCCUPIED: u8 = 255;

pub type KeyHash = usize;

// all for maximum cache-efficiency
// fixed-size, open addressing, linear probing, 3/4 load factor, non-lazy removal (https://stackoverflow.com/a/60709252/7171515)
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

    pub fn hash<T: Hash>(val: &T) -> KeyHash {
        let mut hasher = ZwoHasher::default();
        val.hash(&mut hasher);

        hasher.finish() as KeyHash
    }
}

impl<K: StableAllocated + Hash + Eq, V: StableAllocated> SHashTreeNode<K, V>
where
    [u8; K::SIZE]: Sized,
    [u8; V::SIZE]: Sized,
{
    #[inline]
    pub fn new(capacity: usize) -> Option<Self> {
        if let Some(Some(size)) = (1 + K::SIZE + V::SIZE)
            .checked_mul(capacity)
            .map(|it| it.checked_add(KEYS_OFFSET))
        {
            let table = allocate(size as usize);

            let zeroed = vec![0u8; size as usize];
            table.write_bytes(0, &zeroed);
            table.as_bytes_write(CAPACITY_OFFSET, capacity);

            return Some(Self {
                table_ptr: table.get_ptr(),
                _marker_k: SPhantomData::default(),
                _marker_v: SPhantomData::default(),
            });
        }

        None
    }

    pub fn insert(
        &mut self,
        mut key: K,
        mut value: V,
        capacity: usize,
    ) -> Result<(Option<V>, bool, usize), (K, V)> {
        let key_hash = Self::hash(&key);
        let mut i = key_hash % capacity;

        loop {
            match self.read_key_at(i, true) {
                HashMapKey::Occupied(prev_key) => {
                    if prev_key.eq(&key) {
                        let mut prev_value = self.read_val_at(i, capacity);
                        prev_value.remove_from_stable();

                        value.move_to_stable();
                        self.write_val_at(i, value, capacity);

                        return Ok((Some(prev_value), false, i));
                    } else {
                        i = (i + 1) % capacity;

                        continue;
                    }
                }
                HashMapKey::Empty => {
                    let len = self.len();
                    if self.is_full(len, capacity) {
                        return Err((key, value));
                    }

                    key.move_to_stable();
                    value.move_to_stable();

                    self.write_key_at(i, HashMapKey::Occupied(key));
                    self.write_val_at(i, value, capacity);

                    self.write_len(len + 1);

                    return Ok((None, true, i));
                }
                _ => unreachable!(),
            }
        }
    }

    pub fn remove_by_idx(&mut self, mut i: usize, capacity: usize) -> V {
        let prev_value = self.read_val_at(i, capacity);
        let mut j = i;

        loop {
            j = (j + 1) % capacity;
            if j == i {
                break;
            }
            match self.read_key_at(j, true) {
                HashMapKey::Empty => break,
                HashMapKey::Occupied(next_key) => {
                    let k = Self::hash(&next_key) % capacity;
                    if (j < i) ^ (k <= i) ^ (k > j) {
                        self.write_key_at(i, HashMapKey::Occupied(next_key));
                        self.write_val_at(i, self.read_val_at(j, capacity), capacity);

                        i = j;
                    }
                }
                _ => unreachable!(),
            }
        }

        self.write_key_at(i, HashMapKey::Empty);
        self.write_len(self.read_len() - 1);

        prev_value
    }

    pub fn remove(&mut self, key: &K, capacity: usize) -> Option<V> {
        let (i, _) = self.find_inner_idx(key, capacity)?;

        Some(self.remove_by_idx(i, capacity))
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.read_len()
    }

    #[inline]
    pub const fn is_full(&self, len: usize, capacity: usize) -> bool {
        len == (capacity >> 2) * 3
    }

    pub fn find_inner_idx(&self, key: &K, capacity: usize) -> Option<(usize, K)> {
        let key_hash = Self::hash(key);
        let mut i = key_hash % capacity;

        loop {
            match self.read_key_at(i, true) {
                HashMapKey::Occupied(prev_key) => {
                    if prev_key.eq(key) {
                        return Some((i, prev_key));
                    } else {
                        i = (i + 1) % capacity;
                        continue;
                    }
                }
                HashMapKey::Empty => {
                    return None;
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
    pub fn read_capacity(&self) -> usize {
        SSlice::_as_bytes_read(self.table_ptr, CAPACITY_OFFSET)
    }

    #[inline]
    pub fn read_next(&self) -> u64 {
        SSlice::_as_bytes_read(self.table_ptr, NEXT_OFFSET)
    }

    fn read_key_at(&self, idx: usize, read_value: bool) -> HashMapKey<K> {
        let mut key_flag = [0u8];
        let offset = KEYS_OFFSET + (1 + K::SIZE) * idx;

        SSlice::_read_bytes(self.table_ptr, offset, &mut key_flag);

        match key_flag[0] {
            EMPTY => HashMapKey::Empty,
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
    pub fn read_val_at(&self, idx: usize, capacity: usize) -> V {
        let offset = values_offset::<K>(capacity) + V::SIZE * idx;

        SSlice::_as_bytes_read(self.table_ptr, offset)
    }

    #[inline]
    pub fn write_len(&mut self, len: usize) {
        SSlice::_as_bytes_write(self.table_ptr, LEN_OFFSET, len)
    }

    #[inline]
    pub fn write_capacity(&mut self, capacity: usize) {
        SSlice::_as_bytes_write(self.table_ptr, CAPACITY_OFFSET, capacity)
    }

    #[inline]
    pub fn write_next(&mut self, next: u64) {
        SSlice::_as_bytes_write(self.table_ptr, NEXT_OFFSET, next)
    }

    fn write_key_at(&mut self, idx: usize, key: HashMapKey<K>) {
        let offset = KEYS_OFFSET + (1 + K::SIZE) * idx;

        let key_flag = match key {
            HashMapKey::Empty => [EMPTY],
            HashMapKey::Occupied(k) => {
                SSlice::_as_bytes_write(self.table_ptr, offset + 1, k);

                [OCCUPIED]
            }
            _ => unreachable!(),
        };

        SSlice::_write_bytes(self.table_ptr, offset, &key_flag);
    }

    #[inline]
    fn write_val_at(&mut self, idx: usize, val: V, capacity: usize) {
        let offset = values_offset::<K>(capacity) + V::SIZE * idx;

        SSlice::_as_bytes_write(self.table_ptr, offset, val);
    }

    pub fn debug_print(&self, capacity: usize) {
        print!(
            "Node({}, {}, {})[",
            self.read_len(),
            self.read_capacity(),
            self.read_next(),
        );
        for i in 0..capacity {
            let mut k_flag = [0u8];
            let mut k = [0u8; K::SIZE];
            let mut v = [0u8; V::SIZE];

            SSlice::_read_bytes(self.table_ptr, KEYS_OFFSET + (1 + K::SIZE) * i, &mut k_flag);
            SSlice::_read_bytes(self.table_ptr, KEYS_OFFSET + (1 + K::SIZE) * i + 1, &mut k);
            SSlice::_read_bytes(
                self.table_ptr,
                values_offset::<K>(capacity) + V::SIZE * i,
                &mut v,
            );

            print!("(");

            match k_flag[0] {
                EMPTY => print!("<empty> = "),
                OCCUPIED => print!("<occupied> = "),
                _ => unreachable!(),
            };

            print!("{:?}, {:?})", k, v);

            if i < capacity - 1 {
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
{
    #[inline]
    fn default() -> Self {
        unsafe { Self::new(DEFAULT_CAPACITY).unwrap_unchecked() }
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
