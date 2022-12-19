use crate::collections::b_plus_tree_map::internal_node::InternalBTreeNode;
use crate::collections::b_plus_tree_map::leaf_node::LeafBTreeNode;
use crate::primitive::StableAllocated;
use crate::utils::encoding::AsFixedSizeBytes;
use crate::SSlice;
use std::fmt::Debug;

pub const B: usize = 6;
pub const CAPACITY: usize = 2 * B - 1;
pub const MIN_LEN_AFTER_SPLIT: usize = B - 1;

pub const CHILDREN_CAPACITY: usize = 2 * B;
pub const CHILDREN_MIN_LEN_AFTER_SPLIT: usize = B;

pub const NODE_TYPE_INTERNAL: u8 = 127;
pub const NODE_TYPE_LEAF: u8 = 255;
pub const NODE_TYPE_OFFSET: usize = 0;

mod internal_node;
mod iter;
mod leaf_node;

// LEFT CHILD - LESS THAN
// RIGHT CHILD - MORE OR EQUAL THAN
pub struct SBTreeMap<K, V> {
    pub(crate) root: Option<BTreeNode<K, V>>,
    len: u64,
    _stack: Vec<(InternalBTreeNode<K>, usize, usize)>,
}

impl<K: StableAllocated + Ord + Eq, V: StableAllocated> SBTreeMap<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    pub fn new() -> Self {
        Self {
            root: None,
            len: 0,
            _stack: Vec::default(),
        }
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let mut node = self.get_or_create_root();

        let mut leaf = loop {
            match &node {
                BTreeNode::Internal(internal_node) => {
                    node = self.stacked_lookup_internal(unsafe { internal_node.copy() }, &key);
                }
                BTreeNode::Leaf(leaf_node) => break unsafe { leaf_node.copy() },
            }
        };

        let right_leaf = match Self::insert_leaf(&mut leaf, key, value) {
            Ok(v) => {
                self._stack.clear();
                return Some(v);
            }
            Err(right_leaf_opt) => {
                if let Some(right_leaf) = right_leaf_opt {
                    right_leaf
                } else {
                    self._stack.clear();
                    self.len += 1;

                    return None;
                }
            }
        };

        let mut key_to_index = right_leaf.read_key(0);
        let mut ptr = right_leaf.as_ptr();

        while let Some((mut parent, parent_len, idx)) = self._stack.pop() {
            if let Some((right, _k)) =
                Self::insert_internal(&mut parent, parent_len, idx, key_to_index, ptr)
            {
                key_to_index = _k;
                ptr = right.as_ptr();
                node = BTreeNode::Internal(parent);
            } else {
                self.len += 1;

                return None;
            }
        }

        let new_root = InternalBTreeNode::<K>::create(
            &key_to_index,
            &node.as_ptr().as_fixed_size_bytes(),
            &ptr.as_fixed_size_bytes(),
        );
        self.root = Some(BTreeNode::Internal(new_root));
        self.len += 1;

        None
    }

    fn stacked_lookup_internal(&mut self, node: InternalBTreeNode<K>, key: &K) -> BTreeNode<K, V> {
        let node_len = node.read_len();
        let child_idx = match node.binary_search(key, node_len) {
            Ok(idx) => idx + 1,
            Err(idx) => idx,
        };

        let child_ptr = node.read_child_ptr(child_idx);
        self._stack.push((node, node_len, child_idx));

        BTreeNode::<K, V>::from_ptr(u64::from_fixed_size_bytes(&child_ptr))
    }

    fn insert_leaf(
        leaf_node: &mut LeafBTreeNode<K, V>,
        key: K,
        value: V,
    ) -> Result<V, Option<LeafBTreeNode<K, V>>> {
        let leaf_node_len = leaf_node.read_len();
        let insert_idx = match leaf_node.binary_search(&key, leaf_node_len) {
            Ok(existing_idx) => {
                // if there is already a key like that, return early
                let prev_value = V::from_fixed_size_bytes(&leaf_node.read_value(existing_idx));
                leaf_node.write_value(existing_idx, &value.as_fixed_size_bytes());

                return Ok(prev_value);
            }
            Err(idx) => idx,
        };

        // if there is enough space - simply insert and return early
        if leaf_node_len < CAPACITY {
            leaf_node.insert_key(insert_idx, &key.as_fixed_size_bytes(), leaf_node_len);
            leaf_node.insert_value(insert_idx, &value.as_fixed_size_bytes(), leaf_node_len);

            leaf_node.write_len(leaf_node_len + 1);
            return Err(None);
        }

        // split the leaf and insert so both leaves now have length of B
        let mut right = if insert_idx < B {
            let right = leaf_node.split_max_len(true);
            leaf_node.insert_key(insert_idx, &key.as_fixed_size_bytes(), MIN_LEN_AFTER_SPLIT);
            leaf_node.insert_value(
                insert_idx,
                &value.as_fixed_size_bytes(),
                MIN_LEN_AFTER_SPLIT,
            );

            right
        } else {
            let mut right = leaf_node.split_max_len(false);
            right.insert_key(
                insert_idx - B,
                &key.as_fixed_size_bytes(),
                MIN_LEN_AFTER_SPLIT,
            );
            right.insert_value(
                insert_idx - B,
                &value.as_fixed_size_bytes(),
                MIN_LEN_AFTER_SPLIT,
            );

            right
        };

        leaf_node.write_len(B);
        right.write_len(B);

        Err(Some(right))
    }

    fn insert_internal(
        internal_node: &mut InternalBTreeNode<K>,
        len: usize,
        idx: usize,
        key: [u8; K::SIZE],
        child_ptr: u64,
    ) -> Option<(InternalBTreeNode<K>, [u8; K::SIZE])> {
        if len < CAPACITY {
            internal_node.insert_key(idx, &key, len);
            internal_node.insert_child_ptr(idx + 1, &child_ptr.as_fixed_size_bytes(), len + 1);

            internal_node.write_len(len + 1);
            return None;
        }

        // TODO: possible to optimize when idx == MIN_LEN_AFTER_SPLIT
        let (mut right, mid) = internal_node.split_max_len();

        println!("{}", idx);

        if idx <= MIN_LEN_AFTER_SPLIT {
            internal_node.insert_key(idx, &key, MIN_LEN_AFTER_SPLIT);
            internal_node.insert_child_ptr(idx + 1, &child_ptr.as_fixed_size_bytes(), B);

            internal_node.write_len(B);
            right.write_len(MIN_LEN_AFTER_SPLIT);
        } else {
            right.insert_key(idx - B, &key, MIN_LEN_AFTER_SPLIT);
            right.insert_child_ptr(idx - B + 1, &child_ptr.as_fixed_size_bytes(), B);

            internal_node.write_len(MIN_LEN_AFTER_SPLIT);
            right.write_len(B);
        }

        Some((right, mid))
    }

    fn get_or_create_root(&mut self) -> BTreeNode<K, V> {
        match &self.root {
            Some(r) => unsafe { r.copy() },
            None => {
                self.root = Some(BTreeNode::<K, V>::Leaf(LeafBTreeNode::create()));
                unsafe { self.root.as_ref().unwrap_unchecked().copy() }
            }
        }
    }
}

impl<K: StableAllocated + Ord + Eq + Debug, V: StableAllocated + Debug> SBTreeMap<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    pub fn debug_print(&self) {
        if self.len == 0 {
            print!("EMPTY");
            return;
        }

        let mut level = Vec::new();
        level.push(unsafe { self.root.as_ref().unwrap_unchecked().copy() });

        loop {
            Self::print_level(&level);
            println!();

            let mut new_level = Vec::new();
            for node in level {
                if let BTreeNode::Internal(internal) = node {
                    let c_len = internal.read_len() + 1;
                    for i in 0..c_len {
                        let c = BTreeNode::<K, V>::from_ptr(u64::from_fixed_size_bytes(
                            &internal.read_child_ptr(i),
                        ));
                        new_level.push(c);
                    }
                }
            }

            if new_level.is_empty() {
                break;
            } else {
                level = new_level;
            }
        }
    }

    fn print_level(level: &Vec<BTreeNode<K, V>>) {
        for node in level {
            match node {
                BTreeNode::Internal(i) => i.debug_print(),
                BTreeNode::Leaf(l) => l.debug_print(),
            }
        }
    }
}

impl<K: StableAllocated + Ord + Eq, V: StableAllocated> Default for SBTreeMap<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    fn default() -> Self {
        Self::new()
    }
}

enum BTreeNode<K, V> {
    Internal(InternalBTreeNode<K>),
    Leaf(LeafBTreeNode<K, V>),
}

impl<K: StableAllocated + Ord + Eq, V: StableAllocated> BTreeNode<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    fn from_ptr(ptr: u64) -> Self {
        let node_type: u8 = SSlice::_as_fixed_size_bytes_read(ptr, NODE_TYPE_OFFSET);

        unsafe {
            match node_type {
                NODE_TYPE_INTERNAL => Self::Internal(InternalBTreeNode::<K>::from_ptr(ptr)),
                NODE_TYPE_LEAF => Self::Leaf(LeafBTreeNode::<K, V>::from_ptr(ptr)),
                _ => unreachable!(),
            }
        }
    }

    fn as_ptr(&self) -> u64 {
        match &self {
            Self::Internal(i) => i.as_ptr(),
            Self::Leaf(l) => l.as_ptr(),
        }
    }

    unsafe fn copy(&self) -> Self {
        match &self {
            Self::Internal(i) => Self::Internal(i.copy()),
            Self::Leaf(l) => Self::Leaf(l.copy()),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::b_plus_tree_map::SBTreeMap;
    use crate::{init_allocator, stable};
    use rand::seq::SliceRandom;
    use rand::thread_rng;

    #[test]
    fn insert_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut example = Vec::new();
        for i in 0..300 {
            example.push(i);
        }
        example.shuffle(&mut thread_rng());

        let mut map = SBTreeMap::<u64, u64>::default();

        for i in example {
            println!("inserting {}", i);
            map.insert(i, i);

            map.debug_print();
            println!();
            println!();
        }

        map.debug_print();
    }
}
