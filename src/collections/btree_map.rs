use crate::SUnsafeCell;
use speedy::{LittleEndian, Readable, Writable};
use std::cmp::Ordering;

const DEFAULT_BTREE_DEGREE: usize = 4096;

#[derive(Readable, Writable)]
pub struct SBTreeMap<K, V> {
    root: BTreeNode<K, V>,
    degree: usize,
    len: u64,
}

impl<
        'a,
        K: Ord + Readable<'a, LittleEndian> + Writable<LittleEndian>,
        V: Readable<'a, LittleEndian> + Writable<LittleEndian>,
    > SBTreeMap<K, V>
{
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

    pub fn insert(&mut self, key: K, value: &V) -> Option<V> {
        let root = &mut self.root;
        let btree_key = BTreeKey::new(key, value);

        let res = if root.keys.len() == 2 * self.degree - 1 {
            let mut temp = BTreeNode::new(false, false);

            root.is_root = false;
            temp.children.insert(0, SUnsafeCell::new(root));

            Self::split_child(self.degree, &mut temp, 0);
            let res = Self::insert_non_full(self.degree, &mut temp, btree_key);

            self.root = temp;
            self.root.is_root = true;

            res
        } else {
            Self::insert_non_full(self.degree, &mut self.root, btree_key)
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

    pub fn drop(mut self) {
        while let Some(key) = self.root.keys.pop() {
            key.drop();
        }

        while let Some(child_node) = self.root.children.pop() {
            self._drop(child_node);
        }
    }

    pub fn get_cloned(&self, key: &K) -> Option<V> {
        self._get(&self.root, key)
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self._contains_key(&self.root, key)
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
    ) -> Option<V> {
        match node.keys.binary_search_by(|k| k.get_cloned().cmp(&key)) {
            Ok(idx) => {
                let new_key_cell = SUnsafeCell::new(&key);
                let old_key_cell = std::mem::replace(&mut node.keys[idx], new_key_cell);

                let old_key = old_key_cell.get_cloned();
                old_key_cell.drop();

                Some(old_key.drop())
            }
            Err(mut idx) => {
                if node.is_leaf {
                    let new_key_cell = SUnsafeCell::new(&key);

                    node.keys.insert(idx, new_key_cell);
                    None
                } else {
                    if node.children[idx].get_cloned().keys.len() == 2 * degree - 1 {
                        Self::split_child(degree, node, idx);

                        if key > node.keys[idx].get_cloned() {
                            idx += 1;
                        }
                    }

                    let mut child = node.children[idx].get_cloned();
                    let result = Self::insert_non_full(degree, &mut child, key);

                    unsafe { node.children[idx].set(&child) };

                    result
                }
            }
        }
    }

    fn split_child(degree: usize, node: &mut BTreeNode<K, V>, idx: usize) {
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

        unsafe { node.children[idx].set(&child) };
        node.children.insert(idx + 1, SUnsafeCell::new(&new_child));
    }

    fn _contains_key(&self, node: &BTreeNode<K, V>, key: &K) -> bool {
        match node.keys.binary_search_by(|k| k.get_cloned().key.cmp(key)) {
            Ok(_) => true,
            Err(idx) => {
                if let Some(child_cell) = node.children.get(idx) {
                    let child = child_cell.get_cloned();

                    self._contains_key(&child, key)
                } else {
                    false
                }
            }
        }
    }

    fn _get(&self, node: &BTreeNode<K, V>, key: &K) -> Option<V> {
        match node.keys.binary_search_by(|k| k.get_cloned().key.cmp(key)) {
            Ok(idx) => Some(node.keys[idx].get_cloned().value_cell.get_cloned()),
            Err(idx) => {
                let child = node.children.get(idx)?.get_cloned();
                self._get(&child, key)
            }
        }
    }

    fn _drop(&mut self, node_cell: SUnsafeCell<BTreeNode<K, V>>) {
        let mut node = node_cell.get_cloned();
        for child_cell in node.children {
            self._drop(child_cell);
        }

        while let Some(key) = node.keys.pop() {
            key.drop();
        }
        node_cell.drop();
    }

    fn _delete(degree: usize, node: &mut BTreeNode<K, V>, key: &K) -> Option<V> {
        match node.keys.binary_search_by(|k| k.get_cloned().key.cmp(key)) {
            Ok(idx) => {
                if node.is_leaf {
                    let btree_key_cell = node.keys.remove(idx);

                    let btree_key = btree_key_cell.get_cloned();
                    btree_key_cell.drop();

                    Some(btree_key.drop())
                } else {
                    Self::delete_internal_node(degree, node, key, idx)
                }
            }
            Err(idx) => {
                let mut merged = false;

                if node.is_leaf {
                    return None;
                }

                let mut child = node.children[idx].get_cloned();

                if child.keys.len() >= degree {
                    let res = Self::_delete(degree, &mut child, key);
                    unsafe { node.children[idx].set(&child) };

                    res
                } else {
                    if idx != 0 && idx + 1 < node.children.len() {
                        let left_child_sibling = node.children[idx - 1].get_cloned();
                        let right_child_sibling = node.children[idx + 1].get_cloned();

                        if left_child_sibling.keys.len() >= degree {
                            Self::delete_sibling(node, idx, idx - 1);
                        } else if right_child_sibling.keys.len() >= degree {
                            Self::delete_sibling(node, idx, idx + 1);
                        } else {
                            Self::delete_merge(node, idx, idx + 1);
                            merged = true;
                        }
                    } else if idx == 0 {
                        let right_child_sibling = node.children[idx + 1].get_cloned();

                        if right_child_sibling.keys.len() >= degree {
                            Self::delete_sibling(node, idx, idx + 1);
                        } else {
                            Self::delete_merge(node, idx, idx + 1);
                            merged = true;
                        }
                    } else if idx + 1 == node.children.len() {
                        let left_child_sibling = node.children[idx - 1].get_cloned();

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

                    let mut child = node.children[idx].get_cloned();
                    let res = Self::_delete(degree, &mut child, key);
                    unsafe { node.children[idx].set(&child) };

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
        let mut left_child = node.children[idx].get_cloned();
        let mut right_child = node.children[idx + 1].get_cloned();

        if left_child.keys.len() >= degree {
            let btree_key_cell = std::mem::replace(
                &mut node.keys[idx],
                Self::delete_predecessor(degree, &mut left_child),
            );
            unsafe { node.children[idx].set(&left_child) };

            let btree_key = btree_key_cell.get_cloned();
            btree_key_cell.drop();

            Some(btree_key.drop())
        } else if right_child.keys.len() >= degree {
            let btree_key_cell = std::mem::replace(
                &mut node.keys[idx],
                Self::delete_successor(degree, &mut right_child),
            );
            unsafe { node.children[idx + 1].set(&right_child) };

            let btree_key = btree_key_cell.get_cloned();
            btree_key_cell.drop();

            Some(btree_key.drop())
        } else {
            Self::delete_merge(node, idx, idx + 1);
            Self::_delete(degree, node, key)
        }
    }

    fn delete_predecessor(
        degree: usize,
        child: &mut BTreeNode<K, V>,
    ) -> SUnsafeCell<BTreeKey<K, V>> {
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

        unsafe { child.children[n].set(&grand_child) };

        res
    }

    fn delete_successor(degree: usize, child: &mut BTreeNode<K, V>) -> SUnsafeCell<BTreeKey<K, V>> {
        if child.is_leaf {
            return child.keys.remove(0);
        }

        let grand_child = child.children[0].get_cloned();

        if grand_child.keys.len() >= degree {
            Self::delete_sibling(child, 0, 1);
        } else {
            Self::delete_merge(child, 0, 1);
        }

        let mut grand_child = child.children[0].get_cloned();
        let res = Self::delete_successor(degree, &mut grand_child);

        unsafe { child.children[0].set(&grand_child) };

        res
    }

    fn delete_merge(node: &mut BTreeNode<K, V>, i: usize, j: usize) {
        let mut child = node.children[i].get_cloned();

        let mut new = if j > i {
            let child_right_sibling = node.children[j].get_cloned();
            child.keys.push(node.keys.remove(i));

            child.keys.extend(child_right_sibling.keys);
            child.children.extend(child_right_sibling.children);

            unsafe { node.children[i].set(&child) };

            let child_right_sibling_ptr = node.children.remove(j);
            child_right_sibling_ptr.drop();

            child
        } else {
            let mut child_left_sibling = node.children[j].get_cloned();
            child_left_sibling.keys.push(node.keys.remove(j));

            child_left_sibling.keys.extend(child.keys);
            child_left_sibling.children.extend(child.children);

            unsafe { node.children[j].set(&child_left_sibling) };

            let child_ptr = node.children.remove(i);
            child_ptr.drop();

            child_left_sibling
        };

        if node.is_root && node.keys.is_empty() {
            // dealing with memory leaks - remove the element from stable memory, if it becomes root
            if j > i {
                // FIXME: dirty, but rust does not let me go straight to children[i], since I want to consume it during drop()
                unsafe {
                    SUnsafeCell::<BTreeNode<K, V>>::from_ptr(node.children[i].as_ptr()).drop()
                };
            } else {
                // FIXME: dirty, but rust does not let me go straight to children[j], since I want to consume it during drop()
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

            child.keys.push(node.keys.remove(i));
            node.keys.insert(i, child_right_sibling.keys.remove(0));

            if !child_right_sibling.children.is_empty() {
                child.children.push(child_right_sibling.children.remove(0));
            }

            unsafe { node.children[j].set(&child_right_sibling) };
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

            unsafe { node.children[j].set(&child_left_sibling) };
        }

        unsafe { node.children[i].set(&child) };
    }
}

impl<
        'a,
        K: Ord + Readable<'a, LittleEndian> + Writable<LittleEndian>,
        V: Readable<'a, LittleEndian> + Writable<LittleEndian>,
    > Default for SBTreeMap<K, V>
{
    fn default() -> Self {
        SBTreeMap::<K, V>::new()
    }
}

#[derive(Readable, Writable)]
struct BTreeKey<K, V> {
    key: K,
    value_cell: SUnsafeCell<V>,
}

impl<'a, K, V: Readable<'a, LittleEndian> + Writable<LittleEndian>> BTreeKey<K, V> {
    pub fn new(key: K, value: &V) -> Self {
        Self {
            key,
            value_cell: SUnsafeCell::new(value),
        }
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

#[derive(Readable, Writable)]
struct BTreeNode<K, V> {
    is_leaf: bool,
    is_root: bool,
    keys: Vec<SUnsafeCell<BTreeKey<K, V>>>,
    children: Vec<SUnsafeCell<BTreeNode<K, V>>>,
}

impl<
        'a,
        K: Readable<'a, LittleEndian> + Writable<LittleEndian>,
        V: Readable<'a, LittleEndian> + Writable<LittleEndian>,
    > BTreeNode<K, V>
{
    pub fn new(is_leaf: bool, is_root: bool) -> Self {
        Self {
            is_root,
            is_leaf,
            keys: Vec::new(),
            children: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::btree_map::{BTreeKey, BTreeNode, SBTreeMap};
    use crate::utils::isoprint;
    use crate::{init_allocator, stable};
    use speedy::{LittleEndian, Readable, Writable};
    use std::cmp::{max, min};
    use std::fmt::Debug;

    #[ignore]
    fn btree_to_sorted_vec<
        'a,
        K: Ord + Readable<'a, LittleEndian> + Writable<LittleEndian> + Clone,
        V: Readable<'a, LittleEndian> + Writable<LittleEndian>,
    >(
        btree_node: &BTreeNode<K, V>,
        vec: &mut Vec<(K, V)>,
    ) {
        for i in 0..btree_node.keys.len() {
            if let Some(child) = btree_node.children.get(i).map(|it| it.get_cloned()) {
                btree_to_sorted_vec(&child, vec);
            }
            let btree_key = &btree_node.keys[i].get_cloned();
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

    #[ignore]
    fn print_btree<
        'a,
        K: Ord + Readable<'a, LittleEndian> + Writable<LittleEndian> + Debug,
        V: Readable<'a, LittleEndian> + Writable<LittleEndian> + Debug,
    >(
        btree: &SBTreeMap<K, V>,
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
        'a,
        K: Ord + Readable<'a, LittleEndian> + Writable<LittleEndian> + Debug,
        V: Readable<'a, LittleEndian> + Writable<LittleEndian> + Debug,
    >(
        btree_node: &BTreeNode<K, V>,
    ) -> Vec<BTreeNode<K, V>> {
        let mut children = vec![];

        print!(
            "( is_leaf: {}, is_root: {} - {:?} )",
            btree_node.is_leaf,
            btree_node.is_root,
            btree_node
                .keys
                .iter()
                .map(|it| (it.get_cloned().key, it.get_cloned().value_cell.get_cloned()))
                .collect::<Vec<_>>()
        );

        for ch in &btree_node.children {
            children.push(ch.get_cloned());
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

        let mut map = SBTreeMap::<u64, u64>::new_with_degree(3);

        println!("INSERTION");

        assert!(map.insert(30, &3).is_none());
        print_btree(&map);
        println!();

        assert!(map.insert(90, &9).is_none());
        print_btree(&map);
        println!();

        assert!(map.insert(10, &1).is_none());
        print_btree(&map);
        println!();

        assert!(map.insert(70, &7).is_none());
        print_btree(&map);
        println!();

        assert!(map.insert(80, &8).is_none());
        print_btree(&map);
        println!();

        assert!(map.insert(50, &5).is_none());
        print_btree(&map);
        println!();

        assert!(map.insert(20, &2).is_none());
        print_btree(&map);
        println!();

        assert!(map.insert(60, &6).is_none());
        print_btree(&map);
        println!();

        assert!(map.insert(40, &4).is_none());
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

        map.drop();
    }

    #[test]
    fn sequential_works_as_expected() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SBTreeMap::<u64, u64>::new_with_degree(2);

        println!("INSERTION");

        for i in 0..10 {
            map.insert(i, &0);
            print_btree(&map);
            println!();
        }

        println!("DELETION");

        for i in 0..10 {
            map.remove(&i).unwrap();
            print_btree(&map);
            println!();
        }

        map.drop();
    }

    #[test]
    fn basic_flow_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SBTreeMap::<u64, u64>::new();

        let prev = map.insert(1, &10);
        assert!(prev.is_none());

        let val = map.get_cloned(&1).unwrap();
        assert_eq!(val, 10);
        assert!(map.contains_key(&1));

        assert!(map.insert(2, &20).is_none());
        map.insert(3, &30);
        map.insert(4, &40);
        map.insert(5, &50);

        let val = map.insert(3, &130).unwrap();
        assert_eq!(val, 30);

        assert!(!map.contains_key(&99));
        assert!(map.remove(&99).is_none());

        map.drop();

        let map = SBTreeMap::<u64, u64>::default();
    }

    #[test]
    fn deletion_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SBTreeMap::<u64, u64>::new_with_degree(5);

        for i in 0..50 {
            map.insert(i + 10, &i);
        }

        let val = map.insert(13, &130).unwrap();
        assert_eq!(val, 3);

        let val1 = map.get_cloned(&13).unwrap();
        assert_eq!(val1, 130);

        assert!(!map.contains_key(&99));
        assert!(map.remove(&99).is_none());

        map.insert(13, &3);
        assert_eq!(map.remove(&16).unwrap(), 6);

        map.insert(16, &6);
        map.insert(9, &90);

        assert_eq!(map.remove(&16).unwrap(), 6);

        map.insert(16, &6);
        assert_eq!(map.remove(&9).unwrap(), 90);
        assert_eq!(map.remove(&53).unwrap(), 43);

        map.insert(60, &70);
        map.insert(61, &71);
        assert_eq!(map.remove(&58).unwrap(), 48);

        map.drop();

        let mut map = SBTreeMap::<u64, u64>::new_with_degree(5);

        for i in 0..50 {
            map.insert(i * 2, &i);
        }

        map.insert(85, &1);
        assert_eq!(map.remove(&88).unwrap(), 44);

        map.drop();

        let mut map = SBTreeMap::<u64, u64>::new_with_degree(3);

        for i in 0..50 {
            map.insert(i * 2, &i);
        }

        map.remove(&94);
        map.remove(&96);
        map.remove(&98);

        assert_eq!(map.remove(&88).unwrap(), 44);

        map.insert(81, &1);
        map.insert(83, &1);
        map.insert(94, &1);
        map.insert(85, &1);

        assert_eq!(map.remove(&86).unwrap(), 43);

        map.insert(71, &1);
        map.insert(73, &1);
        map.insert(75, &1);
        map.insert(77, &1);
        map.insert(79, &1);

        map.insert(47, &1);
        map.insert(49, &1);
        map.insert(51, &1);
        map.insert(53, &1);
        map.insert(55, &1);
        map.insert(57, &1);
        map.insert(59, &1);
        map.insert(61, &1);
        map.insert(63, &1);
        map.insert(65, &1);
        map.insert(67, &1);
        map.insert(69, &1);

        print_btree(&map);

        map.drop();

        let mut map = SBTreeMap::<u64, u64>::new_with_degree(3);

        for i in 150..300 {
            map.insert(i, &i);
        }
        for i in 0..150 {
            map.insert(i, &i);
        }

        assert_eq!(map.remove(&203).unwrap(), 203);
        assert_eq!(map.remove(&80).unwrap(), 80);

        print_btree(&map);

        map.drop();
    }

    #[test]
    fn complex_deletes_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SBTreeMap::<u64, u64>::new_with_degree(3);

        for i in 0..75 {
            map.insert(i, &i);
        }

        for i in 0..75 {
            map.insert(150 - i, &i);
        }

        for i in 0..150 {
            let j = if i % 2 == 0 { i } else { 150 - i };

            if j % 3 == 0 {
                map.remove(&j);
            }
        }

        map.drop();

        let mut map = SBTreeMap::<u64, u64>::new_with_degree(3);

        for i in 0..150 {
            map.insert(150 - i, &i);
        }

        for i in 0..150 {
            map.remove(&(150 - i));
        }

        map.drop();
    }

    #[test]
    fn keys_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        assert!(BTreeKey::new(10, &20) < BTreeKey::new(20, &20));
        assert!(min(BTreeKey::new(10, &20), BTreeKey::new(20, &20)) == BTreeKey::new(10, &20));
        assert!(max(BTreeKey::new(10, &20), BTreeKey::new(20, &20)) == BTreeKey::new(20, &20));

        assert!(
            BTreeKey::new(9, &20).clamp(BTreeKey::new(10, &20), BTreeKey::new(20, &20))
                == BTreeKey::new(10, &20)
        );
        assert!(
            BTreeKey::new(21, &20).clamp(BTreeKey::new(10, &20), BTreeKey::new(20, &20))
                == BTreeKey::new(20, &20)
        );
        assert!(
            BTreeKey::new(15, &20).clamp(BTreeKey::new(10, &20), BTreeKey::new(20, &20))
                == BTreeKey::new(15, &20)
        );
    }
}
