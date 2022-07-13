use crate::{OutOfMemory, SUnsafeCell};
use candid::{CandidType, Deserialize};
use serde::de::DeserializeOwned;
use std::cmp::Ordering;

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
    keys: Vec<BTreeKey<K, V>>,
    children: Vec<SUnsafeCell<BTreeNode<K, V>>>,
}

impl<K, V> BTreeNode<K, V> {
    pub fn new(is_leaf: bool) -> Self {
        Self {
            is_leaf,
            keys: Vec::new(),
            children: Vec::new(),
        }
    }
}

/// OOMs work really bad - I can't put my finger on that recursion

#[derive(CandidType, Deserialize)]
pub struct BTreeMap<K, V> {
    root: BTreeNode<K, V>,
    t: usize,
}

impl<K: Ord + CandidType + DeserializeOwned, V: CandidType + DeserializeOwned> BTreeMap<K, V> {
    pub fn new(t: usize) -> Self {
        Self {
            t,
            root: BTreeNode::<K, V>::new(true),
        }
    }

    pub fn insert(&mut self, key: K, value: &V) -> Result<Option<V>, OutOfMemory> {
        let root = self.root;
        let btree_key = BTreeKey::new(key, value)?;

        if root.keys.len() == 2 * self.t - 1 {
            let mut temp = BTreeNode::new(false);
            self.root = temp;
            temp.children.insert(0, SUnsafeCell::new(&root)?);

            if matches!(self.split_child(&mut temp, 0), Err(_)) {
                btree_key.drop();
                return Err(OutOfMemory);
            }

            self.insert_non_full(&mut temp, btree_key)
        } else {
            self.insert_non_full(&mut root, btree_key)
        }
    }

    pub fn insert_non_full(
        &mut self,
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
                    if node.children[idx].get_cloned().keys.len() == 2 * self.t - 1 {
                        if matches!(self.split_child(node, idx), Err(_)) {
                            key.drop();
                            return Err(OutOfMemory);
                        }

                        if key > node.keys[idx] {
                            idx += 1;
                        }
                    }

                    let mut child = node.children[idx].get_cloned();
                    let result = self.insert_non_full(&mut child, key);

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

    pub fn split_child(
        &mut self,
        node: &mut BTreeNode<K, V>,
        idx: usize,
    ) -> Result<(), OutOfMemory> {
        let mut child = node.children[idx].get_cloned();
        let mut new_child = BTreeNode::<K, V>::new(child.is_leaf);

        node.keys.insert(idx, child.keys[self.t - 1]);
        for i in self.t..(self.t * 2 - 1) {
            new_child.keys.push(child.keys.remove(i));
        }

        if !child.is_leaf {
            for i in self.t..(self.t * 2) {
                new_child.children.push(child.children.remove(i))
            }
        }

        // insert afterwards
        unsafe {
            node.children[idx].set(&child).unwrap();
        }
        node.children
            .insert(idx + 1, SUnsafeCell::new(&new_child).unwrap());

        Ok(())
    }
}
