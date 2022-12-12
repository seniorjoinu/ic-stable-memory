use crate::mem::s_slice::Side;
use crate::utils::certification::{MerkleChild, MerkleNode, Sha256Digest, EMPTY_SHA256};
use crate::{allocate, deallocate, SSlice};
use copy_as_bytes::traits::{AsBytes, SuperSized};
use sha2::{Digest, Sha256};
use speedy::{Context, LittleEndian, Readable, Reader, Writable, Writer};
use std::hash::Hasher;

// BY DEFAULT:
// LEN, CAPACITY: usize = 0
// NEXT: u64 = 0
// NODE_HASHES: [Sha256Digest; CAPACITY] = [zeroed(Sha256Digest); CAPACITY]
// ENTRY_HASHES: [Sha256Digest; CAPACITY] = [zeroed(Sha256Digest); CAPACITY]

const LEN_OFFSET: usize = 0;
const CAPACITY_OFFSET: usize = LEN_OFFSET + usize::SIZE;
const NEXT_OFFSET: usize = CAPACITY_OFFSET + usize::SIZE;
const NODE_HASHES_OFFSET: usize = NEXT_OFFSET + u64::SIZE;

#[inline]
pub const fn entry_hashes_offset(capacity: usize) -> usize {
    NODE_HASHES_OFFSET + Sha256Digest::SIZE * capacity
}

#[inline]
pub const fn entry_hash_idx_offset(idx: usize, capacity: usize) -> usize {
    NODE_HASHES_OFFSET + Sha256Digest::SIZE * capacity + (1 + Sha256Digest::SIZE) * idx
}

pub const DEFAULT_CAPACITY: usize = 7;
//pub const MAX_CAPACITY: usize = 2usize.pow(26);
pub const MAX_CAPACITY: usize = 2usize.pow(22);

const EMPTY: u8 = 0;
const OCCUPIED: u8 = 255;

pub type KeyHash = usize;

// all for maximum cache-efficiency
// fixed-size, open addressing, linear probing, 3/4 load factor, non-lazy removal (https://stackoverflow.com/a/60709252/7171515)
pub struct SCertifiedHashMapNode {
    pub(crate) table_ptr: u64,
}

impl SCertifiedHashMapNode {
    #[inline]
    pub unsafe fn from_ptr(table_ptr: u64) -> Self {
        Self { table_ptr }
    }

    #[inline]
    pub unsafe fn copy(&self) -> Self {
        Self {
            table_ptr: self.table_ptr,
        }
    }

    #[inline]
    pub unsafe fn stable_drop_collection(&mut self) {
        let slice = SSlice::from_ptr(self.table_ptr, Side::Start).unwrap();
        deallocate(slice);
    }
}

impl SCertifiedHashMapNode {
    #[inline]
    pub fn new(capacity: usize) -> Option<Self> {
        if capacity >= MAX_CAPACITY {
            return None;
        }

        let bytes_capacity_opt = entry_hashes_offset(capacity) + Sha256Digest::SIZE * capacity;

        if let Some(Some(size)) = bytes_capacity_opt {
            let table = allocate(size as usize);

            let zeroed = vec![0u8; size as usize];
            table.write_bytes(0, &zeroed);
            table.as_bytes_write(CAPACITY_OFFSET, capacity);

            return Some(Self {
                table_ptr: table.get_ptr(),
            });
        }

        None
    }

    pub fn insert(&mut self, hash: Sha256Digest, capacity: usize) -> Option<(bool, usize)> {
        let mut i = Self::hash_to_idx(&hash) % capacity;

        loop {
            match self.read_entry_hash_at(i, true, capacity) {
                HashMapKey::Occupied(found_hash) => {
                    if found_hash.eq(&hash) {
                        return Some((false, i));
                    } else {
                        i = (i + 1) % capacity;

                        continue;
                    }
                }
                HashMapKey::Empty => {
                    let len = self.len();
                    if self.is_full(len, capacity) {
                        return None;
                    }

                    self.write_len(len + 1);
                    self.write_entry_hash_at(i, HashMapKey::Occupied(hash), capacity);

                    return Some((true, i));
                }
                _ => unreachable!(),
            }
        }
    }

    pub fn remove_by_idx(
        &mut self,
        mut i: usize,
        capacity: usize,
        modified_indices: &mut Vec<usize>,
    ) {
        self.write_len(self.read_len() - 1);

        let mut j = i;

        loop {
            j = (j + 1) % capacity;
            if j == i {
                break;
            }
            match self.read_entry_hash_at(j, true, capacity) {
                HashMapKey::Empty => break,
                HashMapKey::Occupied(next_hash) => {
                    let k = Self::hash_to_idx(&next_hash) % capacity;

                    if (j < i) ^ (k <= i) ^ (k > j) {
                        self.write_entry_hash_at(i, HashMapKey::Occupied(next_hash), capacity);

                        if let Err(idx) = modified_indices.binary_search(&i) {
                            modified_indices.insert(idx, i);
                        }

                        i = j;
                    }
                }
                _ => unreachable!(),
            }
        }

        self.write_entry_hash_at(i, HashMapKey::Empty, capacity);

        if let Err(idx) = modified_indices.binary_search(&i) {
            modified_indices.insert(idx, i);
        }
    }

    pub fn remove(
        &mut self,
        hash: &Sha256Digest,
        capacity: usize,
        modified_indices: &mut Vec<usize>,
    ) -> bool {
        if let Some((i, _)) = self.find_inner_idx(hash, capacity) {
            self.remove_by_idx(i, capacity, modified_indices);

            true
        } else {
            false
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.read_len()
    }

    #[inline]
    pub const fn is_full(&self, len: usize, capacity: usize) -> bool {
        if capacity < 12 {
            len == capacity * 3 / 4
        } else {
            len == capacity / 4 * 3
        }
    }

    pub fn find_inner_idx(
        &self,
        hash: &Sha256Digest,
        capacity: usize,
    ) -> Option<(usize, Sha256Digest)> {
        let mut i = Self::hash_to_idx(hash) % capacity;

        loop {
            match self.read_entry_hash_at(i, true, capacity) {
                HashMapKey::Occupied(found_hash) => {
                    if found_hash.eq(hash) {
                        return Some((i, found_hash));
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

    pub fn witness_indices(
        &self,
        sorted_indices: &mut Vec<usize>,
        sorted_nodes_tmp: &mut Vec<MerkleNode>,
        capacity: usize,
    ) -> MerkleNode {
        // insert root anyway, even if it was not requested
        // this way even if no indices were passed to this function, it will return the valid root node
        if sorted_indices.is_empty() || sorted_indices[0] != 0 {
            sorted_indices.insert(0, 0);
        }

        for i in &sorted_indices {
            sorted_nodes_tmp.push(MerkleNode::new(
                self.read_entry_hash_at_anyway(i, capacity),
                MerkleChild::None,
                MerkleChild::None,
            ));
        }

        while sorted_nodes_tmp.len() > 1 {
            let mut last_idx = unsafe { sorted_indices.pop().unwrap_unchecked() };
            let mut last_node = unsafe { sorted_nodes_tmp.pop().unwrap_unchecked() };

            if matches!(last_node.left_child, MerkleChild::None) {
                let lc = if last_idx >= (capacity - 1) / 2 {
                    EMPTY_SHA256
                } else {
                    self.read_node_hash_at((last_idx + 1) * 2 - 1)
                };

                last_node.left_child = MerkleChild::Pruned(lc);
            }

            if matches!(last_node.right_child, MerkleChild::None) {
                let rc = if last_idx >= (capacity - 1) / 2 {
                    EMPTY_SHA256
                } else {
                    self.read_node_hash_at((last_idx + 1) * 2)
                };

                last_node.right_child = MerkleChild::Pruned(rc);
            }

            let is_left = last_idx % 2 == 1;

            last_idx /= 2;
            match sorted_indices.binary_search(&last_idx) {
                Ok(parent_idx) => {
                    // parent already exists in the tree
                    let mut parent = sorted_nodes_tmp[parent_idx];

                    if is_left {
                        debug_assert!(matches!(parent.left_child, MerkleChild::None));
                        parent.left_child = MerkleChild::Hole(last_node);
                    } else {
                        debug_assert!(matches!(parent.right_child, MerkleChild::None));
                        parent.right_child = MerkleChild::Hole(last_node);
                    }
                }
                Err(parent_idx) => {
                    let entry_hash = self.read_entry_hash_at_anyway(last_idx, capacity);

                    let parent = if is_left {
                        MerkleNode::new(entry_hash, MerkleChild::Hole(last_node), MerkleChild::None)
                    } else {
                        MerkleNode::new(entry_hash, MerkleChild::None, MerkleChild::Hole(last_node))
                    };

                    sorted_indices.insert(parent_idx, last_idx);
                    sorted_nodes_tmp.insert(parent_idx, parent);
                }
            }
        }

        sorted_indices.pop();

        let mut merkle_root = sorted_nodes_tmp.pop().unwrap();

        if matches!(merkle_root.left_child, MerkleChild::None) {
            let lc = self.read_node_hash_at(1);
            merkle_root.left_child = MerkleChild::Pruned(lc);
        }

        if matches!(merkle_root.right_child, MerkleChild::None) {
            let rc = self.read_node_hash_at(2);
            merkle_root.right_child = MerkleChild::Pruned(rc);
        }

        merkle_root
    }

    pub fn recalculate_merkle_tree(
        &mut self,
        sorted_indices: &mut Vec<usize>,
        capacity: usize,
        hasher: &mut Sha256,
    ) {
        while let Some(last_idx) = sorted_indices.pop() {
            let (lc_hash, rc_hash) = self.read_children_node_hashes_of(last_idx, capacity);

            let entry_hash = self.read_entry_hash_at_anyway(last_idx, capacity);
            let node_hash = Self::sha256_node(entry_hash, lc_hash, rc_hash, hasher);

            self.write_node_hash_at(last_idx, node_hash);

            let parent_idx = last_idx / 2;

            if !sorted_indices.is_empty() {
                match sorted_indices.binary_search(&parent_idx) {
                    Ok(_) => {}
                    Err(idx) => sorted_indices.insert(idx, parent_idx),
                };
            }
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

    fn read_entry_hash_at(
        &self,
        idx: usize,
        read_value: bool,
        capacity: usize,
    ) -> HashMapKey<Sha256Digest> {
        let mut key_flag = [0u8];
        let offset = entry_hash_idx_offset(idx, capacity);

        SSlice::_read_bytes(self.table_ptr, offset, &mut key_flag);

        match key_flag[0] {
            EMPTY => HashMapKey::Empty,
            OCCUPIED => {
                if read_value {
                    let hash = SSlice::_as_bytes_read(self.table_ptr, offset + 1);

                    HashMapKey::Occupied(hash)
                } else {
                    HashMapKey::OccupiedNull
                }
            }
            _ => unreachable!(),
        }
    }

    fn read_entry_hash_at_anyway(&self, idx: usize, capacity: usize) -> Sha256Digest {
        match self.read_entry_hash_at(idx, true, capacity) {
            HashMapKey::Empty => EMPTY_SHA256,
            HashMapKey::Occupied(hash) => hash,
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn read_node_hash_at(&self, idx: usize) -> Sha256Digest {
        let offset = NODE_HASHES_OFFSET + Sha256Digest::SIZE * idx;

        SSlice::_as_bytes_read(self.table_ptr, offset)
    }

    fn read_children_node_hashes_of(
        &self,
        idx: usize,
        capacity: usize,
    ) -> (Sha256Digest, Sha256Digest) {
        if idx >= (capacity - 1) / 2 {
            (EMPTY_SHA256, EMPTY_SHA256)
        } else {
            (
                self.read_node_hash_at((idx + 1) * 2 - 1),
                self.read_node_hash_at((idx + 1) * 2),
            )
        }
    }

    #[inline]
    pub fn read_root_hash(&self) -> Sha256Digest {
        self.read_node_hash_at(0)
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

    fn write_entry_hash_at(&mut self, idx: usize, hash: HashMapKey<Sha256Digest>, capacity: usize) {
        let offset = entry_hash_idx_offset(idx, capacity);

        let key_flag = match hash {
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
    fn write_node_hash_at(&mut self, idx: usize, node_hash: Sha256Digest) {
        let offset = NODE_HASHES_OFFSET + Sha256Digest::SIZE * idx;

        SSlice::_as_bytes_write(self.table_ptr, offset, node_hash);
    }

    pub fn debug_print(&self, capacity: usize) {
        let capacity = self.read_capacity();
        print!(
            "Node({}, {}, {})[",
            self.read_len(),
            capacity,
            self.read_next(),
        );

        for i in 0..capacity {
            let mut k_flag = [0u8];
            let mut entry_hash = [0u8; Sha256Digest::SIZE];
            let mut node_hash = [0u8; Sha256Digest::SIZE];

            SSlice::_read_bytes(
                self.table_ptr,
                entry_hash_idx_offset(i, capacity),
                &mut k_flag,
            );
            SSlice::_read_bytes(
                self.table_ptr,
                entry_hash_idx_offset(i, capacity) + 1,
                &mut entry_hash,
            );
            SSlice::_read_bytes(
                self.table_ptr,
                NODE_HASHES_OFFSET + Sha256Digest::SIZE * i,
                &mut node_hash,
            );

            print!("(");

            match k_flag[0] {
                EMPTY => print!("<empty> = "),
                OCCUPIED => print!("<occupied> = "),
                _ => unreachable!(),
            };

            print!("entry_hash: {:?}, node_hash: {:?})", entry_hash, node_hash);

            if i < capacity - 1 {
                print!(", ");
            }
        }
        println!("]");
    }

    pub fn hash_to_idx(hash: &Sha256Digest) -> KeyHash {
        let mut buf = KeyHash::super_size_u8_arr();

        buf.copy_from_slice(hash[..KeyHash::SIZE]);
        KeyHash::from_bytes(buf)
    }
}

impl SCertifiedHashMapNode {
    pub fn sha256_node(
        entry_sha256: Sha256Digest,
        lc_sha256: Sha256Digest,
        rc_sha256: Sha256Digest,
        hasher: &mut Sha256,
    ) -> Sha256Digest {
        hasher.update(entry_sha256);
        hasher.update(lc_sha256);
        hasher.update(rc_sha256);

        hasher.finalize_reset().into()
    }
}

impl Default for SCertifiedHashMapNode {
    #[inline]
    fn default() -> Self {
        unsafe { Self::new(DEFAULT_CAPACITY).unwrap_unchecked() }
    }
}

impl<'a> Readable<'a, LittleEndian> for SCertifiedHashMapNode {
    fn read_from<R: Reader<'a, LittleEndian>>(
        reader: &mut R,
    ) -> Result<Self, <speedy::LittleEndian as Context>::Error> {
        let table_ptr = reader.read_u64()?;

        let it = Self { table_ptr };

        Ok(it)
    }
}

impl Writable<LittleEndian> for SCertifiedHashMapNode {
    fn write_to<W: ?Sized + Writer<LittleEndian>>(
        &self,
        writer: &mut W,
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

impl SuperSized for SCertifiedHashMapNode {
    const SIZE: usize = u64::SIZE;
}

impl AsBytes for SCertifiedHashMapNode {
    fn to_bytes(self) -> [u8; Self::SIZE] {
        self.table_ptr.to_bytes()
    }

    fn from_bytes(arr: [u8; Self::SIZE]) -> Self {
        let table_ptr = u64::from_bytes(arr);

        Self { table_ptr }
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
