use crate::{OutOfMemory, SUnsafeCell};
use candid::{decode_one, encode_one, CandidType, Deserialize};
use serde::de::DeserializeOwned;
use std::cmp::Ordering;
use std::fmt::Debug;

const DEFAULT_BTREE_DEGREE: usize = 4096;

/// FIXME: OOMs work really bad - I can't put my finger on that recursion
#[derive(CandidType, Deserialize)]
pub struct SBTreeMap<K, V> {
    root: BTreeNode<K, V>,
    degree: usize,
    len: u64,
}

impl<K: Ord + CandidType + DeserializeOwned, V: CandidType + DeserializeOwned> SBTreeMap<K, V> {
    pub fn new() -> Self {
        Self::new_with_degree(DEFAULT_BTREE_DEGREE)
    }

    pub fn new_with_degree(degree: usize) -> Self {
        assert!(degree > 1, "Unable to create BTree with degree less than 2");

        Self {
            degree,
            root: BTreeNode::<K, V>::new(true, true),
            len: 0,
        }
    }

    pub fn insert(&mut self, key: K, value: &V) -> Result<Option<V>, OutOfMemory> {
        let root = &mut self.root;
        let btree_key = BTreeKey::new(key, value)?;

        let res = if root.keys.len() == 2 * self.degree - 1 {
            let mut temp = BTreeNode::new(false, false);

            root.is_root = false;
            temp.children.insert(0, SUnsafeCell::new(root)?);

            if matches!(Self::split_child(self.degree, &mut temp, 0), Err(_)) {
                btree_key.drop();
                return Err(OutOfMemory);
            }
            let res = Self::insert_non_full(self.degree, &mut temp, btree_key)?;

            self.root = temp;
            self.root.is_root = true;

            res
        } else {
            Self::insert_non_full(self.degree, &mut self.root, btree_key)?
        };

        self.len += 1;

        Ok(res)
    }

    pub fn delete(&mut self, key: &K) -> Option<V> {
        let res = Self::_delete(self.degree, &mut self.root, key)?;
        self.len -= 1;

        Some(res)
    }

    pub fn get(&self, key: &K) -> Option<V> {
        self._get(&self.root, key)
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn insert_non_full(
        degree: usize,
        node: &mut BTreeNode<K, V>,
        key: BTreeKey<K, V>,
    ) -> Result<Option<V>, OutOfMemory> {
        match node.keys.binary_search(&key) {
            Ok(idx) => {
                let old_key = std::mem::replace(&mut node.keys[idx], key);
                Ok(Some(old_key.drop()))
            }
            Err(mut idx) => {
                if node.is_leaf {
                    node.keys.insert(idx, key);
                    Ok(None)
                } else {
                    if node.children[idx].get_cloned().keys.len() == 2 * degree - 1 {
                        if matches!(Self::split_child(degree, node, idx), Err(_)) {
                            key.drop();
                            return Err(OutOfMemory);
                        }

                        if key > node.keys[idx] {
                            idx += 1;
                        }
                    }

                    let mut child = node.children[idx].get_cloned();
                    let result = Self::insert_non_full(degree, &mut child, key);

                    unsafe {
                        // unwrapping, because I don't see any way of returning OOM here - recursion
                        // I'm not sure, but it also looks like an error place
                        let should_update = node.children[idx].set(&child).unwrap();
                    }

                    result
                }
            }
        }
    }

    fn split_child(
        degree: usize,
        node: &mut BTreeNode<K, V>,
        idx: usize,
    ) -> Result<(), OutOfMemory> {
        let mut child = node.children[idx].get_cloned();
        let mut new_child = BTreeNode::<K, V>::new(child.is_leaf, false);

        for _ in 0..(degree - 1) {
            new_child.keys.push(child.keys.remove(degree));
        }
        node.keys.insert(idx, child.keys.remove(degree - 1));

        if !child.is_leaf {
            for _ in 0..degree {
                new_child.children.push(child.children.remove(degree))
            }
        }

        unsafe {
            node.children[idx].set(&child).unwrap();
        }
        node.children
            .insert(idx + 1, SUnsafeCell::new(&new_child).unwrap());

        Ok(())
    }

    fn _get(&self, node: &BTreeNode<K, V>, key: &K) -> Option<V> {
        match node.keys.binary_search_by(|k| k.key.cmp(key)) {
            Ok(idx) => Some(node.keys[idx].value_cell.get_cloned()),
            Err(idx) => {
                let child = node.children.get(idx)?.get_cloned();
                self._get(&child, key)
            }
        }
    }

    fn _delete(degree: usize, node: &mut BTreeNode<K, V>, key: &K) -> Option<V> {
        match node.keys.binary_search_by(|k| k.key.cmp(key)) {
            Ok(idx) => {
                if node.is_leaf {
                    let btree_key = node.keys.remove(idx);
                    Some(btree_key.drop())
                } else {
                    Self::delete_internal_node(degree, node, key, idx)
                }
            }
            Err(idx) => {
                let mut child = node.children[idx].get_cloned();

                if child.keys.len() >= degree {
                    let res = Self::_delete(degree, &mut child, key);
                    unsafe {
                        node.children[idx].set(&child).unwrap();
                    }

                    res
                } else {
                    let left_child_sibling = node.children[idx - 1].get_cloned();
                    let right_child_sibling = node.children[idx + 1].get_cloned();

                    if idx != 0 && idx + 1 < node.children.len() {
                        if left_child_sibling.keys.len() >= degree {
                            Self::delete_sibling(node, idx, idx - 1);
                        } else if right_child_sibling.keys.len() >= degree {
                            Self::delete_sibling(node, idx, idx + 1);
                        } else {
                            Self::delete_merge(node, idx, idx + 1);
                        }
                    } else if idx == 0 {
                        if right_child_sibling.keys.len() >= degree {
                            Self::delete_sibling(node, idx, idx + 1);
                        } else {
                            Self::delete_merge(node, idx, idx + 1);
                        }
                    } else if idx + 1 == node.children.len() {
                        if left_child_sibling.keys.len() >= degree {
                            Self::delete_sibling(node, idx, idx - 1);
                        } else {
                            Self::delete_merge(node, idx, idx - 1);
                        }
                    }

                    let mut child = node.children[idx].get_cloned();
                    let res = Self::_delete(degree, &mut child, key);
                    unsafe {
                        node.children[idx].set(&child).unwrap();
                    }

                    res
                }
            }
        }
    }

    fn delete_internal_node(
        degree: usize,
        node: &mut BTreeNode<K, V>,
        key: &K,
        idx: usize,
    ) -> Option<V> {
        if node.is_leaf && node.keys[idx].key.eq(key) {
            let btree_key = node.keys.remove(idx);
            return Some(btree_key.drop());
        }

        let mut left_child = node.children[idx].get_cloned();
        let mut right_child = node.children[idx + 1].get_cloned();

        if left_child.keys.len() >= degree {
            let btree_key = std::mem::replace(
                &mut node.keys[idx],
                Self::delete_predecessor(degree, &mut left_child),
            );
            unsafe { node.children[idx].set(&left_child).unwrap() };

            Some(btree_key.drop())
        } else if right_child.keys.len() >= degree {
            let btree_key = std::mem::replace(
                &mut node.keys[idx],
                Self::delete_successor(degree, &mut right_child),
            );
            unsafe { node.children[idx + 1].set(&right_child).unwrap() };

            Some(btree_key.drop())
        } else {
            Self::delete_merge(node, idx, idx + 1);

            if node.is_root {
                Self::_delete(degree, node, key)
            } else {
                let mut left_child = node.children[idx].get_cloned();
                let res = Self::delete_internal_node(degree, &mut left_child, key, degree - 1);

                unsafe {
                    node.children[idx].set(&left_child).unwrap();
                }

                res
            }
        }
    }

    fn delete_predecessor(degree: usize, child: &mut BTreeNode<K, V>) -> BTreeKey<K, V> {
        if child.is_leaf {
            return child.keys.pop().unwrap();
        }

        let n = child.keys.len() - 1;
        let grand_child = child.children[n].get_cloned();

        if grand_child.keys.len() >= degree {
            Self::delete_sibling(child, n + 1, n);
        } else {
            Self::delete_merge(child, n + 1, n);
        }

        let mut grand_child = child.children[n].get_cloned();
        let res = Self::delete_predecessor(degree, &mut grand_child);

        unsafe {
            child.children[n].set(&grand_child).unwrap();
        }
        res
    }

    fn delete_successor(degree: usize, child: &mut BTreeNode<K, V>) -> BTreeKey<K, V> {
        if child.is_leaf {
            return child.keys.remove(0);
        }

        let grand_child = child.children[1].get_cloned();

        if grand_child.keys.len() >= degree {
            Self::delete_sibling(child, 0, 1);
        } else {
            Self::delete_merge(child, 0, 1);
        }

        let mut grand_child = child.children[1].get_cloned();
        let res = Self::delete_successor(degree, &mut grand_child);

        unsafe {
            child.children[0].set(&grand_child).unwrap();
        }
        res
    }

    fn delete_merge(node: &mut BTreeNode<K, V>, i: usize, j: usize) {
        let mut child = node.children[i].get_cloned();

        let mut new = if j > i {
            let child_right_sibling = node.children[j].get_cloned();
            child.keys.push(node.keys.remove(i));

            child.keys.extend(child_right_sibling.keys);
            child.children.extend(child_right_sibling.children);

            unsafe {
                node.children[i].set(&child).unwrap();
            }

            let child_right_sibling_ptr = node.children.remove(j);
            child_right_sibling_ptr.drop();

            child
        } else {
            let mut child_left_sibling = node.children[j].get_cloned();
            child_left_sibling.keys.push(node.keys.remove(j));

            child_left_sibling.keys.extend(child.keys);
            child_left_sibling.children.extend(child.children);

            unsafe {
                node.children[j].set(&child_left_sibling).unwrap();
            }

            let child_ptr = node.children.remove(i);
            child_ptr.drop();

            child_left_sibling
        };

        if node.is_root && node.keys.is_empty() {
            // dealing with memory leaks - remove the element from stable memory, if it becomes root
            if j > i {
                // FIXME: also dirty, but rust does not let me go straight to children[i]
                unsafe {
                    SUnsafeCell::<BTreeNode<K, V>>::from_ptr(node.children[i].as_ptr()).drop()
                };
            } else {
                // FIXME: also dirty, but rust does not let me go straight to children[j]
                unsafe {
                    SUnsafeCell::<BTreeNode<K, V>>::from_ptr(node.children[j].as_ptr()).drop()
                };
            }

            // FIXME: make this new node the root
            new.is_root = true;
            *node = new;
        }
    }

    fn delete_sibling(node: &mut BTreeNode<K, V>, i: usize, j: usize) {
        let mut child = node.children[i].get_cloned();

        if j > i {
            let mut child_right_sibling = node.children[j].get_cloned();

            node.keys[i] = child_right_sibling.keys.remove(0);

            if !child_right_sibling.children.is_empty() {
                child.children.push(child_right_sibling.children.remove(0));
            }

            unsafe {
                node.children[j].set(&child_right_sibling).unwrap();
            }
        } else {
            let mut child_left_sibling = node.children[j].get_cloned();
            child.keys.insert(0, node.keys.remove(i - 1));
            node.keys
                .insert(i - 1, child_left_sibling.keys.pop().unwrap());

            if !child_left_sibling.children.is_empty() {
                child
                    .children
                    .insert(0, child_left_sibling.children.pop().unwrap())
            }

            unsafe {
                node.children[j].set(&child_left_sibling).unwrap();
            }
        }

        unsafe {
            node.children[i].set(&child).unwrap();
        }
    }
}

impl<K: Ord + CandidType + DeserializeOwned, V: CandidType + DeserializeOwned> Default
    for SBTreeMap<K, V>
{
    fn default() -> Self {
        SBTreeMap::<K, V>::new()
    }
}

#[derive(CandidType, Deserialize)]
struct BTreeKey<K, V> {
    key: K,
    value_cell: SUnsafeCell<V>,
}

impl<K, V: CandidType + DeserializeOwned> BTreeKey<K, V> {
    pub fn new(key: K, value: &V) -> Result<Self, OutOfMemory> {
        Ok(Self {
            key,
            value_cell: SUnsafeCell::new(value)?,
        })
    }

    pub fn drop(self) -> V {
        let it = self.value_cell.get_cloned();
        self.value_cell.drop();

        it
    }
}

impl<K: Ord, V> Eq for BTreeKey<K, V> {}

impl<K: Ord, V> PartialEq<Self> for BTreeKey<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.key.eq(&other.key)
    }
}

impl<K: Ord, V> PartialOrd<Self> for BTreeKey<K, V> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.key.partial_cmp(&other.key)
    }
}

impl<K: Ord, V> Ord for BTreeKey<K, V> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key.cmp(&other.key)
    }

    fn min(self, other: Self) -> Self
    where
        Self: Sized,
    {
        if self < other {
            self
        } else {
            other
        }
    }

    fn max(self, other: Self) -> Self
    where
        Self: Sized,
    {
        if self > other {
            self
        } else {
            other
        }
    }

    fn clamp(self, min: Self, max: Self) -> Self
    where
        Self: Sized,
    {
        if self > max {
            max
        } else if self < min {
            min
        } else {
            self
        }
    }
}

#[derive(CandidType, Deserialize)]
struct BTreeNode<K, V> {
    is_leaf: bool,
    is_root: bool,
    keys: Vec<BTreeKey<K, V>>,
    children: Vec<SUnsafeCell<BTreeNode<K, V>>>,
}

impl<K: CandidType + DeserializeOwned, V: CandidType + DeserializeOwned> BTreeNode<K, V> {
    pub fn new(is_leaf: bool, is_root: bool) -> Self {
        Self {
            is_root,
            is_leaf,
            keys: Vec::new(),
            children: Vec::new(),
        }
    }

    fn dirty_clone(&self) -> Self {
        decode_one(&encode_one(self).unwrap()).unwrap()
    }
}

fn btree_to_sorted_vec<
    K: Ord + CandidType + DeserializeOwned + Clone,
    V: CandidType + DeserializeOwned,
>(
    btree_node: &BTreeNode<K, V>,
    vec: &mut Vec<(K, V)>,
) {
    for i in 0..btree_node.keys.len() {
        if let Some(child) = btree_node.children.get(i).map(|it| it.get_cloned()) {
            btree_to_sorted_vec(&child, vec);
        }
        let btree_key = &btree_node.keys[i];
        vec.push((btree_key.key.clone(), btree_key.value_cell.get_cloned()));
    }

    if let Some(child) = btree_node
        .children
        .get(btree_node.keys.len())
        .map(|it| it.get_cloned())
    {
        btree_to_sorted_vec(&child, vec);
    }
}

fn print_btree<
    K: Ord + CandidType + DeserializeOwned + Debug,
    V: CandidType + DeserializeOwned + Debug,
>(
    btree: &SBTreeMap<K, V>,
) {
    let mut nodes_1 = print_btree_level(&btree.root);
    println!();

    loop {
        let mut nodes_2 = vec![];

        for node in &nodes_1 {
            let res = print_btree_level(node);

            for n in res {
                nodes_2.push(n);
            }
        }

        println!();

        if nodes_2.is_empty() {
            break;
        }

        nodes_1 = nodes_2;
    }
}

fn print_btree_level<
    K: Ord + CandidType + DeserializeOwned + Debug,
    V: CandidType + DeserializeOwned + Debug,
>(
    btree_node: &BTreeNode<K, V>,
) -> Vec<BTreeNode<K, V>> {
    let mut children = vec![];

    print!(
        "{:?}",
        btree_node
            .keys
            .iter()
            .map(|it| (&it.key, it.value_cell.get_cloned()))
            .collect::<Vec<_>>()
    );

    for ch in &btree_node.children {
        children.push(ch.get_cloned());
    }

    children
}

#[cfg(test)]
mod tests {
    use crate::collections::btree_map::{btree_to_sorted_vec, print_btree, SBTreeMap};
    use crate::{init_allocator, stable};

    #[test]
    fn works_as_expected() {
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

        let mut map = SBTreeMap::<u64, u64>::new_with_degree(4);

        println!("INSERTION");

        assert!(map.insert(30, &3).unwrap().is_none());
        print_btree(&map);
        println!();

        assert!(map.insert(90, &9).unwrap().is_none());
        print_btree(&map);
        println!();

        assert!(map.insert(10, &1).unwrap().is_none());
        print_btree(&map);
        println!();

        assert!(map.insert(70, &7).unwrap().is_none());
        print_btree(&map);
        println!();

        assert!(map.insert(80, &8).unwrap().is_none());
        print_btree(&map);
        println!();

        assert!(map.insert(50, &5).unwrap().is_none());
        print_btree(&map);
        println!();

        assert!(map.insert(20, &2).unwrap().is_none());
        print_btree(&map);
        println!();

        assert!(map.insert(60, &6).unwrap().is_none());
        print_btree(&map);
        println!();

        assert!(map.insert(40, &4).unwrap().is_none());
        print_btree(&map);
        println!();

        assert_eq!(map.len(), 9);

        let mut probe = vec![];
        btree_to_sorted_vec(&map.root, &mut probe);
        assert_eq!(example, probe);

        println!("DELETION");

        assert_eq!(map.delete(&30).unwrap(), 3);
        print_btree(&map);
        println!();

        assert_eq!(map.delete(&70).unwrap(), 7);
        print_btree(&map);
        println!();

        assert_eq!(map.delete(&50).unwrap(), 5);
        print_btree(&map);
        println!();

        assert_eq!(map.delete(&40).unwrap(), 4);
        print_btree(&map);
        println!();

        assert_eq!(map.delete(&60).unwrap(), 6);
        print_btree(&map);
        println!();

        assert_eq!(map.delete(&20).unwrap(), 2);
        print_btree(&map);
        println!();

        assert_eq!(map.delete(&80).unwrap(), 8);
        print_btree(&map);
        println!();

        assert_eq!(map.delete(&10).unwrap(), 1);
        print_btree(&map);
        println!();

        assert_eq!(map.delete(&90).unwrap(), 9);
        print_btree(&map);
        println!();

        assert!(map.is_empty());
    }
}
