use crate::collections::btree_map::node::{BTreeNode, B, MIN_LEN_AFTER_SPLIT};
use crate::mem::s_slice::Side;
use crate::primitive::StableAllocated;
use crate::{deallocate, SSlice};
use std::fmt::Debug;

mod node;

// FIXME: REMOVE PARENTS, MAKE IT INTO A B+ TREE

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

                        let (mut k, mut v, nodes_opt) =
                            BTreeNode::delete_in_violating_leaf(node, idx, p, p_idx, p_idx, p_len);

                        self.handle_violating_internal(nodes_opt);

                        k.remove_from_stable();
                        v.remove_from_stable();

                        self.len -= 1;
                        return Some(v);
                    }

                    return Some(self.delete_non_leaf(node, len, idx));
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

    fn delete_non_leaf(&mut self, mut node: BTreeNode<K, V>, len: usize, idx: usize) -> V {
        // go to left subtree's max child or to right subtree's min child
        let to_left = idx >= len / 2;

        let mut parent = unsafe { node.copy() };
        let mut parent_len = len;
        let mut node_idx_in_parent = if to_left { idx } else { idx + 1 };
        let mut parent_value_idx_to_rotate = if to_left { idx } else { idx + 1 };

        let mut child =
            unsafe { BTreeNode::<K, V>::from_ptr(node.get_child_ptr(node_idx_in_parent)) };
        let mut child_len = child.len();
        let mut child_is_leaf = child.is_leaf();
        let mut idx_to_delete = if to_left { child_len - 1 } else { 0 };

        loop {
            if !child_is_leaf {
                parent = unsafe { child.copy() };
                parent_len = child_len;
                node_idx_in_parent = if to_left { child_len } else { 0 };
                parent_value_idx_to_rotate = idx_to_delete;

                child =
                    unsafe { BTreeNode::<K, V>::from_ptr(child.get_child_ptr(node_idx_in_parent)) };
                child_len = child.len();
                child_is_leaf = child.is_leaf();
                idx_to_delete = if to_left { child_len - 1 } else { 0 };
            } else {
                break;
            }
        }

        let mut k = node.get_key(idx);
        let mut v = node.get_value(idx);

        k.remove_from_stable();
        v.remove_from_stable();

        // FIXME: there should be a way to simply copy bits, without deserialization
        let replace_k = child.get_key(idx_to_delete);
        let replace_v = child.get_value(idx_to_delete);
        node.set_key(idx, replace_k);
        node.set_value(idx, replace_v);

        // if we can simply remove from the leaf - then do it
        if child_len > MIN_LEN_AFTER_SPLIT {
            child.remove_key(idx_to_delete, child_len);
            child.remove_value(idx_to_delete, child_len);
            child.set_len(child_len - 1);

            self.len -= 1;
            return v;
        }

        // FIXME: this should be unnecessary
        child.set_key(idx_to_delete, k);
        child.set_value(idx_to_delete, v);

        // FIXME: optimize unnecessary reads here
        let (_, v, nodes_opt) = BTreeNode::delete_in_violating_leaf(
            child,
            idx_to_delete,
            parent,
            parent_value_idx_to_rotate,
            node_idx_in_parent,
            parent_len,
        );

        self.handle_violating_internal(nodes_opt);

        self.len -= 1;

        v
    }

    fn handle_violating_internal(
        &mut self,
        mut nodes_opt: Option<(BTreeNode<K, V>, BTreeNode<K, V>)>,
    ) {
        while let Some((node, mut child)) = nodes_opt {
            nodes_opt = match BTreeNode::<K, V>::handle_violating_internal(unsafe { node.copy() }) {
                Ok(it) => it,
                Err(_) => {
                    let slice =
                        unsafe { SSlice::from_ptr(node.as_ptr(), Side::Start).unwrap_unchecked() };

                    deallocate(slice);

                    child.set_parent(0);
                    self.root = Some(child);

                    None
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
    use crate::collections::btree_map::SBTreeMap;
    use crate::primitive::StableAllocated;
    use crate::{init_allocator, stable};
    use rand::seq::SliceRandom;
    use rand::thread_rng;

    #[test]
    fn random_works_as_expected() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut example = Vec::new();
        for i in 0..100 {
            example.push(i);
        }

        example.shuffle(&mut thread_rng());

        let mut map = SBTreeMap::new();

        println!("INSERTION");

        for i in &example {
            println!("{}", i);
            assert!(map.insert(*i, *i).is_none());

            map.debug_print();
            println!();
        }

        println!("DELETION");

        for i in &example {
            println!("{}", i);
            assert!(map.remove(i).is_some());

            map.debug_print();
            println!();
        }

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

        for i in 0..100 {
            map.insert(i, 0);

            println!("{}", i);
            map.debug_print();
            println!();
        }

        println!("DELETION");

        for i in 0..100 {
            map.remove(&i).unwrap();

            println!("{}", i);
            map.debug_print();
            println!();
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
