use crate::collections::hash_map::node::SHashTreeNode;
use crate::primitive::StableAllocated;
use std::hash::Hash;

// non-reallocating big hash map based on rope data structure
// linked list of hashmaps, from big ones to small ones
// infinite; both: logarithmic and amortized const
pub struct SHashTreeMap<K, V> {
    root: Option<SHashTreeNode<K, V>>,
    len: u64,
}

impl<K, V> SHashTreeMap<K, V> {
    #[inline]
    pub fn new() -> Self {
        Self { root: None, len: 0 }
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

impl<K: StableAllocated + Hash + Eq, V: StableAllocated> SHashTreeMap<K, V>
where
    [u8; K::SIZE]: Sized,
    [u8; V::SIZE]: Sized,
{
    pub fn insert(&mut self, mut k: K, mut v: V) -> Option<V> {
        let mut node = self.get_or_create_root();
        let mut capacity = node.read_capacity();
        let root_capacity = capacity;

        loop {
            match node.insert(k, v, capacity) {
                Ok((res, should_update_len, _)) => {
                    if should_update_len {
                        self.len += 1;
                    }

                    return res;
                }
                Err((_k, _v)) => {
                    k = _k;
                    v = _v;

                    let next = node.read_next();

                    node = if next == 0 {
                        let mut new_root_capacity = root_capacity * 2 - 1;
                        let mut new_root =
                            if let Some(new_root) = SHashTreeNode::new(new_root_capacity) {
                                new_root
                            } else {
                                new_root_capacity = root_capacity;
                                unsafe { SHashTreeNode::new(root_capacity).unwrap_unchecked() }
                            };

                        let root = self.get_root_unchecked();
                        new_root.write_next(root.table_ptr);

                        capacity = new_root_capacity;
                        self.root = Some(unsafe { new_root.copy() });

                        new_root
                    } else {
                        let next = unsafe { SHashTreeNode::from_ptr(next) };
                        capacity = next.read_capacity();

                        next
                    };
                }
            }
        }
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        if self.is_empty() {
            return None;
        }

        let mut node = self.get_or_create_root();
        let mut capacity = node.read_capacity();

        loop {
            match node.remove(key, capacity) {
                Some(v) => {
                    self.len -= 1;

                    return Some(v);
                }
                None => {
                    let next = node.read_next();

                    if next == 0 {
                        return None;
                    }

                    node = unsafe { SHashTreeNode::from_ptr(next) };
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

    fn find_key(&self, key: &K) -> Option<(SHashTreeNode<K, V>, usize, usize)> {
        if self.is_empty() {
            return None;
        }

        let mut non_empty_node_opt: Option<(SHashTreeNode<K, V>, usize)> = None;
        let mut node = self.get_root_unchecked();
        let mut capacity = node.read_capacity();

        loop {
            match node.find_inner_idx(key, capacity) {
                Some((idx, k)) => {
                    let res = if let Some((mut non_empty_node, its_capacity)) = non_empty_node_opt {
                        let val = node.remove_by_idx(idx, capacity);

                        // ignoring the result, because it cannot fail
                        match non_empty_node.insert(k, val, its_capacity) {
                            Ok((_, _, i)) => (non_empty_node, i, its_capacity),
                            _ => unreachable!(),
                        }
                    } else {
                        (node, idx, capacity)
                    };

                    return Some(res);
                }
                None => {
                    let next = node.read_next();

                    if non_empty_node_opt.is_none() {
                        let len = node.read_len();

                        if !node.is_full(len, capacity) {
                            non_empty_node_opt = Some((node, capacity));
                        }
                    }

                    if next == 0 {
                        return None;
                    }

                    node = unsafe { SHashTreeNode::from_ptr(next) };
                    capacity = node.read_capacity();
                }
            };
        }
    }

    fn get_or_create_root(&mut self) -> SHashTreeNode<K, V> {
        if let Some(root) = &self.root {
            unsafe { root.copy() }
        } else {
            self.root = Some(SHashTreeNode::default());

            unsafe { self.root.as_ref().unwrap_unchecked().copy() }
        }
    }

    fn get_root_unchecked(&self) -> SHashTreeNode<K, V> {
        unsafe { self.root.as_ref().map(|it| it.copy()).unwrap_unchecked() }
    }
}

impl<K, V> Default for SHashTreeMap<K, V> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::hash_map::map::SHashTreeMap;
    use crate::init_allocator;
    use crate::primitive::s_box::SBox;
    use crate::primitive::StableAllocated;
    use crate::utils::mem_context::stable;
    use copy_as_bytes::traits::AsBytes;
    use rand::seq::SliceRandom;
    use rand::thread_rng;
    use speedy::{Readable, Writable};

    #[test]
    fn insert_remove_read() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SHashTreeMap::new();
        let iterations = 2000;

        for i in 0..iterations {
            assert!(map.insert(i, i).is_none());

            for j in 0..i {
                assert_eq!(map.get_copy(&j).unwrap(), j);
            }
        }

        for i in 0..iterations {
            assert_eq!(map.remove(&i).unwrap(), i);

            if map.len() > 1 {
                for j in ((i + 1)..iterations).rev() {
                    let res = map.get_copy(&j);
                    assert_eq!(res.unwrap(), j);
                }
            }
        }
    }

    #[test]
    fn simple_flow_works_well() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SHashTreeMap::new();

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

        assert_eq!(map.get_copy(&k1).unwrap(), 1);
        assert_eq!(map.get_copy(&k2).unwrap(), 2);
        assert_eq!(map.get_copy(&k3).unwrap(), 3);
        assert_eq!(map.get_copy(&k4).unwrap(), 4);
        assert_eq!(map.get_copy(&k5).unwrap(), 5);
        assert_eq!(map.get_copy(&k6).unwrap(), 6);
        assert_eq!(map.get_copy(&k7).unwrap(), 7);
        assert_eq!(map.get_copy(&k8).unwrap(), 8);

        assert!(map.get_copy(&9).is_none());
        assert!(map.get_copy(&0).is_none());

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

        let mut map = SHashTreeMap::new();

        assert!(map.remove(&10).is_none());
        assert!(map.get_copy(&10).is_none());

        let it = map.insert(1, 1);
        assert!(it.is_none());
        assert!(map.insert(2, 2).is_none());
        assert!(map.insert(3, 3).is_none());
        assert_eq!(map.insert(1, 10).unwrap(), 1);

        assert!(map.remove(&5).is_none());
        assert_eq!(map.remove(&1).unwrap(), 10);

        assert!(map.contains_key(&2));
        assert!(!map.contains_key(&5));

        // unsafe { map.stable_drop() };

        let mut map = SHashTreeMap::default();
        for i in 0..100 {
            assert!(map.insert(i, i).is_none());
        }

        for i in 0..100 {
            assert_eq!(map.get_copy(&i).unwrap(), i);
        }

        for i in 0..100 {
            assert_eq!(map.remove(&(99 - i)).unwrap(), 99 - i);
        }

        // unsafe { map.stable_drop() };
    }

    #[test]
    fn removes_work() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SHashTreeMap::new();

        for i in 0..500 {
            map.insert(499 - i, i);
        }

        let mut vec = (200..300).collect::<Vec<_>>();
        vec.shuffle(&mut thread_rng());

        for (idx, i) in vec.into_iter().enumerate() {
            map.remove(&i).unwrap();
        }

        for i in 500..5000 {
            map.insert(i, i);
        }

        for i in 200..300 {
            map.insert(i, i);
        }

        let mut vec = (0..5000).collect::<Vec<_>>();
        vec.shuffle(&mut thread_rng());

        for (idx, i) in vec.into_iter().enumerate() {
            println!("{} {}", idx, i);
            let res = map.remove(&i);

            if res.is_none() {
                println!();
            }
        }

        // unsafe { map.stable_drop() };
    }

    #[test]
    fn tombstones_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SHashTreeMap::new();

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

        // unsafe { map.stable_drop() };
    }
}
