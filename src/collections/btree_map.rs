use crate::collections::vec::SVec;
use crate::mem::s_slice::Side;
use crate::primitive::StackAllocated;
use crate::SSlice;
use speedy::{Readable, Writable};
use std::mem::size_of;

const DEFAULT_BTREE_DEGREE: usize = 4096;

#[derive(Readable, Writable)]
pub struct SBTreeMap<K, V, AK, AV> {
    root: BTreeNode<K, V, AK, AV>,
    degree: usize,
    len: u64,
}

impl<AK, AV, K, V> SBTreeMap<K, V, AK, AV> {
    pub fn new() -> Self {
        Self::new_with_degree(DEFAULT_BTREE_DEGREE)
    }

    pub fn new_with_degree(degree: usize) -> Self {
        assert!(degree > 1, "Unable to create BTree with degree less than 2");

        Self {
            degree,
            root: BTreeNode::<K, V, AK, AV>::new(true, true),
            len: 0,
        }
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl<
        AK: AsMut<[u8]>,
        AV: AsMut<[u8]>,
        K: Ord + StackAllocated<K, AK>,
        V: StackAllocated<V, AV>,
    > SBTreeMap<K, V, AK, AV>
{
    pub fn insert(&mut self, key: &K, value: &V) -> Option<V> {
        let root = &mut self.root;

        let res = if root.keys.len() == 2 * self.degree - 1 {
            let mut temp = BTreeNode::new(false, false);

            root.is_root = false;
            temp.children.insert(0, root);

            Self::split_child(self.degree, &mut temp, 0);
            let res = Self::insert_non_full(self.degree, &mut temp, key, value);

            self.root = temp;
            self.root.is_root = true;

            res
        } else {
            Self::insert_non_full(self.degree, &mut self.root, key, value)
        };

        if res.is_none() {
            self.len += 1;
        }

        res
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        let res = Self::_delete(self.degree, &mut self.root, key)?;
        self.len -= 1;

        Some(res)
    }

    pub unsafe fn drop(mut self) {
        while let Some(child_node) = self.root.children.pop() {
            Self::_drop(child_node);
        }
    }

    pub fn get_cloned(&self, key: &K) -> Option<V> {
        Self::_get(&self.root, key)
    }

    pub fn contains_key(&self, key: &K) -> bool {
        Self::_contains_key(&self.root, key)
    }

    fn insert_non_full(
        degree: usize,
        node: &mut BTreeNode<K, V, AK, AV>,
        key: &K,
        value: &V,
    ) -> Option<V> {
        match node.keys.binary_search_by(|k| k.cmp(key)) {
            Ok(idx) => Some(node.values.replace(idx, value)),
            Err(mut idx) => {
                if node.is_leaf {
                    node.keys.insert(idx, key);
                    node.values.insert(idx, value);

                    None
                } else {
                    if node.children.get_copy(idx).unwrap().keys.len() == 2 * degree - 1 {
                        Self::split_child(degree, node, idx);

                        if key.gt(&node.keys.get_copy(idx).unwrap()) {
                            idx += 1;
                        }
                    }

                    let mut child = node.children.get_copy(idx).unwrap();
                    let result = Self::insert_non_full(degree, &mut child, key, value);

                    node.children.replace(idx, &child);

                    result
                }
            }
        }
    }

    fn split_child(degree: usize, node: &mut BTreeNode<K, V, AK, AV>, idx: usize) {
        let mut child = node.children.get_copy(idx).unwrap();
        let mut new_child = BTreeNode::<K, V, AK, AV>::new(child.is_leaf, false);

        for _ in 0..(degree - 1) {
            new_child.keys.push(&child.keys.remove(degree));
            new_child.values.push(&child.values.remove(degree));
        }
        node.keys.insert(idx, &child.keys.remove(degree - 1));
        node.values.insert(idx, &child.values.remove(degree - 1));

        if !child.is_leaf {
            for i in 0..degree {
                let grand_child = child.children.remove(degree);

                if grand_child.keys.is_empty() {
                    let slice = SSlice::from_ptr(child.children.ptr, Side::Start).unwrap();
                    let mut bytes = vec![0u8; slice.get_size_bytes()];
                    slice.read_bytes(0, &mut bytes);

                    println!("{} {:?}", idx, bytes);

                    panic!();
                }

                new_child.children.push(&grand_child);
            }
        }

        node.children.replace(idx, &child);
        node.children.insert(idx + 1, &new_child);
    }

    fn _contains_key(node: &BTreeNode<K, V, AK, AV>, key: &K) -> bool {
        match node.keys.binary_search_by(|k| k.cmp(key)) {
            Ok(_) => true,
            Err(idx) => {
                if let Some(child) = node.children.get_copy(idx) {
                    Self::_contains_key(&child, key)
                } else {
                    false
                }
            }
        }
    }

    fn _get(node: &BTreeNode<K, V, AK, AV>, key: &K) -> Option<V> {
        match node.keys.binary_search_by(|k| k.cmp(key)) {
            Ok(idx) => node.values.get_copy(idx),
            Err(idx) => {
                let child = node.children.get_copy(idx)?;
                Self::_get(&child, key)
            }
        }
    }

    unsafe fn _drop(node: BTreeNode<K, V, AK, AV>) {
        for i in 0..node.children.len() {
            Self::_drop(node.children.get_copy(i).unwrap());
        }

        node.drop();
    }

    fn _delete(degree: usize, node: &mut BTreeNode<K, V, AK, AV>, key: &K) -> Option<V> {
        match node.keys.binary_search_by(|k| k.cmp(key)) {
            Ok(idx) => {
                if node.is_leaf {
                    let k = node.keys.remove(idx);
                    let v = node.values.remove(idx);

                    Some(v)
                } else {
                    Self::delete_internal_node(degree, node, key, idx)
                }
            }
            Err(idx) => {
                let mut merged = false;

                if node.is_leaf {
                    return None;
                }

                let mut child = node.children.get_copy(idx).unwrap();

                if child.keys.len() >= degree {
                    let res = Self::_delete(degree, &mut child, key);
                    node.children.replace(idx, &child);

                    res
                } else {
                    if idx != 0 && idx + 1 < node.children.len() {
                        let left_child_sibling = node.children.get_copy(idx - 1).unwrap();
                        let right_child_sibling = node.children.get_copy(idx + 1).unwrap();

                        if left_child_sibling.keys.len() >= degree {
                            Self::delete_sibling(node, idx, idx - 1);
                        } else if right_child_sibling.keys.len() >= degree {
                            Self::delete_sibling(node, idx, idx + 1);
                        } else {
                            Self::delete_merge(node, idx, idx + 1);
                            merged = true;
                        }
                    } else if idx == 0 {
                        let right_child_sibling = node.children.get_copy(idx + 1).unwrap();

                        if right_child_sibling.keys.len() >= degree {
                            Self::delete_sibling(node, idx, idx + 1);
                        } else {
                            Self::delete_merge(node, idx, idx + 1);
                            merged = true;
                        }
                    } else if idx + 1 == node.children.len() {
                        let left_child_sibling = node.children.get_copy(idx - 1).unwrap();

                        if left_child_sibling.keys.len() >= degree {
                            Self::delete_sibling(node, idx, idx - 1);
                        } else {
                            Self::delete_merge(node, idx, idx - 1);
                            merged = true;
                        }
                    }

                    if merged {
                        return Self::_delete(degree, node, key);
                    }

                    let mut child = node.children.get_copy(idx).unwrap();
                    let res = Self::_delete(degree, &mut child, key);
                    node.children.replace(idx, &child);

                    res
                }
            }
        }
    }

    fn delete_internal_node(
        degree: usize,
        node: &mut BTreeNode<K, V, AK, AV>,
        key: &K,
        idx: usize,
    ) -> Option<V> {
        let mut left_child = node.children.get_copy(idx).unwrap();
        let mut right_child = node.children.get_copy(idx + 1).unwrap();

        if left_child.keys.len() >= degree {
            let (k, v) = Self::delete_predecessor(degree, &mut left_child);
            let v = node.values.replace(idx, &v);

            node.keys.replace(idx, &k);
            node.children.replace(idx, &left_child);

            Some(v)
        } else if right_child.keys.len() >= degree {
            let (k, v) = Self::delete_successor(degree, &mut right_child);
            let v = node.values.replace(idx, &v);

            node.keys.replace(idx, &k);
            node.children.replace(idx + 1, &right_child);

            Some(v)
        } else {
            Self::delete_merge(node, idx, idx + 1);
            Self::_delete(degree, node, key)
        }
    }

    fn delete_predecessor(degree: usize, child: &mut BTreeNode<K, V, AK, AV>) -> (K, V) {
        if child.is_leaf {
            let k = child.keys.pop().unwrap();
            let v = child.values.pop().unwrap();

            return (k, v);
        }

        let n = child.keys.len() - 1;
        let grand_child = child.children.get_copy(n).unwrap();

        if grand_child.keys.len() >= degree {
            Self::delete_sibling(child, n + 1, n);
        } else {
            Self::delete_merge(child, n + 1, n);
        }

        let mut grand_child = child.children.get_copy(n).unwrap();
        let res = Self::delete_predecessor(degree, &mut grand_child);

        child.children.replace(n, &grand_child);

        res
    }

    fn delete_successor(degree: usize, child: &mut BTreeNode<K, V, AK, AV>) -> (K, V) {
        if child.is_leaf {
            let k = child.keys.remove(0);
            let v = child.values.remove(0);

            return (k, v);
        }

        let grand_child = child.children.get_copy(0).unwrap();

        if grand_child.keys.len() >= degree {
            Self::delete_sibling(child, 0, 1);
        } else {
            Self::delete_merge(child, 0, 1);
        }

        let mut grand_child = child.children.get_copy(0).unwrap();
        let res = Self::delete_successor(degree, &mut grand_child);

        child.children.replace(0, &grand_child);

        res
    }

    fn delete_merge(node: &mut BTreeNode<K, V, AK, AV>, i: usize, j: usize) {
        let mut child = node.children.get_copy(i).unwrap();

        let mut new = if j > i {
            let child_right_sibling = node.children.remove(j);
            child.keys.push(&node.keys.remove(i));
            child.values.push(&node.values.remove(i));

            child.keys.extend_from(&child_right_sibling.keys);
            child.values.extend_from(&child_right_sibling.values);
            child.children.extend_from(&child_right_sibling.children);

            node.children.replace(i, &child);

            unsafe { child_right_sibling.drop() };

            child
        } else {
            let mut child_left_sibling = node.children.get_copy(j).unwrap();
            child_left_sibling.keys.push(&node.keys.remove(j));
            child_left_sibling.values.push(&node.values.remove(j));

            child_left_sibling.keys.extend_from(&child.keys);
            child_left_sibling.values.extend_from(&child.values);
            child_left_sibling.children.extend_from(&child.children);

            node.children.replace(j, &child_left_sibling);

            let child = node.children.remove(i);
            unsafe { child.drop() };

            child_left_sibling
        };

        if node.is_root && node.keys.is_empty() {
            new.is_root = true;
            *node = new;
        }
    }

    fn delete_sibling(node: &mut BTreeNode<K, V, AK, AV>, i: usize, j: usize) {
        let mut child = node.children.get_copy(i).unwrap();

        if j > i {
            let mut child_right_sibling = node.children.get_copy(j).unwrap();

            child.keys.push(&node.keys.remove(i));
            child.values.push(&node.values.remove(i));

            node.keys.insert(i, &child_right_sibling.keys.remove(0));
            node.values.insert(i, &child_right_sibling.values.remove(0));

            if !child_right_sibling.children.is_empty() {
                child.children.push(&child_right_sibling.children.remove(0));
            }

            node.children.replace(j, &child_right_sibling);
        } else {
            let mut child_left_sibling = node.children.get_copy(j).unwrap();

            child.keys.insert(0, &node.keys.remove(i - 1));
            child.values.insert(0, &node.values.remove(i - 1));

            node.keys
                .insert(i - 1, &child_left_sibling.keys.pop().unwrap());
            node.values
                .insert(i - 1, &child_left_sibling.values.pop().unwrap());

            if !child_left_sibling.children.is_empty() {
                child
                    .children
                    .insert(0, &child_left_sibling.children.pop().unwrap())
            }

            node.children.replace(j, &child_left_sibling);
        }

        node.children.replace(i, &child);
    }
}

impl<AK, AV, K, V> Default for SBTreeMap<K, V, AK, AV> {
    fn default() -> Self {
        SBTreeMap::<K, V, AK, AV>::new()
    }
}

struct BTreeNode<K, V, AK, AV> {
    is_leaf: bool,
    is_root: bool,
    keys: SVec<K, AK>,
    values: SVec<V, AV>,
    children: SVec<BTreeNode<K, V, AK, AV>, [u8; 80]>,
}

impl<K, V, AK, AV> BTreeNode<K, V, AK, AV> {
    pub fn new(is_leaf: bool, is_root: bool) -> Self {
        Self {
            is_root,
            is_leaf,
            keys: SVec::new(),
            values: SVec::new(),
            children: SVec::new(),
        }
    }

    pub unsafe fn drop(self) {
        self.keys.drop();
        self.values.drop();
        self.children.drop();
    }
}

impl<K, V, AK, AV> StackAllocated<BTreeNode<K, V, AK, AV>, [u8; 80]> for BTreeNode<K, V, AK, AV> {
    fn size_of_u8_array() -> usize {
        80
    }

    fn fixed_size_u8_array() -> [u8; 80] {
        [0u8; 80]
    }

    #[inline]
    fn as_u8_slice(it: &Self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(it as *const Self as *const u8, size_of::<Self>()) }
    }

    #[inline]
    fn from_u8_fixed_size_array(arr: [u8; 80]) -> Self {
        unsafe { std::mem::transmute(arr) }
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::btree_map::{BTreeNode, SBTreeMap};
    use crate::primitive::StackAllocated;
    use crate::utils::isoprint;
    use crate::{init_allocator, stable};
    use std::fmt::Debug;
    use std::mem::size_of;

    #[ignore]
    fn btree_to_sorted_vec<
        AK: AsMut<[u8]>,
        AV: AsMut<[u8]>,
        K: Ord + StackAllocated<K, AK>,
        V: StackAllocated<V, AV>,
    >(
        btree_node: &BTreeNode<K, V, AK, AV>,
        vec: &mut Vec<(K, V)>,
    ) {
        for i in 0..btree_node.keys.len() {
            if let Some(child) = btree_node.children.get_copy(i) {
                btree_to_sorted_vec(&child, vec);
            }
            let k = btree_node.keys.get_copy(i).unwrap();
            let v = btree_node.values.get_copy(i).unwrap();

            vec.push((k, v));
        }

        if let Some(child) = btree_node.children.get_copy(btree_node.keys.len()) {
            btree_to_sorted_vec(&child, vec);
        }
    }

    #[ignore]
    fn print_btree<
        AK: AsMut<[u8]>,
        AV: AsMut<[u8]>,
        K: Ord + StackAllocated<K, AK> + Debug,
        V: StackAllocated<V, AV> + Debug,
    >(
        btree: &SBTreeMap<K, V, AK, AV>,
    ) {
        let mut nodes_1 = print_btree_level(&btree.root);
        isoprint("");

        loop {
            let mut nodes_2 = vec![];

            for node in &nodes_1 {
                let res = print_btree_level(node);

                for n in res {
                    nodes_2.push(n);
                }
            }

            isoprint("");

            if nodes_2.is_empty() {
                break;
            }

            nodes_1 = nodes_2;
        }
    }

    #[ignore]
    fn print_btree_level<
        AK: AsMut<[u8]>,
        AV: AsMut<[u8]>,
        K: Ord + StackAllocated<K, AK> + Debug,
        V: StackAllocated<V, AV> + Debug,
    >(
        btree_node: &BTreeNode<K, V, AK, AV>,
    ) -> Vec<BTreeNode<K, V, AK, AV>> {
        let mut children = vec![];

        let keys: Vec<_> = Vec::from(&btree_node.keys);
        let values: Vec<_> = Vec::from(&btree_node.values);

        print!(
            "( is_leaf: {}, is_root: {} - {:?} )",
            btree_node.is_leaf,
            btree_node.is_root,
            keys.iter().zip(values.iter()).collect::<Vec<_>>()
        );

        for ch in 0..btree_node.children.len() {
            let child = btree_node.children.get_copy(ch).unwrap();

            children.push(child);
        }

        children
    }

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

        let mut map = SBTreeMap::new_with_degree(3);

        println!("INSERTION");

        assert!(map.insert(&30, &3).is_none());
        print_btree(&map);
        println!();

        assert!(map.insert(&90, &9).is_none());
        print_btree(&map);
        println!();

        assert!(map.insert(&10, &1).is_none());
        print_btree(&map);
        println!();

        assert!(map.insert(&70, &7).is_none());
        print_btree(&map);
        println!();

        assert!(map.insert(&80, &8).is_none());
        print_btree(&map);
        println!();

        assert!(map.insert(&50, &5).is_none());
        print_btree(&map);
        println!();

        assert!(map.insert(&20, &2).is_none());
        print_btree(&map);
        println!();

        assert!(map.insert(&60, &6).is_none());
        print_btree(&map);
        println!();

        assert!(map.insert(&40, &4).is_none());
        print_btree(&map);
        println!();

        assert_eq!(map.len(), 9);

        let mut probe = vec![];
        btree_to_sorted_vec(&map.root, &mut probe);
        assert_eq!(example, probe);

        println!("DELETION");

        assert_eq!(map.remove(&30).unwrap(), 3);
        print_btree(&map);
        println!();

        assert_eq!(map.remove(&70).unwrap(), 7);
        print_btree(&map);
        println!();

        assert_eq!(map.remove(&50).unwrap(), 5);
        print_btree(&map);
        println!();

        assert_eq!(map.remove(&40).unwrap(), 4);
        print_btree(&map);
        println!();

        assert_eq!(map.remove(&60).unwrap(), 6);
        print_btree(&map);
        println!();

        assert_eq!(map.remove(&20).unwrap(), 2);
        print_btree(&map);
        println!();

        assert_eq!(map.remove(&80).unwrap(), 8);
        print_btree(&map);
        println!();

        assert_eq!(map.remove(&10).unwrap(), 1);
        print_btree(&map);
        println!();

        assert_eq!(map.remove(&90).unwrap(), 9);
        print_btree(&map);
        println!();

        assert!(map.is_empty());

        unsafe { map.drop() };
    }

    #[test]
    fn sequential_works_as_expected() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SBTreeMap::new_with_degree(2);

        println!("INSERTION");

        for i in 0..10 {
            map.insert(&i, &0);
            print_btree(&map);
            println!();
        }

        println!("DELETION");

        for i in 0..10 {
            map.remove(&i).unwrap();
            print_btree(&map);
            println!();
        }

        unsafe { map.drop() };
    }

    #[test]
    fn basic_flow_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SBTreeMap::new();

        let prev = map.insert(&1, &10);
        assert!(prev.is_none());

        let val = map.get_cloned(&1).unwrap();
        assert_eq!(val, 10);
        assert!(map.contains_key(&1));

        assert!(map.insert(&2, &20).is_none());
        map.insert(&3, &30);
        map.insert(&4, &40);
        map.insert(&5, &50);

        let val = map.insert(&3, &130).unwrap();
        assert_eq!(val, 30);

        assert!(!map.contains_key(&99));
        assert!(map.remove(&99).is_none());

        unsafe { map.drop() };

        let _map = SBTreeMap::<u64, u64, [u8; 8], [u8; 8]>::default();
    }

    #[test]
    fn deletion_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SBTreeMap::new_with_degree(5);

        for i in 0..50 {
            map.insert(&(i + 10), &i);
        }

        let val = map.insert(&13, &130).unwrap();
        assert_eq!(val, 3);

        let val1 = map.get_cloned(&13).unwrap();
        assert_eq!(val1, 130);

        assert!(!map.contains_key(&99));
        assert!(map.remove(&99).is_none());

        map.insert(&13, &3);
        assert_eq!(map.remove(&16).unwrap(), 6);

        map.insert(&16, &6);
        map.insert(&9, &90);

        assert_eq!(map.remove(&16).unwrap(), 6);

        map.insert(&16, &6);
        assert_eq!(map.remove(&9).unwrap(), 90);
        assert_eq!(map.remove(&53).unwrap(), 43);

        map.insert(&60, &70);
        map.insert(&61, &71);
        assert_eq!(map.remove(&58).unwrap(), 48);

        unsafe { map.drop() };

        let mut map = SBTreeMap::new_with_degree(5);

        for i in 0..50 {
            map.insert(&(i * 2), &i);
        }

        map.insert(&85, &1);
        assert_eq!(map.remove(&88).unwrap(), 44);

        unsafe { map.drop() };

        let mut map = SBTreeMap::new_with_degree(3);

        for i in 0..50 {
            map.insert(&(i * 2), &i);
        }

        map.remove(&94);
        map.remove(&96);
        map.remove(&98);

        assert_eq!(map.remove(&88).unwrap(), 44);

        map.insert(&81, &1);
        map.insert(&83, &1);
        map.insert(&94, &1);
        map.insert(&85, &1);

        assert_eq!(map.remove(&86).unwrap(), 43);

        map.insert(&71, &1);
        map.insert(&73, &1);
        map.insert(&75, &1);
        map.insert(&77, &1);
        map.insert(&79, &1);

        map.insert(&47, &1);
        map.insert(&49, &1);
        map.insert(&51, &1);
        map.insert(&53, &1);
        map.insert(&55, &1);
        map.insert(&57, &1);
        map.insert(&59, &1);
        map.insert(&61, &1);
        map.insert(&63, &1);
        map.insert(&65, &1);
        map.insert(&67, &1);
        map.insert(&69, &1);

        print_btree(&map);

        unsafe { map.drop() };

        let mut map = SBTreeMap::new_with_degree(3);

        for i in 150..300 {
            map.insert(&i, &i);
        }
        for i in 0..150 {
            map.insert(&i, &i);
        }

        assert_eq!(map.remove(&203).unwrap(), 203);
        assert_eq!(map.remove(&80).unwrap(), 80);

        print_btree(&map);

        unsafe { map.drop() };
    }

    #[test]
    fn complex_deletes_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SBTreeMap::new_with_degree(3);

        for i in 0..75 {
            map.insert(&i, &i);
        }

        for i in 0..75 {
            map.insert(&(150 - i), &i);
        }

        for i in 0..150 {
            let j = if i % 2 == 0 { i } else { 150 - i };

            if j % 3 == 0 {
                map.remove(&j);
            }
        }

        unsafe { map.drop() };

        let mut map = SBTreeMap::new_with_degree(3);

        for i in 0..150 {
            map.insert(&(150 - i), &i);
        }

        for i in 0..150 {
            map.remove(&(150 - i));
        }

        unsafe { map.drop() };
    }

    #[test]
    fn set_like_map_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SBTreeMap::<i32, (), [u8; size_of::<i32>()], [u8; 0]>::new();
        map.insert(&1, &());
        unsafe { map.drop() };
    }
}
