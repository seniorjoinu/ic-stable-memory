use crate::primitive::s_cellbox::SCellBox;
use crate::utils::encode::AsBytes;
use crate::OutOfMemory;
use candid::CandidType;
use serde::Deserialize;
use std::marker::PhantomData;
use std::mem;

const DEFAULT_STABLE_BTREE_ORDER: u32 = 50;

pub struct SBTreeMap<K: Sized + AsBytes + Ord, V: Sized + AsBytes>(
    SCellBox<SBTree>,
    PhantomData<K>,
    PhantomData<V>,
);

#[derive(CandidType, Deserialize)]
pub struct SBTree {
    order: u32,
    len: u64,
    head: Option<SCellBox<SBTreeNode>>,
}

impl SBTree {
    pub fn new(order: u32) -> Self {
        Self {
            order,
            len: 0,
            head: None,
        }
    }

    fn binary_search(&self, key: &Vec<u8>) -> Option<(SCellBox<SBTreeNode>, Result<usize, usize>)> {
        let mut node_box = self.head.clone()?;

        loop {
            let node = node_box.get_cloned();

            let res = node.binary_search(key);
            match res {
                Ok(idx) => return Some((node_box, Ok(idx))),
                Err(idx) => {
                    if let Some(child) = node.children.get(idx) {
                        node_box = child.clone();
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
    pub parent: Option<SCellBox<SBTreeNode>>,
    pub elems: Vec<(Vec<u8>, Vec<u8>)>,
    pub children: Vec<SCellBox<SBTreeNode>>,
}

impl SBTreeNode {
    pub fn new(parent: Option<SCellBox<SBTreeNode>>) -> Self {
        Self {
            parent,
            elems: Vec::new(),
            children: Vec::new(),
        }
    }

    fn binary_search(&self, key: &Vec<u8>) -> Result<usize, usize> {
        self.elems.binary_search_by(|it| it.0.cmp(key))
    }

    fn split(&mut self) -> Self {
        let mut elems = Vec::new();
        for _ in 0..(self.elems.len() / 2) {
            elems.push(self.elems.pop().unwrap());
        }

        let mut children = Vec::new();
        if !self.children.is_empty() {
            for _ in 0..(self.elems.len() / 2 + 2) {
                children.push(self.children.pop().unwrap());
            }
        }

        Self {
            parent: self.parent.clone(),
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
            result.extend(self.children[i].get_cloned().to_sorted_vec());
            result.push(self.elems[i].clone());
        }
        result.extend(self.children[self.elems.len()].get_cloned().to_sorted_vec());

        result
    }
}

impl<K: Sized + AsBytes + Ord, V: Sized + AsBytes> SBTreeMap<K, V> {
    pub fn new() -> Result<Self, OutOfMemory> {
        Self::with_order(DEFAULT_STABLE_BTREE_ORDER)
    }

    pub fn with_order(order: u32) -> Result<Self, OutOfMemory> {
        assert!(order > 1);

        let tree = SBTree::new(order);

        Ok(Self(
            SCellBox::new(&tree)?,
            PhantomData::default(),
            PhantomData::default(),
        ))
    }

    pub fn len(&self) -> u64 {
        self.tree().get_cloned().len
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn order(&self) -> u32 {
        self.tree().get_cloned().order
    }

    pub fn insert(&mut self, key: K, value: V) -> Result<Option<V>, OutOfMemory> {
        let key_bytes = unsafe { key.as_bytes() };
        let value_bytes = unsafe { value.as_bytes() };
        let mut tree = self.tree().get_cloned();

        let (mut node_box, res) = if let Some(it) = tree.binary_search(&key_bytes) {
            it
        } else {
            let head = SCellBox::new(&SBTreeNode::new(None))?;
            tree.head = Some(head.clone());

            (head, Err(0))
        };

        let mut node = node_box.get_cloned();

        match res {
            Ok(idx) => {
                let prev_v = mem::replace(&mut node.elems[idx].1, value_bytes);
                node_box.set(&node)?;

                unsafe { Ok(Some(V::from_bytes(&prev_v))) }
            }
            Err(idx) => {
                node.elems.insert(idx, (key_bytes, value_bytes));
                node_box.set(&node)?;

                tree.len += 1;
                self.0.set(&tree)?;

                while node.elems.len() as u32 > tree.order {
                    let parent = self.promote_node(&mut node)?;

                    node_box.set(&node)?;

                    node_box = parent;
                    node = node_box.get_cloned();
                }

                Ok(None)
            }
        }
    }

    fn tree(&self) -> SCellBox<SBTree> {
        self.0.clone()
    }

    fn head(&self) -> Option<SCellBox<SBTreeNode>> {
        self.tree().get_cloned().head
    }

    fn promote_node(&mut self, node: &mut SBTreeNode) -> Result<SCellBox<SBTreeNode>, OutOfMemory> {
        let mut parent_box = if let Some(parent) = &node.parent {
            parent.clone()
        } else {
            let parent = SCellBox::new(&SBTreeNode::new(None))?;
            node.parent = Some(parent.clone());

            parent
        };

        let mut parent = parent_box.get_cloned();

        // insert mid
        let (mid_k, mid_v) = node.elems.remove(node.elems.len() / 2);
        let idx = parent.binary_search(&mid_k).unwrap_err();
        parent.elems.insert(idx, (mid_k, mid_v));

        // insert right (left should already point to the correct branch)
        let right = SCellBox::new(&node.split())?;
        parent.children.insert(idx, right);

        parent_box.set(&parent)?;

        Ok(parent_box)
    }

    fn to_sorted_vec(&self) -> Vec<(K, V)> {
        let head = if let Some(h) = self.head() {
            h.get_cloned()
        } else {
            return Vec::new();
        };

        head.to_sorted_vec()
            .into_iter()
            .map(|(k, v)| unsafe { (K::from_bytes(&k), V::from_bytes(&v)) })
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
            head.get_cloned()
        } else {
            return;
        };

        _assert_obeys_rules(&node, map.order());
    }

    fn _assert_obeys_rules(node: &SBTreeNode, order: u32) {
        assert!(!node.elems.is_empty() && node.elems.len() as u32 <= order);
        assert!(node.children.len() == node.elems.len() + 1 || node.children.is_empty());

        for child in &node.children {
            _assert_obeys_rules(&child.get_cloned(), order);
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

        let mut map = SBTreeMap::with_order(2).unwrap();
        assert_obeys_rules(&map);

        let mut control = Vec::new();

        for elem in elements {
            control.push(elem);
            control.sort();

            map.insert(elem, ()).unwrap();
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
