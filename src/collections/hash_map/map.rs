use crate::collections::hash_map::node::{values_offset, SHashTreeNode, CAPACITY, HALF_CAPACITY};
use crate::primitive::StableAllocated;
use std::hash::Hash;

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

// TODO: optimization tips
// 1. Non-leaves are always full
// 2. When removing the leaf, you can track parent and is_left, without reading them once again

impl<K: StableAllocated + Hash + Eq, V: StableAllocated> SHashTreeMap<K, V>
where
    [u8; K::SIZE]: Sized,
    [u8; V::SIZE]: Sized,
    [u8; values_offset::<K>() + V::SIZE * CAPACITY]: Sized,
{
    pub fn insert(&mut self, mut k: K, mut v: V) -> Option<V> {
        let mut node = self.get_or_create_root();
        let mut level = 0;

        loop {
            match node.insert(k, v, level) {
                Ok((res, should_update_len)) => {
                    if should_update_len {
                        self.len += 1;
                    }

                    return res;
                }
                Err((_k, _v, key_hash)) => {
                    k = _k;
                    v = _v;

                    let is_left = Self::should_go_left(key_hash);

                    let ptr = if is_left {
                        node.read_left()
                    } else {
                        node.read_right()
                    };

                    node = if ptr == 0 {
                        let mut child = SHashTreeNode::default();
                        child.write_parent(node.table_ptr);

                        if is_left {
                            node.write_left(child.table_ptr);
                        } else {
                            node.write_right(child.table_ptr);
                        }

                        child
                    } else {
                        unsafe { SHashTreeNode::from_ptr(ptr) }
                    };

                    level += 1;
                }
            }
        }
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        if self.is_empty() {
            return None;
        }

        let mut node = self.get_or_create_root();
        let mut level = 0;

        let (mut child, value) = loop {
            match node.find_inner_idx(key, level) {
                Ok((idx, mut key, key_hash)) => {
                    self.len -= 1;
                    let value = node.remove_internal_no_len_mod(&mut key, idx);

                    let is_left = Self::should_go_left(key_hash);
                    let child = Self::get_child(&node, is_left);

                    if let Some(c) = child {
                        break (c, value);
                    } else {
                        let len = node.read_len() - 1;
                        node.write_len(len);

                        if len == 0 && !self.is_root(&node) {
                            self.remove_node(node);
                        }

                        return Some(value);
                    }
                }
                Err(key_hash) => {
                    let is_left = Self::should_go_left(key_hash);

                    let ptr = if is_left {
                        node.read_left()
                    } else {
                        node.read_right()
                    };

                    if ptr == 0 {
                        return None;
                    }

                    level += 1;
                    node = unsafe { SHashTreeNode::<K, V>::from_ptr(ptr) };
                }
            }
        };

        let mut child_level = level + 1;

        loop {
            let key_hash = child.hash(key, child_level);
            let is_left = Self::should_go_left(key_hash);

            if let Some(c) = Self::get_child(&child, is_left) {
                child = c;
                child_level += 1;

                continue;
            }

            let (replace_k, replace_v) = child.take_any_leaf_non_empty_no_len_mod();
            let len = child.read_len() - 1;
            child.write_len(len);

            if len == 0 {
                self.remove_node(child);
            }

            node.replace_internal_not_full(replace_k, replace_v, level);

            return Some(value);
        }
    }

    pub fn get_copy(&self, key: &K) -> Option<V> {
        if self.is_empty() {
            return None;
        }

        let mut node = unsafe { self.root.as_ref()?.copy() };
        let mut level = 0;

        loop {
            match node.find_inner_idx(key, level) {
                Ok((idx, _, _)) => {
                    return Some(node.read_val_at(idx));
                }
                Err(key_hash) => {
                    let is_left = Self::should_go_left(key_hash);

                    let ptr = if is_left {
                        node.read_left()
                    } else {
                        node.read_right()
                    };

                    if ptr == 0 {
                        return None;
                    }

                    node = unsafe { SHashTreeNode::<K, V>::from_ptr(ptr) };
                    level += 1;
                }
            }
        }
    }

    pub fn contains_key(&self, key: &K) -> bool {
        if self.is_empty() {
            return false;
        }

        if let Some(mut node) = unsafe { self.root.as_ref().map(|it| it.copy()) } {
            let mut level = 0;

            loop {
                match node.find_inner_idx(key, level) {
                    Ok(_) => {
                        return true;
                    }
                    Err(key_hash) => {
                        let is_left = Self::should_go_left(key_hash);

                        let ptr = if is_left {
                            node.read_left()
                        } else {
                            node.read_right()
                        };

                        if ptr == 0 {
                            return false;
                        }

                        node = unsafe { SHashTreeNode::<K, V>::from_ptr(ptr) };
                        level += 1;
                    }
                }
            }
        } else {
            false
        }
    }

    fn get_child(node: &SHashTreeNode<K, V>, is_left: bool) -> Option<SHashTreeNode<K, V>> {
        if is_left {
            let left = node.read_left();

            if left == 0 {
                let right = node.read_right();

                if right == 0 {
                    None
                } else {
                    Some(unsafe { SHashTreeNode::<K, V>::from_ptr(right) })
                }
            } else {
                Some(unsafe { SHashTreeNode::from_ptr(left) })
            }
        } else {
            let right = node.read_right();

            if right == 0 {
                let left = node.read_left();

                if left == 0 {
                    None
                } else {
                    Some(unsafe { SHashTreeNode::from_ptr(left) })
                }
            } else {
                Some(unsafe { SHashTreeNode::from_ptr(right) })
            }
        }
    }

    fn remove_node(&self, mut node: SHashTreeNode<K, V>) {
        let mut parent = unsafe { SHashTreeNode::<K, V>::from_ptr(node.read_parent()) };
        let left = parent.read_left();

        if left == node.table_ptr {
            parent.write_left(0);
        } else {
            debug_assert!(parent.read_right() == node.table_ptr);

            parent.write_right(0);
        }

        unsafe { node.stable_drop_collection() };
    }

    #[inline]
    fn is_root(&self, node: &SHashTreeNode<K, V>) -> bool {
        self.root.as_ref().unwrap().table_ptr == node.table_ptr
    }

    #[inline]
    const fn should_go_left(key_hash: usize) -> bool {
        key_hash & 1 == 1
    }

    fn get_or_create_root(&mut self) -> SHashTreeNode<K, V> {
        if let Some(root) = &self.root {
            unsafe { root.copy() }
        } else {
            self.root = Some(SHashTreeNode::default());

            unsafe { self.root.as_ref().unwrap_unchecked().copy() }
        }
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
