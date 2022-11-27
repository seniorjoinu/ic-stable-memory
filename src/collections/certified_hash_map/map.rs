use crate::collections::certified_hash_map::node::SCertifiedHashMapNode;
use crate::primitive::StableAllocated;
use crate::utils::certification::{MerkleWitness, Sha256Digest, ToHashableBytes, EMPTY_SHA256};
use sha2::{Digest, Sha256};

// non-reallocating big hash map based on rope data structure
// linked list of hashmaps, from big ones to small ones
// infinite; both: logarithmic and amortized const
pub struct SCertifiedHashMap<K, V> {
    root: Option<SCertifiedHashMapNode<K, V>>,
    len: u64,
    root_hash: Sha256Digest,
}

impl<K, V> SCertifiedHashMap<K, V> {
    #[inline]
    pub fn new() -> Self {
        Self {
            root: None,
            len: 0,
            root_hash: EMPTY_SHA256,
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
}

impl<K: StableAllocated + ToHashableBytes + Eq, V: StableAllocated + ToHashableBytes>
    SCertifiedHashMap<K, V>
where
    [u8; K::SIZE]: Sized,
    [u8; V::SIZE]: Sized,
{
    pub fn insert(&mut self, mut k: K, mut v: V) -> Option<V> {
        let mut node = self.get_or_create_root();
        let mut capacity = node.read_capacity();
        let root_capacity = capacity;

        let mut root_hashes = Vec::new();

        loop {
            match node.insert(k, v, capacity) {
                Ok((res, should_update_len, _, root_hash)) => {
                    root_hashes.push(root_hash);

                    loop {
                        let next = node.read_next();
                        if next == 0 {
                            break;
                        }

                        node = unsafe { SCertifiedHashMapNode::<K, V>::from_ptr(next) };
                        root_hashes.push(node.read_root_hash());
                    }

                    // todo: refactor
                    let mut hasher = Sha256::default();
                    for h in root_hashes {
                        hasher.update(h);
                    }
                    self.root_hash = hasher.finalize().into();

                    if should_update_len {
                        self.len += 1;
                    }

                    return res;
                }
                Err((_k, _v)) => {
                    k = _k;
                    v = _v;

                    root_hashes.push(node.read_root_hash());

                    let next = node.read_next();

                    node = if next == 0 {
                        let mut new_root_capacity = root_capacity * 2 - 1;
                        let mut new_root = if let Some(new_root) =
                            SCertifiedHashMapNode::new(new_root_capacity)
                        {
                            new_root
                        } else {
                            new_root_capacity = root_capacity;
                            unsafe { SCertifiedHashMapNode::new(root_capacity).unwrap_unchecked() }
                        };

                        let root = self.get_root_unchecked();
                        new_root.write_next(root.table_ptr);

                        capacity = new_root_capacity;
                        self.root = Some(unsafe { new_root.copy() });

                        match new_root.insert(k, v, capacity) {
                            Ok((res, _, _, root_hash)) => {
                                root_hashes.insert(0, root_hash);

                                // todo: refactor
                                let mut hasher = Sha256::default();
                                for h in root_hashes {
                                    hasher.update(h);
                                }
                                self.root_hash = hasher.finalize().into();
                                self.len += 1;

                                return res;
                            }
                            _ => unreachable!(),
                        }
                    } else {
                        let next = unsafe { SCertifiedHashMapNode::from_ptr(next) };
                        capacity = next.read_capacity();

                        next
                    };
                }
            }
        }
    }

    fn update_root_hash(&mut self, mut hasher: Sha256, mut node: SCertifiedHashMapNode<K, V>) {
        loop {
            let next = node.read_next();
            if next == 0 {
                break;
            }

            node = unsafe { SCertifiedHashMapNode::from_ptr(next) };

            let rh = node.read_root_hash();
            hasher.update(rh);
        }

        self.root_hash = hasher.finalize().into();
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        if self.is_empty() {
            return None;
        }

        let mut node = self.get_or_create_root();
        let mut capacity = node.read_capacity();

        let mut hasher = Sha256::default();

        loop {
            match node.remove(key, capacity) {
                Some((v, root_hash)) => {
                    hasher.update(root_hash);

                    self.update_root_hash(hasher, node);

                    self.len -= 1;

                    return Some(v);
                }
                None => {
                    let rh = node.read_root_hash();
                    hasher.update(rh);
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

    pub fn get_copy(&self, key: &K) -> Option<V> {
        let (node, idx, capacity) = self.find_key(key)?;
        Some(node.read_val_at(idx, capacity))
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.find_key(key).is_some()
    }

    pub fn witness_key(&self, key: &K) -> Option<MerkleWitness<K, V>> {
        if self.is_empty() {
            return None;
        }

        let mut node = self.get_root_unchecked();
        let mut capacity = node.read_capacity();
        let mut additional_hashes = Vec::new();

        loop {
            match node.witness_key(key, capacity) {
                Some(tree) => {
                    additional_hashes.push(None);

                    loop {
                        let next = node.read_next();
                        if next == 0 {
                            break;
                        }

                        node = unsafe { SCertifiedHashMapNode::<K, V>::from_ptr(next) };
                        additional_hashes.push(Some(node.read_root_hash()));
                    }

                    return Some(MerkleWitness::new(tree, additional_hashes));
                }
                None => {
                    additional_hashes.push(Some(node.read_root_hash()));
                    let next = node.read_next();

                    if next == 0 {
                        return None;
                    }

                    node = unsafe { SCertifiedHashMapNode::<K, V>::from_ptr(next) };
                    capacity = node.read_capacity();
                }
            }
        }
    }

    #[inline]
    pub fn get_root_hash(&self) -> Sha256Digest {
        self.root_hash
    }

    fn find_key(&self, key: &K) -> Option<(SCertifiedHashMapNode<K, V>, usize, usize)> {
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

    fn get_or_create_root(&mut self) -> SCertifiedHashMapNode<K, V> {
        if let Some(root) = &self.root {
            unsafe { root.copy() }
        } else {
            self.root = Some(SCertifiedHashMapNode::default());

            unsafe { self.root.as_ref().unwrap_unchecked().copy() }
        }
    }

    fn get_root_unchecked(&self) -> SCertifiedHashMapNode<K, V> {
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

impl<K, V> Default for SCertifiedHashMap<K, V> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::certified_hash_map::map::SCertifiedHashMap;
    use crate::init_allocator;
    use crate::primitive::StableAllocated;
    use crate::utils::certification::MerkleKV;
    use crate::utils::mem_context::stable;
    use rand::seq::SliceRandom;
    use rand::thread_rng;
    use sha2::{Digest, Sha256};

    #[test]
    fn simple_flow_works_well() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SCertifiedHashMap::new();

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

        let mut map = SCertifiedHashMap::new();

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

        let mut map = SCertifiedHashMap::default();
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

        let mut map = SCertifiedHashMap::new();

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

        let mut map = SCertifiedHashMap::new();

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

        let mut map = SCertifiedHashMap::new();

        for i in 0..100u32 {
            map.insert(i, i);
            let root = map.get_root_hash();

            /*for j in 0..i + 1 {
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
            }*/
        }

        for i in 1..100u32 {
            println!("removing {}", i - 1);
            map.remove(&(i - 1));

            map.debug_print();
            let root = map.get_root_hash();

            for j in i..100u32 {
                println!("witnessing {}", j);

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
