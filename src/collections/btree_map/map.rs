use crate::collections::btree_map::node::{BTreeNode, MIN_LEN_AFTER_SPLIT};
use crate::primitive::StableAllocated;
use copy_as_bytes::traits::{AsBytes, SuperSized};
use std::fmt::Debug;

pub struct SBTreeMap<K, V> {
    root: Option<BTreeNode<K, V>>,
    len: u64,
}

impl<K: StableAllocated + Ord, V: StableAllocated> SBTreeMap<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
    [(); MIN_LEN_AFTER_SPLIT * K::SIZE]: Sized,
    [(); MIN_LEN_AFTER_SPLIT * V::SIZE]: Sized,
{
    pub fn insert(&mut self, mut k: K, mut v: V) -> Option<V> {
        let mut node = self.get_root_mut();

        let mut new_node = loop {
            match node.insert_down(k, v) {
                Ok(res) => match res {
                    Ok(it) => {
                        if it.is_none() {
                            self.len += 1;
                        }

                        return it;
                    }
                    Err((_k, _v, mut _new_node)) => {
                        (k, v) = self.maybe_update_root(&mut node, &mut _new_node, _k, _v)?;

                        node = unsafe { BTreeNode::<K, V>::from_ptr(node.get_parent()) };
                        break _new_node;
                    }
                },
                Err((_k, _v, _node)) => {
                    k = _k;
                    v = _v;
                    node = _node;
                }
            }
        };

        loop {
            match node.insert_up(k, v, new_node) {
                Ok(_) => {
                    self.len += 1;
                    return None;
                }
                Err((_k, _v, mut _new_node)) => {
                    (k, v) = self.maybe_update_root(&mut node, &mut _new_node, _k, _v)?;

                    new_node = _new_node;
                    node = unsafe { BTreeNode::<K, V>::from_ptr(node.get_parent()) };
                }
            }
        }
    }

    pub fn remove(&mut self, k: &K) -> Option<V> {
        // the algorithm works by always removing from the leaf - if k is not in leaf, then we simply
        // search for an appropiate leaf node and swap an element from that leaf with our target element
        // and then delete from leaf

        let mut parent: Option<BTreeNode<K, V>> = None;
        let mut parent_idx: Option<usize> = None;
        let mut parent_len: Option<usize> = None;

        let mut node = self.get_root_mut();

        loop {
            let len = node.len();
            let is_leaf = node.is_leaf();

            match node.find_idx(k, len) {
                Ok(idx) => {
                    if is_leaf {
                        // if it is possible to remove without violation (or it is root) - do it
                        if len > MIN_LEN_AFTER_SPLIT || parent.is_none() {
                            let mut k = node.get_key(idx);
                            let mut v = node.get_value(idx);

                            node.remove_key(idx, len);
                            node.remove_value(idx, len);

                            k.remove_from_stable();
                            v.remove_from_stable();

                            node.set_len(len - 1);

                            self.len -= 1;
                            return Some(v);
                        }

                        // if it is impossible to simply remove the element without violating the min-len constraint
                        // try to steal an element from a neighbor or merge with them

                        let p = unsafe { parent.unwrap_unchecked() };
                        let p_idx = unsafe { parent_idx.unwrap_unchecked() };
                        let p_len = unsafe { parent_len.unwrap_unchecked() };

                        let (mut k, mut v, mut p_opt) =
                            BTreeNode::delete_in_violating_leaf(node, p, p_idx, p_len, idx);

                        // if we merged and have stolen an element from the parent, we may wanna fix it
                        // if it violates
                        while let Some(parent_to_handle) = p_opt {
                            p_opt = BTreeNode::<K, V>::handle_violating_internal(parent_to_handle);
                        }

                        k.remove_from_stable();
                        v.remove_from_stable();

                        self.len -= 1;
                        return Some(v);
                    }

                    // if the element is not in a leaf
                    // go to left subtree's max child or to right subtree's min child
                    let mut child = unsafe {
                        BTreeNode::<K, V>::from_ptr(node.get_child_ptr(
                            if idx > MIN_LEN_AFTER_SPLIT {
                                idx
                            } else {
                                idx + 1
                            },
                        ))
                    };

                    let mut child_parent = unsafe { node.copy() };
                    let mut child_p_idx = idx;
                    let mut child_p_len = len;

                    let mut child_is_leaf = child.is_leaf();
                    let mut child_len = child.len();

                    loop {
                        if !child_is_leaf {
                            child_is_leaf = child.is_leaf();
                            child_len = child.len();
                            child_parent = child;
                            child_p_idx = child_len - 1;
                            child_p_len = child_len;

                            child = unsafe {
                                BTreeNode::<K, V>::from_ptr(child_parent.get_child_ptr(
                                    if idx > MIN_LEN_AFTER_SPLIT {
                                        child_len
                                    } else {
                                        0
                                    },
                                ))
                            };

                            continue;
                        }

                        break;
                    }

                    let mut k = node.get_key(idx);
                    let mut v = node.get_value(idx);

                    k.remove_from_stable();
                    v.remove_from_stable();

                    let child_idx = if idx > MIN_LEN_AFTER_SPLIT {
                        child_len - 1
                    } else {
                        0
                    };

                    let replace_k = child.get_key(child_idx);
                    let replace_v = child.get_value(child_idx);

                    node.set_key(idx, replace_k);
                    node.set_value(idx, replace_v);

                    // if we can simply remove from the leaf - then do it
                    if child_len > MIN_LEN_AFTER_SPLIT {
                        child.remove_key(child_idx, child_len);
                        child.remove_value(child_idx, child_len);
                        child.set_len(child_len - 1);

                        self.len -= 1;
                        return Some(v);
                    }

                    child.set_key(child_idx, k);
                    child.set_value(child_idx, v);

                    // FIXME: optimize unnecessary reads here
                    let (_, v, mut p_opt) = BTreeNode::delete_in_violating_leaf(
                        child,
                        child_parent,
                        child_p_idx,
                        child_p_len,
                        child_idx,
                    );

                    while let Some(parent_to_handle) = p_opt {
                        p_opt = BTreeNode::<K, V>::handle_violating_internal(parent_to_handle);
                    }

                    self.len -= 1;
                    return Some(v);
                }
                Err(idx) => {
                    if is_leaf {
                        return None;
                    }

                    // if not found go deeper
                    parent = unsafe { Some(node.copy()) };
                    parent_len = Some(len);
                    parent_idx = Some(idx);

                    node = unsafe { BTreeNode::<K, V>::from_ptr(node.get_child_ptr(idx)) };
                }
            }
        }
    }

    pub fn get_copy(&self, k: &K) -> Option<V> {
        let mut node = self.root.as_ref().map(|it| unsafe { it.copy() })?;
        let mut len = node.len();

        loop {
            match node.find_idx(k, len) {
                Ok(idx) => return Some(node.get_value(idx)),
                Err(idx) => {
                    if node.is_leaf() {
                        return None;
                    } else {
                        node = unsafe { BTreeNode::<K, V>::from_ptr(node.get_child_ptr(idx)) };
                        len = node.len();
                    }
                }
            };
        }
    }

    pub fn contains_key(&self, k: &K) -> bool {
        if let Some(mut node) = self.root.as_ref().map(|it| unsafe { it.copy() }) {
            let mut len = node.len();

            loop {
                match node.find_idx(k, len) {
                    Ok(_) => return true,
                    Err(idx) => {
                        if node.is_leaf() {
                            return false;
                        } else {
                            node = unsafe { BTreeNode::<K, V>::from_ptr(node.get_child_ptr(idx)) };
                            len = node.len();
                        }
                    }
                };
            }
        } else {
            false
        }
    }

    fn maybe_update_root(
        &mut self,
        node: &mut BTreeNode<K, V>,
        new_node: &mut BTreeNode<K, V>,
        k: K,
        v: V,
    ) -> Option<(K, V)> {
        if self.node_is_root(node) {
            let mut new_root = BTreeNode::<K, V>::new(false);
            new_root.set_key(0, k);
            new_root.set_value(0, v);
            new_root.set_len(1);
            new_root.set_child_ptr(0, node.as_ptr());
            new_root.set_child_ptr(1, new_node.as_ptr());

            new_node.set_parent(new_root.as_ptr());
            node.set_parent(new_root.as_ptr());

            self.root = Some(new_root);
            self.len += 1;

            None
        } else {
            Some((k, v))
        }
    }

    fn node_is_root(&self, node: &BTreeNode<K, V>) -> bool {
        if let Some(root) = self.root.as_ref() {
            node.as_ptr() == root.as_ptr()
        } else {
            false
        }
    }

    fn get_root_mut(&mut self) -> BTreeNode<K, V> {
        if let Some(r) = self.root.as_ref().map(|it| unsafe { it.copy() }) {
            r
        } else {
            self.root = Some(BTreeNode::<K, V>::new(true));
            self.get_root_mut()
        }
    }
}

impl<K: StableAllocated + Debug, V: StableAllocated + Debug> SBTreeMap<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    pub(crate) fn debug_print(&self) {
        let mut nodes = vec![self.root.as_ref().map(|it| unsafe { it.copy() }).unwrap()];

        loop {
            for node in &nodes {
                print!("{:?} ", node);
            }
            println!();

            let mut new_nodes = Vec::new();
            for node in &nodes {
                if node.is_leaf() {
                    return;
                }

                for i in 0..node.len() + 1 {
                    let new_node = unsafe { BTreeNode::<K, V>::from_ptr(node.get_child_ptr(i)) };
                    new_nodes.push(new_node);
                }
            }

            nodes = new_nodes;
        }
    }
}

impl<K, V> SBTreeMap<K, V> {
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

impl<K, V> Default for SBTreeMap<K, V> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::btree_map::map::SBTreeMap;
    use crate::primitive::StableAllocated;
    use crate::{init_allocator, stable};
    use copy_as_bytes::traits::AsBytes;
    use rand::seq::SliceRandom;
    use rand::thread_rng;
    use speedy::{Readable, Writable};

    #[test]
    fn random_works_as_expected() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let example = vec![
            (10, 1),
            (20, 2),
            (30, 3),
            (40, 4),
            (50, 5),
            (60, 6),
            (70, 7),
            (80, 8),
            (90, 9),
        ];

        let mut map = SBTreeMap::new();

        println!("INSERTION");

        assert!(map.insert(30, 3).is_none());
        assert!(map.insert(90, 9).is_none());
        assert!(map.insert(10, 1).is_none());
        assert!(map.insert(70, 7).is_none());
        assert!(map.insert(80, 8).is_none());
        assert!(map.insert(50, 5).is_none());
        assert!(map.insert(20, 2).is_none());
        assert!(map.insert(60, 6).is_none());
        assert!(map.insert(40, 4).is_none());

        assert_eq!(map.len(), 9);

        println!("DELETION");

        assert_eq!(map.remove(&30).unwrap(), 3);
        assert_eq!(map.remove(&70).unwrap(), 7);
        assert_eq!(map.remove(&50).unwrap(), 5);
        assert_eq!(map.remove(&40).unwrap(), 4);
        assert_eq!(map.remove(&60).unwrap(), 6);
        assert_eq!(map.remove(&20).unwrap(), 2);
        assert_eq!(map.remove(&80).unwrap(), 8);
        assert_eq!(map.remove(&10).unwrap(), 1);
        assert_eq!(map.remove(&90).unwrap(), 9);

        let len = map.len;
        assert!(map.is_empty());

        // unsafe { map.stable_drop() };
    }

    #[test]
    fn sequential_works_as_expected() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SBTreeMap::new();

        println!("INSERTION");

        for i in 0..10 {
            map.insert(i, 0);
        }

        println!("DELETION");

        for i in 0..10 {
            map.remove(&i).unwrap();
        }

        // unsafe { map.stable_drop() };
    }

    #[test]
    fn basic_flow_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SBTreeMap::new();

        let prev = map.insert(1, 10);
        assert!(prev.is_none());

        let val = map.get_copy(&1).unwrap();
        assert_eq!(val, 10);
        assert!(map.contains_key(&1));

        assert!(map.insert(2, 20).is_none());
        map.insert(3, 30);
        map.insert(4, 40);
        map.insert(5, 50);

        let val = map.insert(3, 130).unwrap();
        assert_eq!(val, 30);

        assert!(!map.contains_key(&99));
        assert!(map.remove(&99).is_none());

        // unsafe { map.stable_drop() };

        let _map = SBTreeMap::<u64, u64>::default();
    }

    #[test]
    fn temp() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SBTreeMap::new();

        for i in 0..11 {
            map.insert(i, i);
        }

        map.debug_print();
        println!();

        map.insert(11, 11);

        map.debug_print();
    }

    #[test]
    fn deletion_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SBTreeMap::new();

        for i in 0..50 {
            map.insert(i + 10, i);
        }

        let val = map.insert(13, 130).unwrap();
        assert_eq!(val, 3);

        let val1 = map.get_copy(&13).unwrap();
        assert_eq!(val1, 130);

        assert!(!map.contains_key(&99));
        assert!(map.remove(&99).is_none());

        map.insert(13, 3);
        assert_eq!(map.remove(&16).unwrap(), 6);

        map.insert(16, 6);
        map.insert(9, 90);

        assert_eq!(map.remove(&16).unwrap(), 6);

        map.insert(16, 6);
        assert_eq!(map.remove(&9).unwrap(), 90);
        assert_eq!(map.remove(&53).unwrap(), 43);

        map.insert(60, 70);
        map.insert(61, 71);
        assert_eq!(map.remove(&58).unwrap(), 48);

        // unsafe { map.stable_drop() };

        let mut map = SBTreeMap::new();

        for i in 0..50 {
            map.insert(i * 2, i);
        }

        map.insert(85, 1);
        assert_eq!(map.remove(&88).unwrap(), 44);

        // unsafe { map.stable_drop() };

        let mut map = SBTreeMap::new();

        for i in 0..50 {
            map.insert(i * 2, i);
        }

        map.remove(&94);
        map.remove(&96);
        map.remove(&98);

        assert_eq!(map.remove(&88).unwrap(), 44);

        map.insert(81, 1);
        map.insert(83, 1);
        map.insert(94, 1);
        map.insert(85, 1);

        assert_eq!(map.remove(&86).unwrap(), 43);

        map.insert(71, 1);
        map.insert(73, 1);
        map.insert(75, 1);
        map.insert(77, 1);
        map.insert(79, 1);

        map.insert(47, 1);
        map.insert(49, 1);
        map.insert(51, 1);
        map.insert(53, 1);
        map.insert(55, 1);
        map.insert(57, 1);
        map.insert(59, 1);
        map.insert(61, 1);
        map.insert(63, 1);
        map.insert(65, 1);
        map.insert(67, 1);
        map.insert(69, 1);

        // unsafe { map.stable_drop() };

        let mut map = SBTreeMap::new();

        for i in 150..300 {
            map.insert(i, i);
        }

        for i in 0..150 {
            map.insert(i, i);
        }

        map.debug_print();

        assert_eq!(map.remove(&203).unwrap(), 203);
        assert_eq!(map.remove(&80).unwrap(), 80);

        // unsafe { map.stable_drop() };
    }

    #[test]
    fn complex_deletes_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SBTreeMap::new();

        for i in 0..75 {
            map.insert(i, i);
        }

        for i in 0..75 {
            map.insert(150 - i, i);
        }

        for i in 0..150 {
            let j = if i % 2 == 0 { i } else { 150 - i };

            if j % 3 == 0 {
                println!();
                println!("{}", j);
                map.debug_print();

                map.remove(&j);
            }
        }

        // unsafe { map.stable_drop() };

        let mut map = SBTreeMap::new();

        for i in 0..150 {
            map.insert(150 - i, i);
        }

        for i in 0..150 {
            map.remove(&(150 - i));
        }

        // unsafe { map.stable_drop() };

        let mut map = SBTreeMap::new();

        for i in 0..5000 {
            map.insert(4999 - i, i);
        }

        let mut vec = (2000..3000).collect::<Vec<_>>();
        vec.shuffle(&mut thread_rng());

        for i in vec {
            map.remove(&i);
        }

        for i in 2000..3000 {
            map.insert(i, i);
        }

        let mut vec = (0..5000).collect::<Vec<_>>();
        vec.shuffle(&mut thread_rng());

        for i in vec {
            map.remove(&i);
        }

        // unsafe { map.stable_drop() };
    }

    #[test]
    fn set_like_map_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SBTreeMap::<i32, ()>::new();
        map.insert(1, ());
        // unsafe { map.stable_drop() };
    }

    /*    #[test]
    fn iter_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SBTreeMap::new();
        for i in 0..100 {
            map.insert(i, i);
        }

        let mut c = 0;
        for (idx, (k, v)) in map.iter().enumerate() {
            assert!(k == idx && v == idx);
            c += 1;
        }

        assert_eq!(c, 100);
    }*/

    /*    #[test]
    fn serialization_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let map = SBTreeMap::<u32, u32>::new();
        let buf = map.write_to_vec().unwrap();
        let map1 = SBTreeMap::<u32, u32>::read_from_buffer_copying_data(&buf).unwrap();

        assert_eq!(map.len, map1.len);
        assert_eq!(map.root.is_root, map1.root.is_root);
        assert_eq!(map.root.is_leaf, map1.root.is_leaf);

        assert_eq!(map.root.keys.ptr, map1.root.keys.ptr);
        assert_eq!(map.root.keys.len, map1.root.keys.len);
        assert_eq!(map.root.keys.cap, map1.root.keys.cap);

        assert_eq!(map.root.values.ptr, map1.root.values.ptr);
        assert_eq!(map.root.values.len, map1.root.values.len);
        assert_eq!(map.root.values.cap, map1.root.values.cap);

        assert_eq!(map.root.children.ptr, map1.root.children.ptr);
        assert_eq!(map.root.children.len, map1.root.children.len);
        assert_eq!(map.root.children.cap, map1.root.children.cap);

        let len = map.len;
        let is_root = map.root.is_root;
        let is_leaf = map.root.is_leaf;

        let keys_ptr = map.root.keys.ptr;
        let keys_len = map.root.keys.len;
        let keys_cap = map.root.keys.cap;

        let values_ptr = map.root.values.ptr;
        let values_len = map.root.values.len;
        let values_cap = map.root.values.cap;

        let children_ptr = map.root.children.ptr;
        let children_len = map.root.children.len;
        let children_cap = map.root.children.cap;

        let buf = map.to_bytes();
        let map1 = SBTreeMap::<u32, u32>::from_bytes(buf);

        assert_eq!(len, map1.len);
        assert_eq!(is_root, map1.root.is_root);
        assert_eq!(is_leaf, map1.root.is_leaf);

        assert_eq!(keys_ptr, map1.root.keys.ptr);
        assert_eq!(keys_len, map1.root.keys.len);
        assert_eq!(keys_cap, map1.root.keys.cap);

        assert_eq!(values_ptr, map1.root.values.ptr);
        assert_eq!(values_len, map1.root.values.len);
        assert_eq!(values_cap, map1.root.values.cap);

        assert_eq!(children_ptr, map1.root.children.ptr);
        assert_eq!(children_len, map1.root.children.len);
        assert_eq!(children_cap, map1.root.children.cap);
    }*/
}