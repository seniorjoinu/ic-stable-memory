use crate::mem::membox::s::{SBox, SBoxError};
use crate::utils::encode::AsBytes;
use crate::OutOfMemory;
use candid::types::{Field, Label, Serializer, Type};
use candid::{encode_one, CandidType, Error as CandidError};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::cmp::Ordering;
use std::marker::PhantomData;
use std::mem;
use std::mem::size_of;

const DEFAULT_STABLE_BTREE_ORDER: u32 = 50;

pub struct SBTreeMap<K: Copy + Ord, V: Copy>(SBox<SBTree>, PhantomData<K>, PhantomData<V>);

#[derive(CandidType, Deserialize)]
pub struct SBTree {
    order: u32,
    len: u64,
    head: Option<SBox<SBTreeNode>>,
}

impl SBTree {
    pub fn new(order: u32) -> Self {
        Self {
            order,
            len: 0,
            head: None,
        }
    }

    fn binary_search(&self, key: &Vec<u8>) -> Option<(SBox<SBTreeNode>, Result<usize, usize>)> {
        let mut node_box = self.head?;

        loop {
            let node = node_box.get_cloned().unwrap();

            let res = node.binary_search(key);
            match res {
                Ok(idx) => return Some((node_box, Ok(idx))),
                Err(idx) => {
                    if let Some(child) = node.children.get(idx) {
                        node_box = *child;
                    } else {
                        return Some((node_box, Err(idx)));
                    }
                }
            };
        }
    }
}

#[derive(CandidType, Deserialize)]
pub struct SBTreeNode {
    pub parent: Option<SBox<SBTreeNode>>,
    pub elems: Vec<(Vec<u8>, Vec<u8>)>,
    pub children: Vec<SBox<SBTreeNode>>,
}

impl SBTreeNode {
    pub fn new(parent: Option<SBox<SBTreeNode>>) -> Self {
        Self {
            parent,
            elems: Vec::new(),
            children: Vec::new(),
        }
    }

    fn binary_search(&self, key: &Vec<u8>) -> Result<usize, usize> {
        self.elems.binary_search_by(|it| it.0.cmp(key))
    }

    fn replace_child(&mut self, prev_ptr: u64, child: SBox<SBTreeNode>) {
        for i in 0..self.children.len() {
            if self.children[i].as_raw().get_ptr() == prev_ptr {
                self.children[i] = child;
                return;
            }
        }
    }

    fn split(&mut self) -> Self {
        let mut elems = Vec::new();
        for _ in 0..(self.elems.len() / 2) {
            elems.push(self.elems.pop().unwrap());
        }

        let mut children = Vec::new();
        for _ in 0..(self.elems.len() / 2 + 2) {
            children.push(self.children.pop().unwrap());
        }

        Self {
            parent: self.parent,
            elems,
            children,
        }
    }

    fn to_sorted_vec(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
        if self.children.is_empty() {
            return self.elems.clone();
        }

        let mut result = Vec::new();
        for i in 0..self.elems.len() {
            result.extend(self.children[i].get_cloned().unwrap().to_sorted_vec());
            result.push(self.elems[i].clone());
        }
        result.extend(
            self.children[self.elems.len()]
                .get_cloned()
                .unwrap()
                .to_sorted_vec(),
        );

        result
    }
}

impl<K: Copy + Ord, V: Copy> Default for SBTreeMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Copy + Ord, V: Copy> SBTreeMap<K, V> {
    pub fn new() -> Self {
        Self::with_order(DEFAULT_STABLE_BTREE_ORDER)
    }

    pub fn with_order(order: u32) -> Self {
        assert!(order > 1);

        let tree = SBTree::new(order);

        Self(
            SBox::new(&tree).unwrap(),
            PhantomData::default(),
            PhantomData::default(),
        )
    }

    pub fn len(&self) -> u64 {
        self.tree().get_cloned().unwrap().len
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn order(&self) -> u32 {
        self.tree().get_cloned().unwrap().order
    }

    pub unsafe fn insert(&mut self, key: K, value: V) -> Result<(bool, Option<V>), OutOfMemory> {
        let key_bytes = key.as_bytes();
        let value_bytes = value.as_bytes();
        let mut tree = self.tree().get_cloned().unwrap();

        let (mut node_box, res) = if let Some(it) = tree.binary_search(&key_bytes) {
            it
        } else {
            let head = SBox::new(&SBTreeNode::new(None)).unwrap();
            tree.head = Some(head);

            (head, Err(0))
        };

        let mut node = node_box.get_cloned().unwrap();
        let mut should_update = false;

        match res {
            Ok(idx) => {
                let prev_v = mem::replace(&mut node.elems[idx].1, value_bytes);
                if self.set_node(node_box, &node)? {
                    should_update = true;
                }

                Ok((should_update, Some(V::from_bytes(&prev_v))))
            }
            Err(idx) => {
                node.elems.insert(idx, (key_bytes, value_bytes));
                if self.set_node(node_box, &node)? {
                    should_update = true;
                }

                tree.len += 1;
                if self.set_tree(self.0, &tree)? {
                    should_update = true;
                }

                while node.elems.len() as u32 > tree.order {
                    let (should_update1, parent) = self.promote_node(&mut node)?;

                    if self.set_node(node_box, &node)? || should_update1 {
                        should_update = true;
                    }

                    node_box = parent;
                    node = node_box.get_cloned().unwrap();
                }

                Ok((should_update, None))
            }
        }
    }

    fn tree(&self) -> SBox<SBTree> {
        self.0
    }

    fn head(&self) -> Option<SBox<SBTreeNode>> {
        self.tree().get_cloned().unwrap().head
    }

    unsafe fn set_tree(
        &mut self,
        mut tree_box: SBox<SBTree>,
        tree: &SBTree,
    ) -> Result<bool, OutOfMemory> {
        let should_update = tree_box.set(tree).map_err(SBoxError::unwrap_oom)?;

        if should_update {
            self.0 = tree_box;
        }

        Ok(should_update)
    }

    unsafe fn set_node(
        &mut self,
        mut node_box: SBox<SBTreeNode>,
        node: &SBTreeNode,
    ) -> Result<bool, OutOfMemory> {
        let mut prev_ptr = node_box.as_raw().get_ptr();
        let mut node_parent = node.parent;
        let mut should_update = node_box.set(node).map_err(SBoxError::unwrap_oom)?;

        while should_update {
            match node_parent {
                Some(mut p) => {
                    let mut parent = p.get_cloned().unwrap();
                    parent.replace_child(prev_ptr, node_box);

                    prev_ptr = p.as_raw().get_ptr();
                    node_parent = parent.parent;
                    should_update = p.set(&parent).map_err(SBoxError::unwrap_oom)?;
                }
                None => {
                    let tree_box = self.tree();
                    let mut tree = tree_box.get_cloned().unwrap();

                    tree.head = Some(node_box);
                    let should_update = self.set_tree(tree_box, &tree)?;

                    return Ok(should_update);
                }
            }
        }

        Ok(false)
    }

    unsafe fn promote_node(
        &mut self,
        node: &mut SBTreeNode,
    ) -> Result<(bool, SBox<SBTreeNode>), OutOfMemory> {
        let mut parent_box = if let Some(parent) = &node.parent {
            *parent
        } else {
            let parent = SBox::new(&SBTreeNode::new(None)).map_err(SBoxError::unwrap_oom)?;
            node.parent = Some(parent);

            parent
        };

        let mut parent = parent_box.get_cloned().unwrap();

        // insert mid
        let (mid_k, mid_v) = node.elems.remove(node.elems.len() / 2);
        let idx = parent.binary_search(&mid_k).unwrap();
        parent.elems.insert(idx, (mid_k, mid_v));

        // insert right (left should already point to the correct branch)
        let right = SBox::new(&node.split()).map_err(SBoxError::unwrap_oom)?;
        parent.children.insert(idx + 1, right);

        let should_update = self.set_node(parent_box, &parent)?;

        Ok((should_update, parent_box))
    }

    fn to_sorted_vec(&self) -> Vec<(K, V)> {
        let head = if let Some(h) = self.head() {
            h.get_cloned().unwrap()
        } else {
            return Vec::new();
        };

        head.to_sorted_vec()
            .into_iter()
            .map(|(k, v)| (K::from_bytes(&k), V::from_bytes(&v)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::btree::{SBTreeMap, SBTreeNode};
    use crate::init_allocator;
    use crate::utils::mem_context::stable;

    fn assert_obeys_rules<K: Copy + Ord, V: Copy>(map: &SBTreeMap<K, V>) {
        let node = if let Some(head) = map.head() {
            head.get_cloned().unwrap()
        } else {
            return;
        };

        _assert_obeys_rules(&node, map.order());
    }

    fn _assert_obeys_rules(node: &SBTreeNode, order: u32) {
        assert!(!node.elems.is_empty() && node.elems.len() as u32 <= order);
        assert!(node.children.len() == node.elems.len() + 1 || node.children.is_empty());

        for child in &node.children {
            _assert_obeys_rules(&child.get_cloned().unwrap(), order);
        }
    }

    #[test]
    fn insertion_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let elements = vec![
            1, 4, 2, 6, 7, 8, 3, 9, 12, 43, 65, 34, 24, 78, 13, 98, 132, 21, 18, 19, 500, 95, 92,
            41, 40, 55, 10,
        ];

        let mut map = SBTreeMap::with_order(2);
        assert_obeys_rules(&map);

        let mut control = Vec::new();

        for elem in elements {
            control.push(elem);
            control.sort();

            unsafe { map.insert(elem, ()).unwrap() };
            let probe = map
                .to_sorted_vec()
                .into_iter()
                .map(|(k, _)| k)
                .collect::<Vec<_>>();

            println!("{:?}", control);
            println!("{:?}", probe);
            println!();

            assert_obeys_rules(&map);
            assert_eq!(probe, control);
        }
    }
}
