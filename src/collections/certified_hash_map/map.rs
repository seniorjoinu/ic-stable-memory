use crate::collections::certified_hash_map::node::SCertifiedHashMapNode;
use crate::primitive::StableAllocated;
use crate::utils::certification::{
    MerkleChild, MerkleHash, MerkleNode, MerkleWitness, Sha256Digest, EMPTY_SHA256,
};
use sha2::{Digest, Sha256};

struct Bucket {
    pub ptr: u64,
    modified_indices: Vec<usize>,
}

impl Bucket {
    pub fn new(ptr: u64) -> Self {
        Bucket {
            ptr,
            modified_indices: Vec::new(),
        }
    }

    pub fn add_modified_index(&mut self, idx: usize) {
        if let Err(i) = self.modified_indices.binary_search(&idx) {
            self.modified_indices.insert(i, idx)
        }
    }

    pub fn get_modified_indices(&mut self) -> &mut Vec<usize> {
        &mut self.modified_indices
    }
}

// non-reallocating big hash map based on rope data structure
// linked list of hashmaps, from big ones to small ones
// infinite; both: logarithmic and amortized const
pub struct SCertifiedSet<H = Sha256> {
    root: Option<SCertifiedHashMapNode>,
    len: u64,
    tip_of_the_tree: Vec<Sha256Digest>,
    batch_started: bool,
    buckets: Vec<Bucket>,
    hasher: Sha256,
}

impl SCertifiedSet {
    #[inline]
    pub fn new() -> Self {
        Self {
            root: None,
            len: 0,
            tip_of_the_tree: Vec::new(),
            buckets: Vec::new(),
            batch_started: false,
            hasher: Sha256::default(),
        }
    }

    #[inline]
    pub fn len(&self) -> u64 {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn insert(&mut self, hash: Sha256Digest) -> bool {
        let mut bucket = self.get_or_create_root();
        let mut bucket_idx = 0usize;

        let mut capacity = bucket.read_capacity();
        let root_capacity = capacity;

        loop {
            // if there was an empty space in this bucket - insert
            if let Some((should_update_len, modified_idx)) = bucket.insert(hash, capacity) {
                if should_update_len {
                    self.buckets[bucket_idx].add_modified_index(modified_idx);

                    self.len += 1;

                    if !self.batch_started {
                        self.recalculate_merkle_tree();
                    }
                }

                return should_update_len;
            }

            let next = bucket.read_next();

            // if there is no next - create new bucket and make it the new head
            bucket = if next == 0 {
                let mut new_root_capacity = root_capacity * 2 - 1;
                let mut new_root =
                    if let Some(new_root) = SCertifiedHashMapNode::new(new_root_capacity) {
                        new_root
                    } else {
                        new_root_capacity = root_capacity;
                        unsafe { SCertifiedHashMapNode::new(root_capacity).unwrap_unchecked() }
                    };
                self.buckets.insert(0, Bucket::new(new_root.table_ptr));

                let root = self.get_root_unchecked();
                new_root.write_next(root.table_ptr);

                capacity = new_root_capacity;
                self.root = Some(unsafe { new_root.copy() });

                let (_, modified_idx) = new_root.insert(hash, capacity).unwrap();

                self.buckets[0].add_modified_index(modified_idx);

                if !self.batch_started {
                    self.recalculate_merkle_tree();
                }

                self.len += 1;

                return true;
            } else {
                // if there is next - try inserting again
                let next = unsafe { SCertifiedHashMapNode::from_ptr(next) };
                capacity = next.read_capacity();
                bucket_idx += 1;

                next
            };
        }
    }

    pub fn remove(&mut self, hash: &Sha256Digest) -> bool {
        if self.is_empty() {
            return None;
        }

        let mut bucket = self.get_or_create_root();
        let mut bucket_idx = 0usize;
        let mut modified_indices = self.buckets[bucket_idx].get_modified_indices();
        let mut capacity = bucket.read_capacity();

        loop {
            if bucket.remove(hash, capacity, modified_indices) {
                self.len -= 1;

                if !self.batch_started {
                    self.recalculate_merkle_tree();
                }

                return true;
            }

            let next = bucket.read_next();
            if next == 0 {
                return false;
            }

            bucket = unsafe { SCertifiedHashMapNode::from_ptr(next) };
            capacity = bucket.read_capacity();
            bucket_idx += 1;
            modified_indices = self.buckets[bucket_idx].get_modified_indices();
        }
    }

    pub fn contains(&self, hash: &Sha256Digest) -> bool {
        self.find_hash(hash).is_some()
    }

    pub fn start_batch(&mut self) {
        assert!(!self.batch_started);
        self.batch_started = true;
    }

    pub fn end_batch(&mut self) {
        assert!(self.batch_started);

        self.recalculate_merkle_tree();
        self.batch_started = false;
    }

    fn recalculate_merkle_tree(&mut self) {
        let buckets_len = self.buckets.len();
        if buckets_len == 0 {
            return;
        }

        self.tip_of_the_tree.clear();
        for i in 0..buckets_len {
            self.tip_of_the_tree.push(EMPTY_SHA256);
        }

        for bucket_info in &mut self.buckets {
            let mut bucket = unsafe { SCertifiedHashMapNode::from_ptr(bucket_info.ptr) };
            let capacity = bucket.read_capacity();

            bucket.recalculate_merkle_tree(
                bucket_info.get_modified_indices(),
                capacity,
                &mut self.hasher,
            );
            self.tip_of_the_tree.push(bucket.read_root_hash());
        }

        let mut i = buckets_len - 1;

        loop {
            self.hasher.update(EMPTY_SHA256);

            let right_child_idx = (i + 1) * 2;
            let left_child_idx = right_child_idx - 1;

            self.hasher.update(self.tip_of_the_tree[left_child_idx]);

            if right_child_idx < self.tip_of_the_tree.len() {
                self.hasher.update(self.tip_of_the_tree[right_child_idx]);
            } else {
                self.hasher.update(EMPTY_SHA256);
            }

            self.tip_of_the_tree[i] = self.hasher.finalize_reset().into();

            if i == 0 {
                break;
            }

            i -= 1;
        }
    }

    pub fn witness_hashes(&self, mut hashes: Vec<Sha256Digest>) -> Option<MerkleNode> {
        if self.is_empty() {
            return None;
        }

        let mut bucket = self.get_root_unchecked();
        let mut capacity = bucket.read_capacity();
        let mut merkle_roots = Vec::new();

        let mut inner_indices = Vec::new();
        let mut hashes_indices_to_remove = Vec::new();

        loop {
            let mut found = false;

            for i in 0..hashes.len() {
                if let Some((inner_idx, _)) = bucket.find_inner_idx(&hashes[i], capacity) {
                    let j = inner_indices.binary_search(&inner_idx).unwrap_err();
                    inner_indices.insert(j, inner_idx);

                    found = true;

                    hashes_indices_to_remove.push(i);
                }
            }

            // TODO: pass tmp vec for sorted_nodes also
            let merkle_root = bucket.witness_indices(inner_indices, capacity);
            merkle_roots.push((merkle_root, found));

            for i in 0..hashes_indices_to_remove.len() {
                hashes.remove(hashes_indices_to_remove.pop());
            }

            if hashes.is_empty() {
                break;
            }

            let next = bucket.read_next();
            if next == 0 {
                break;
            }

            bucket = unsafe { SCertifiedHashMapNode::from_ptr(next) };
            capacity = bucket.read_capacity();
        }

        if !hashes.is_empty() {
            return None;
        }

        let mut last_idx = merkle_roots.len() - 1;
        while last_idx > 0 {
            let (last_root, found) = unsafe { merkle_roots.pop().unwrap_unchecked() };

            let is_left = last_idx % 2 == 1;
            let parent_idx = last_idx / 2;

            last_idx -= 1;

            if !found {
                continue;
            }

            merkle_roots[parent_idx].1 = true;

            if is_left {
                merkle_roots[parent_idx].0.additional_left_child = MerkleChild::Hole(last_root);
            } else {
                merkle_roots[parent_idx].0.additional_right_child = MerkleChild::Hole(last_root)
            }
        }

        Some(merkle_roots[0].0)
    }

    #[inline]
    pub fn get_root_hash(&self) -> Sha256Digest {
        self.tip_of_the_tree
            .get(0)
            .map(|it| *it)
            .unwrap_or(EMPTY_SHA256)
    }

    fn find_hash(&self, hash: &Sha256Digest) -> Option<(SCertifiedHashMapNode, usize, usize)> {
        if self.is_empty() {
            return None;
        }

        let mut node = self.get_root_unchecked();
        let mut capacity = node.read_capacity();

        // TODO: APPLY OPTIMIZATION THAT WILL PULL OLDER KEYS TO A NEWER PLACES WHEN IT IS FREE

        loop {
            match node.find_inner_idx(key, capacity) {
                Some((idx, _)) => {
                    return Some((node, idx, capacity));
                }
                None => {
                    let next = node.read_next();

                    if next == 0 {
                        return None;
                    }

                    node = unsafe { SCertifiedHashMapNode::from_ptr(next) };
                    capacity = node.read_capacity();
                }
            };
        }
    }

    fn get_or_create_root(&mut self) -> SCertifiedHashMapNode {
        if let Some(root) = &self.root {
            unsafe { root.copy() }
        } else {
            let root_bucket = SCertifiedHashMapNode::default();
            self.buckets.push(Bucket::new(root_bucket.table_ptr));

            self.root = Some(root_bucket);

            unsafe { self.root.as_ref().unwrap_unchecked().copy() }
        }
    }

    fn get_root_unchecked(&self) -> SCertifiedHashMapNode {
        unsafe { self.root.as_ref().map(|it| it.copy()).unwrap_unchecked() }
    }

    pub fn debug_print(&self) {
        let mut node = self.get_root_unchecked();

        loop {
            node.debug_print(node.read_capacity());

            let next = node.read_next();
            if next == 0 {
                break;
            }

            node = unsafe { SCertifiedHashMapNode::<K, V>::from_ptr(next) };
        }
    }
}

impl<K, V> Default for SCertifiedSet {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::certified_hash_map::map::SCertifiedSet;
    use crate::init_allocator;
    use crate::primitive::StableAllocated;
    use crate::utils::certification::{MerkleKV, Sha256Digest, EMPTY_SHA256};
    use crate::utils::mem_context::stable;
    use rand::seq::SliceRandom;
    use rand::thread_rng;
    use sha2::{Digest, Sha256};

    #[test]
    fn simple_flow_works_well() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SCertifiedSet::new();

        let k1 = 1u32;
        let k2 = 2u32;
        let k3 = 3u32;
        let k4 = 4u32;
        let k5 = 5u32;
        let k6 = 6u32;
        let k7 = 7u32;
        let k8 = 8u32;

        map.insert(k1, 1);
        map.insert(k2, 2);
        map.insert(k3, 3);
        map.insert(k4, 4);
        map.insert(k5, 5);
        map.insert(k6, 6);
        map.insert(k7, 7);
        map.insert(k8, 8);

        assert_eq!(map.get_copy(&k1).unwrap(), 1);
        assert_eq!(map.get_copy(&k2).unwrap(), 2);
        assert_eq!(map.get_copy(&k3).unwrap(), 3);
        assert_eq!(map.get_copy(&k4).unwrap(), 4);
        assert_eq!(map.get_copy(&k5).unwrap(), 5);
        assert_eq!(map.get_copy(&k6).unwrap(), 6);
        assert_eq!(map.get_copy(&k7).unwrap(), 7);
        assert_eq!(map.get_copy(&k8).unwrap(), 8);

        assert!(map.get_copy(&9u32).is_none());
        assert!(map.get_copy(&0u32).is_none());

        assert_eq!(map.remove(&k3).unwrap(), 3);
        assert!(map.get_copy(&k3).is_none());

        assert_eq!(map.remove(&k1).unwrap(), 1);
        assert!(map.get_copy(&k1).is_none());

        assert_eq!(map.remove(&k5).unwrap(), 5);
        assert!(map.get_copy(&k5).is_none());

        assert_eq!(map.remove(&k7).unwrap(), 7);
        assert!(map.get_copy(&k7).is_none());

        assert_eq!(map.get_copy(&k2).unwrap(), 2);
        assert_eq!(map.get_copy(&k4).unwrap(), 4);
        assert_eq!(map.get_copy(&k6).unwrap(), 6);
        assert_eq!(map.get_copy(&k8).unwrap(), 8);

        // unsafe { map.stable_drop() };
    }

    #[test]
    fn basic_flow_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SCertifiedSet::new();

        assert!(map.remove(&10u32).is_none());
        assert!(map.get_copy(&10u32).is_none());

        let it = map.insert(1u32, 1);
        assert!(it.is_none());
        assert!(map.insert(2u32, 2).is_none());
        assert!(map.insert(3u32, 3).is_none());
        assert_eq!(map.insert(1u32, 10).unwrap(), 1);

        assert!(map.remove(&5u32).is_none());
        assert_eq!(map.remove(&1u32).unwrap(), 10);

        assert!(map.contains_key(&2u32));
        assert!(!map.contains_key(&5u32));

        // unsafe { map.stable_drop() };

        let mut map = SCertifiedSet::default();
        for i in 0..100u32 {
            assert!(map.insert(i, i).is_none());
        }

        for i in 0..100u32 {
            assert_eq!(map.get_copy(&i).unwrap(), i);
        }

        for i in 0..100u32 {
            assert_eq!(map.remove(&(99 - i)).unwrap(), 99 - i);
        }

        // unsafe { map.stable_drop() };
    }

    #[test]
    fn removes_work() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SCertifiedSet::new();

        for i in 0..500u32 {
            map.insert((499 - i), i);
        }

        let mut vec = (200u32..300).collect::<Vec<_>>();
        vec.shuffle(&mut thread_rng());

        for i in vec {
            map.remove(&i);
        }

        for i in 500..5000u32 {
            map.insert(i, i);
        }

        for i in 200..300u32 {
            map.insert(i, i);
        }

        let mut vec = (0..5000u32).collect::<Vec<_>>();
        vec.shuffle(&mut thread_rng());

        for i in vec {
            map.remove(&i);
        }

        // unsafe { map.stable_drop() };
    }

    // TODO: RENAME
    #[test]
    fn tombstones_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SCertifiedSet::new();

        for i in 0..100u32 {
            map.insert(i, i);
        }

        assert_eq!(map.len(), 100);

        for i in 0..50u32 {
            map.remove(&i);
        }

        assert_eq!(map.len(), 50);

        for i in 0..50u32 {
            map.insert(i, i);
        }

        assert_eq!(map.len(), 100);

        // unsafe { map.stable_drop() };
    }

    /*    #[test]
    fn iter_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SCertifiedHashMap::new();
        for i in 0..100u32 {
            map.insert(i, i);
        }

        let mut c = 0;
        for (k, v) in map.iter() {
            c += 1;

            assert!(u32::from_le_bytes(k) < 100);
        }

        assert_eq!(c, 100);
    }*/

    #[test]
    fn certification_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SCertifiedSet::new();

        for i in 0..100u32 {
            map.insert(i, i);
            let root = map.get_root_hash();

            for j in 0..i + 1 {
                let witness = map.witness_key(&j);

                let (key, root_1) = witness.unwrap().reconstruct();

                match key {
                    MerkleKV::Plain((k, v)) => {
                        assert_eq!(k, j);
                        assert_eq!(v, j);
                    }
                    _ => unreachable!(),
                }

                assert_eq!(root_1, root);
            }
        }

        assert_eq!(map.get_copy(&0).unwrap(), 0);

        for i in 1..100u32 {
            map.remove(&(i - 1)).unwrap();

            let root = map.get_root_hash();

            for j in i..100u32 {
                let witness = map.witness_key(&j).unwrap();

                let (key, root_1) = witness.reconstruct();

                match key {
                    MerkleKV::Plain((k, v)) => {
                        assert_eq!(k, j);
                        assert_eq!(v, j);
                    }
                    _ => unreachable!(),
                }

                assert_eq!(root_1, root);
            }
        }
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
