use crate::collections::btree_map::internal_node::InternalBTreeNode;
use crate::collections::btree_map::iter::SBTreeMapIter;
use crate::collections::btree_map::leaf_node::LeafBTreeNode;
use crate::primitive::StableAllocated;
use crate::utils::encoding::{AsFixedSizeBytes, FixedSize};
use crate::SSlice;
use std::fmt::Debug;

pub const B: usize = 8;
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
    root: Option<BTreeNode<K, V>>,
    len: u64,
    _stack: Vec<(InternalBTreeNode<K>, usize, usize)>,
    _buf: Vec<u8>,
}

impl<K: StableAllocated + Ord + Eq, V: StableAllocated> SBTreeMap<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    #[inline]
    pub fn new() -> Self {
        Self {
            root: None,
            len: 0,
            _stack: Vec::default(),
            _buf: Vec::default(),
        }
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        let mut node = self.get_or_create_root();
        let mut found_internal_node = None;

        // lookup for the leaf that may contain the key
        let mut leaf = loop {
            match node {
                BTreeNode::Internal(internal_node) => {
                    let node_len = internal_node.read_len();
                    let child_idx = match internal_node.binary_search(key, node_len) {
                        Ok(idx) => {
                            found_internal_node = Some((unsafe { internal_node.copy() }, idx));

                            idx + 1
                        }
                        Err(idx) => idx,
                    };

                    let child_ptr = internal_node.read_child_ptr(child_idx);
                    self._stack.push((internal_node, node_len, child_idx));

                    node = BTreeNode::<K, V>::from_ptr(u64::from_fixed_size_bytes(&child_ptr));
                }
                BTreeNode::Leaf(leaf_node) => break unsafe { leaf_node.copy() },
            }
        };

        let leaf_len = leaf.read_len();
        let idx = leaf.binary_search(key, leaf_len).ok()?;

        self.len -= 1;

        // if possible to simply remove the key without violating - return early
        if leaf_len > MIN_LEN_AFTER_SPLIT {
            self._stack.clear();

            let v = leaf.remove_by_idx(idx, leaf_len, &mut self._buf);
            leaf.write_len(leaf_len - 1);

            if let Some((mut fin, i)) = found_internal_node {
                fin.write_key(i, &leaf.read_key(0));
            }

            return Some(v);
        };

        let stack_top_frame = self.peek_stack();

        // if the only node in the tree is the root - return early
        if stack_top_frame.is_none() {
            let v = leaf.remove_by_idx(idx, leaf_len, &mut self._buf);
            leaf.write_len(leaf_len - 1);

            return Some(v);
        }

        let (mut parent, parent_len, parent_idx) = unsafe { stack_top_frame.unwrap_unchecked() };

        // try to steal an element from the left sibling
        let has_left_sibling = parent_idx > 0;
        if has_left_sibling {
            let left_sibling_ptr =
                u64::from_fixed_size_bytes(&parent.read_child_ptr(parent_idx - 1));
            let mut left_sibling = unsafe { LeafBTreeNode::<K, V>::from_ptr(left_sibling_ptr) };
            let left_sibling_len = left_sibling.read_len();

            // if possible to steal - return early
            if left_sibling_len > MIN_LEN_AFTER_SPLIT {
                leaf.steal_from_left(
                    MIN_LEN_AFTER_SPLIT,
                    &mut left_sibling,
                    left_sibling_len,
                    &mut parent,
                    parent_idx - 1,
                    None,
                    &mut self._buf,
                );

                left_sibling.write_len(left_sibling_len - 1);

                // idx + 1, because after rotation leaf has one more key added before
                let v = leaf.remove_by_idx(idx + 1, B, &mut self._buf);

                if let Some((mut fin, i)) = found_internal_node {
                    fin.write_key(i, &leaf.read_key(0));
                }

                self._stack.clear();

                return Some(v);
            }

            // also try to do the same thing for right sibling if possible
            let has_right_sibling = parent_idx < parent_len;
            if has_right_sibling {
                let right_sibling_ptr =
                    u64::from_fixed_size_bytes(&parent.read_child_ptr(parent_idx + 1));
                let mut right_sibling =
                    unsafe { LeafBTreeNode::<K, V>::from_ptr(right_sibling_ptr) };
                let right_sibling_len = right_sibling.read_len();

                // if possible to steal - return early
                if right_sibling_len > MIN_LEN_AFTER_SPLIT {
                    leaf.steal_from_right(
                        MIN_LEN_AFTER_SPLIT,
                        &mut right_sibling,
                        right_sibling_len,
                        &mut parent,
                        parent_idx,
                        None,
                        &mut self._buf,
                    );

                    right_sibling.write_len(right_sibling_len - 1);

                    // just idx, because after rotation leaf has one more key added to the end
                    let v = leaf.remove_by_idx(idx, B, &mut self._buf);

                    if let Some((mut fin, i)) = found_internal_node {
                        fin.write_key(i, &leaf.read_key(0));
                    }

                    self._stack.clear();

                    return Some(v);
                }

                // otherwise merge with right
                leaf.merge_min_len(right_sibling, &mut self._buf);
                // just idx, because leaf keys stay unchanged
                let v = leaf.remove_by_idx(idx, CAPACITY - 1, &mut self._buf);
                leaf.write_len(CAPACITY - 2);

                if let Some((mut fin, i)) = found_internal_node {
                    fin.write_key(i, &leaf.read_key(0));
                }

                self.handle_stack_after_merge(true, leaf);

                return Some(v);
            }

            // if there is no right sibling - merge with left
            left_sibling.merge_min_len(leaf, &mut self._buf);
            // idx + MIN_LEN_AFTER_SPLIT, because all keys of leaf are added to the
            // end of left_sibling
            let v =
                left_sibling.remove_by_idx(idx + MIN_LEN_AFTER_SPLIT, CAPACITY - 1, &mut self._buf);
            left_sibling.write_len(CAPACITY - 2);

            // no reason to handle 'found_internal_node', because the key is
            // guaranteed to be in the nearest parent and left_sibling keys are all
            // continue to present

            self.handle_stack_after_merge(false, left_sibling);

            return Some(v);
        }

        // if there is no left sibling - repeat all the steps for the right one
        // parent_idx is 0
        let right_sibling_ptr = u64::from_fixed_size_bytes(&parent.read_child_ptr(1));
        let mut right_sibling = unsafe { LeafBTreeNode::<K, V>::from_ptr(right_sibling_ptr) };
        let right_sibling_len = right_sibling.read_len();

        // if possible to steal - return early
        if right_sibling_len > MIN_LEN_AFTER_SPLIT {
            leaf.steal_from_right(
                MIN_LEN_AFTER_SPLIT,
                &mut right_sibling,
                right_sibling_len,
                &mut parent,
                0,
                None,
                &mut self._buf,
            );

            right_sibling.write_len(right_sibling_len - 1);

            // just idx, because after the rotation the leaf has one more key added to the end
            let v = leaf.remove_by_idx(idx, B, &mut self._buf);

            if let Some((mut fin, i)) = found_internal_node {
                fin.write_key(i, &leaf.read_key(0));
            }

            self._stack.clear();

            return Some(v);
        }

        // otherwise merge with right
        leaf.merge_min_len(right_sibling, &mut self._buf);

        // just idx, because leaf keys stay unchanged
        let v = leaf.remove_by_idx(idx, CAPACITY - 1, &mut self._buf);
        leaf.write_len(CAPACITY - 2);

        if let Some((mut fin, i)) = found_internal_node {
            fin.write_key(i, &leaf.read_key(0));
        }

        self.handle_stack_after_merge(true, leaf);

        Some(v)
    }

    fn handle_stack_after_merge(&mut self, mut merged_right: bool, leaf: LeafBTreeNode<K, V>) {
        let mut prev_node = BTreeNode::Leaf(leaf);

        while let Some((mut node, node_len, remove_idx)) = self._stack.pop() {
            let (idx_to_remove, child_idx_to_remove) = if merged_right {
                (remove_idx, remove_idx + 1)
            } else {
                (remove_idx - 1, remove_idx)
            };

            // if the node has enough keys, return early
            if node_len > MIN_LEN_AFTER_SPLIT {
                node.remove_key(idx_to_remove, node_len, &mut self._buf);
                node.remove_child_ptr(child_idx_to_remove, node_len + 1, &mut self._buf);
                node.write_len(node_len - 1);

                self._stack.clear();

                return;
            }

            let stack_top_frame = self.peek_stack();

            // if there is no parent, return early
            if stack_top_frame.is_none() {
                // if the root has only one key, make child the new root
                if node_len == 1 {
                    node.destroy();
                    self.root = Some(prev_node);

                    return;
                }

                // otherwise simply remove and return
                node.remove_key(idx_to_remove, node_len, &mut self._buf);
                node.remove_child_ptr(child_idx_to_remove, node_len + 1, &mut self._buf);
                node.write_len(node_len - 1);

                return;
            }

            let (mut parent, parent_len, parent_idx) =
                unsafe { stack_top_frame.unwrap_unchecked() };

            let has_left_sibling = parent_idx > 0;
            if has_left_sibling {
                let left_sibling_ptr =
                    u64::from_fixed_size_bytes(&parent.read_child_ptr(parent_idx - 1));
                let mut left_sibling =
                    unsafe { InternalBTreeNode::<K>::from_ptr(left_sibling_ptr) };
                let left_sibling_len = left_sibling.read_len();

                // steal from left if it is possible
                if left_sibling_len > MIN_LEN_AFTER_SPLIT {
                    node.steal_from_left(
                        node_len,
                        &mut left_sibling,
                        left_sibling_len,
                        &mut parent,
                        parent_idx - 1,
                        None,
                        &mut self._buf,
                    );
                    left_sibling.write_len(left_sibling_len - 1);
                    node.remove_key(idx_to_remove + 1, B, &mut self._buf);
                    node.remove_child_ptr(child_idx_to_remove + 1, B + 1, &mut self._buf);

                    self._stack.clear();

                    return;
                }

                let has_right_sibling = parent_idx < parent_len;
                if has_right_sibling {
                    let right_sibling_ptr =
                        u64::from_fixed_size_bytes(&parent.read_child_ptr(parent_idx + 1));
                    let mut right_sibling =
                        unsafe { InternalBTreeNode::<K>::from_ptr(right_sibling_ptr) };
                    let right_sibling_len = right_sibling.read_len();

                    // steal from right if it's possible
                    if right_sibling_len > MIN_LEN_AFTER_SPLIT {
                        node.steal_from_right(
                            node_len,
                            &mut right_sibling,
                            right_sibling_len,
                            &mut parent,
                            parent_idx,
                            None,
                            &mut self._buf,
                        );
                        right_sibling.write_len(right_sibling_len - 1);
                        node.remove_key(idx_to_remove, B, &mut self._buf);
                        node.remove_child_ptr(child_idx_to_remove, B + 1, &mut self._buf);

                        self._stack.clear();

                        return;
                    }

                    // otherwise merge with right
                    let mid_element = parent.read_key(parent_idx);
                    node.merge_min_len(&mid_element, right_sibling, &mut self._buf);
                    node.remove_key(idx_to_remove, CAPACITY, &mut self._buf);
                    node.remove_child_ptr(child_idx_to_remove, CHILDREN_CAPACITY, &mut self._buf);
                    node.write_len(CAPACITY - 1);

                    merged_right = true;
                    prev_node = BTreeNode::Internal(node);

                    continue;
                }

                // otherwise merge with left
                let mid_element = parent.read_key(parent_idx - 1);
                left_sibling.merge_min_len(&mid_element, node, &mut self._buf);
                left_sibling.remove_key(idx_to_remove + B, CAPACITY, &mut self._buf);
                left_sibling.remove_child_ptr(
                    child_idx_to_remove + B,
                    CHILDREN_CAPACITY,
                    &mut self._buf,
                );
                left_sibling.write_len(CAPACITY - 1);

                merged_right = false;
                prev_node = BTreeNode::Internal(left_sibling);

                continue;
            }

            // otherwise merge with right
            // parent_idx == 0
            let right_sibling_ptr = u64::from_fixed_size_bytes(&parent.read_child_ptr(1));
            let mut right_sibling = unsafe { InternalBTreeNode::<K>::from_ptr(right_sibling_ptr) };
            let right_sibling_len = right_sibling.read_len();

            // steal from right if it's possible
            if right_sibling_len > MIN_LEN_AFTER_SPLIT {
                node.steal_from_right(
                    node_len,
                    &mut right_sibling,
                    right_sibling_len,
                    &mut parent,
                    0,
                    None,
                    &mut self._buf,
                );
                right_sibling.write_len(right_sibling_len - 1);
                node.remove_key(idx_to_remove, B, &mut self._buf);
                node.remove_child_ptr(child_idx_to_remove, B + 1, &mut self._buf);

                self._stack.clear();

                return;
            }

            // otherwise merge with right
            let mid_element = parent.read_key(parent_idx);
            node.merge_min_len(&mid_element, right_sibling, &mut self._buf);
            node.remove_key(idx_to_remove, CAPACITY, &mut self._buf);
            node.remove_child_ptr(child_idx_to_remove, CHILDREN_CAPACITY, &mut self._buf);
            node.write_len(CAPACITY - 1);

            merged_right = true;
            prev_node = BTreeNode::Internal(node);
        }
    }

    fn peek_stack(&self) -> Option<(InternalBTreeNode<K>, usize, usize)> {
        self._stack
            .last()
            .map(|(n, l, i)| (unsafe { n.copy() }, *l, *i))
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let mut node = self.get_or_create_root();

        let mut leaf = loop {
            match unsafe { node.copy() } {
                BTreeNode::Internal(internal_node) => {
                    let node_len = internal_node.read_len();
                    let child_idx = match internal_node.binary_search(&key, node_len) {
                        Ok(idx) => idx + 1,
                        Err(idx) => idx,
                    };

                    let child_ptr = internal_node.read_child_ptr(child_idx);
                    self._stack.push((internal_node, node_len, child_idx));

                    node = BTreeNode::<K, V>::from_ptr(u64::from_fixed_size_bytes(&child_ptr));
                }
                BTreeNode::Leaf(leaf_node) => break unsafe { leaf_node.copy() },
            }
        };

        let right_leaf = match self.insert_leaf(&mut leaf, key, value) {
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
            if let Some((right, _k)) = self.insert_internal(
                &mut parent,
                parent_len,
                idx,
                key_to_index,
                ptr.as_fixed_size_bytes(),
            ) {
                key_to_index = _k;
                ptr = right.as_ptr();
                node = BTreeNode::Internal(parent);
            } else {
                self.len += 1;
                self._stack.clear();

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

    #[inline]
    pub fn get_copy(&self, key: &K) -> Option<V> {
        let (leaf_node, idx) = self.lookup(key, false)?;
        let v = V::from_fixed_size_bytes(&leaf_node.read_value(idx));

        Some(v)
    }

    #[inline]
    pub fn contains_key(&self, key: &K) -> bool {
        self.lookup(key, true).is_some()
    }

    #[inline]
    pub fn iter(&self) -> SBTreeMapIter<K, V> {
        SBTreeMapIter::<K, V>::new(self)
    }

    // WARNING: return_early == true will return nonsense leaf node and idx
    fn lookup(&self, key: &K, return_early: bool) -> Option<(LeafBTreeNode<K, V>, usize)> {
        let mut node = unsafe { self.root.as_ref()?.copy() };
        loop {
            match node {
                BTreeNode::Internal(internal_node) => {
                    let child_idx = match internal_node.binary_search(key, internal_node.read_len())
                    {
                        Ok(idx) => {
                            if return_early {
                                return unsafe { Some((LeafBTreeNode::from_ptr(0), 0)) };
                            } else {
                                idx + 1
                            }
                        }
                        Err(idx) => idx,
                    };

                    let child_ptr =
                        u64::from_fixed_size_bytes(&internal_node.read_child_ptr(child_idx));
                    node = BTreeNode::from_ptr(child_ptr);
                }
                BTreeNode::Leaf(leaf_node) => {
                    return match leaf_node.binary_search(key, leaf_node.read_len()) {
                        Ok(idx) => Some((leaf_node, idx)),
                        _ => None,
                    }
                }
            }
        }
    }

    fn insert_leaf(
        &mut self,
        leaf_node: &mut LeafBTreeNode<K, V>,
        mut key: K,
        mut value: V,
    ) -> Result<V, Option<LeafBTreeNode<K, V>>> {
        let leaf_node_len = leaf_node.read_len();
        let insert_idx = match leaf_node.binary_search(&key, leaf_node_len) {
            Ok(existing_idx) => {
                // if there is already a key like that, return early
                let mut prev_value = V::from_fixed_size_bytes(&leaf_node.read_value(existing_idx));
                prev_value.remove_from_stable();
                value.move_to_stable();

                leaf_node.write_value(existing_idx, &value.as_fixed_size_bytes());

                return Ok(prev_value);
            }
            Err(idx) => idx,
        };

        key.move_to_stable();
        let k = key.as_fixed_size_bytes();

        value.move_to_stable();
        let v = value.as_fixed_size_bytes();

        // if there is enough space - simply insert and return early
        if leaf_node_len < CAPACITY {
            leaf_node.insert_key(insert_idx, &k, leaf_node_len, &mut self._buf);
            leaf_node.insert_value(insert_idx, &v, leaf_node_len, &mut self._buf);

            leaf_node.write_len(leaf_node_len + 1);
            return Err(None);
        }

        // try passing an element to a neighbor, to make room for a new one
        if self.pass_elem_to_sibling_leaf(leaf_node, &k, &v, insert_idx) {
            return Err(None);
        }

        // split the leaf and insert so both leaves now have length of B
        let mut right = if insert_idx < B {
            let right = leaf_node.split_max_len(true, &mut self._buf);
            leaf_node.insert_key(insert_idx, &k, MIN_LEN_AFTER_SPLIT, &mut self._buf);
            leaf_node.insert_value(insert_idx, &v, MIN_LEN_AFTER_SPLIT, &mut self._buf);

            right
        } else {
            let mut right = leaf_node.split_max_len(false, &mut self._buf);
            right.insert_key(insert_idx - B, &k, MIN_LEN_AFTER_SPLIT, &mut self._buf);
            right.insert_value(insert_idx - B, &v, MIN_LEN_AFTER_SPLIT, &mut self._buf);

            right
        };

        leaf_node.write_len(B);
        right.write_len(B);

        Err(Some(right))
    }

    fn insert_internal(
        &mut self,
        internal_node: &mut InternalBTreeNode<K>,
        len: usize,
        idx: usize,
        key: [u8; K::SIZE],
        child_ptr: [u8; u64::SIZE],
    ) -> Option<(InternalBTreeNode<K>, [u8; K::SIZE])> {
        if len < CAPACITY {
            internal_node.insert_key(idx, &key, len, &mut self._buf);
            internal_node.insert_child_ptr(idx + 1, &child_ptr, len + 1, &mut self._buf);

            internal_node.write_len(len + 1);
            return None;
        }

        if self.pass_elem_to_sibling_internal(internal_node, idx, &key, &child_ptr) {
            return None;
        }

        // TODO: possible to optimize when idx == MIN_LEN_AFTER_SPLIT
        let (mut right, mid) = internal_node.split_max_len(&mut self._buf);

        if idx <= MIN_LEN_AFTER_SPLIT {
            internal_node.insert_key(idx, &key, MIN_LEN_AFTER_SPLIT, &mut self._buf);
            internal_node.insert_child_ptr(idx + 1, &child_ptr, B, &mut self._buf);

            internal_node.write_len(B);
            right.write_len(MIN_LEN_AFTER_SPLIT);
        } else {
            right.insert_key(idx - B, &key, MIN_LEN_AFTER_SPLIT, &mut self._buf);
            right.insert_child_ptr(idx - B + 1, &child_ptr, B, &mut self._buf);

            internal_node.write_len(MIN_LEN_AFTER_SPLIT);
            right.write_len(B);
        }

        Some((right, mid))
    }

    fn pass_elem_to_sibling_leaf(
        &mut self,
        leaf_node: &mut LeafBTreeNode<K, V>,
        key: &[u8; K::SIZE],
        value: &[u8; V::SIZE],
        insert_idx: usize,
    ) -> bool {
        let stack_top_frame = self.peek_stack();
        if stack_top_frame.is_none() {
            return false;
        }

        let (mut parent, parent_len, parent_idx) = unsafe { stack_top_frame.unwrap_unchecked() };

        let has_left_sibling = parent_idx > 0;
        if !has_left_sibling {
            let has_right_sibling = parent_idx < parent_len;

            if !has_right_sibling {
                return false;
            }

            let right_sibling_ptr =
                u64::from_fixed_size_bytes(&parent.read_child_ptr(parent_idx + 1));
            let mut right_sibling = unsafe { LeafBTreeNode::<K, V>::from_ptr(right_sibling_ptr) };
            let right_sibling_len = right_sibling.read_len();

            if right_sibling_len == CAPACITY {
                return false;
            }

            self.pass_to_right_sibling_leaf(
                &mut parent,
                parent_idx,
                leaf_node,
                &mut right_sibling,
                right_sibling_len,
                insert_idx,
                key,
                value,
            );

            return true;
        }

        let left_sibling_ptr = u64::from_fixed_size_bytes(&parent.read_child_ptr(parent_idx - 1));
        let mut left_sibling = unsafe { LeafBTreeNode::<K, V>::from_ptr(left_sibling_ptr) };
        let left_sibling_len = left_sibling.read_len();

        // if it is possible to pass to the left sibling - do that
        if left_sibling_len < CAPACITY {
            self.pass_to_left_sibling_leaf(
                &mut parent,
                parent_idx,
                leaf_node,
                &mut left_sibling,
                left_sibling_len,
                insert_idx,
                key,
                value,
            );

            return true;
        }

        let has_right_sibling = parent_idx < parent_len;
        if !has_right_sibling {
            return false;
        }

        let right_sibling_ptr = u64::from_fixed_size_bytes(&parent.read_child_ptr(parent_idx + 1));
        let mut right_sibling = unsafe { LeafBTreeNode::<K, V>::from_ptr(right_sibling_ptr) };
        let right_sibling_len = right_sibling.read_len();

        if right_sibling_len == CAPACITY {
            return false;
        }

        self.pass_to_right_sibling_leaf(
            &mut parent,
            parent_idx,
            leaf_node,
            &mut right_sibling,
            right_sibling_len,
            insert_idx,
            key,
            value,
        );

        true
    }

    fn pass_to_right_sibling_leaf(
        &mut self,
        p: &mut InternalBTreeNode<K>,
        p_idx: usize,
        leaf: &mut LeafBTreeNode<K, V>,
        rs: &mut LeafBTreeNode<K, V>,
        rs_len: usize,
        i_idx: usize,
        key: &[u8; K::SIZE],
        value: &[u8; V::SIZE],
    ) {
        if i_idx != CAPACITY {
            rs.steal_from_left(rs_len, leaf, CAPACITY, p, p_idx, None, &mut self._buf);

            leaf.insert_key(i_idx, key, CAPACITY - 1, &mut self._buf);
            leaf.insert_value(i_idx, value, CAPACITY - 1, &mut self._buf);

            rs.write_len(rs_len + 1);
            return;
        }

        let last = Some((key, value));
        rs.steal_from_left(rs_len, leaf, CAPACITY, p, p_idx, last, &mut self._buf);
        rs.write_len(rs_len + 1);
    }

    fn pass_to_left_sibling_leaf(
        &mut self,
        p: &mut InternalBTreeNode<K>,
        p_idx: usize,
        leaf: &mut LeafBTreeNode<K, V>,
        ls: &mut LeafBTreeNode<K, V>,
        ls_len: usize,
        i_idx: usize,
        key: &[u8; K::SIZE],
        value: &[u8; V::SIZE],
    ) {
        if i_idx != 1 {
            ls.steal_from_right(ls_len, leaf, CAPACITY, p, p_idx - 1, None, &mut self._buf);

            leaf.insert_key(i_idx - 1, key, CAPACITY - 1, &mut self._buf);
            leaf.insert_value(i_idx - 1, value, CAPACITY - 1, &mut self._buf);

            ls.write_len(ls_len + 1);
            return;
        };

        let first = Some((key, value));
        ls.steal_from_right(ls_len, leaf, CAPACITY, p, p_idx - 1, first, &mut self._buf);
        ls.write_len(ls_len + 1);
    }

    fn pass_elem_to_sibling_internal(
        &mut self,
        internal_node: &mut InternalBTreeNode<K>,
        idx: usize,
        key: &[u8; K::SIZE],
        child_ptr: &[u8; u64::SIZE],
    ) -> bool {
        let stack_top_frame = self.peek_stack();
        if stack_top_frame.is_none() {
            return false;
        }

        let (mut parent, parent_len, parent_idx) = unsafe { stack_top_frame.unwrap_unchecked() };

        let has_left_sibling = parent_idx > 0;
        if !has_left_sibling {
            let has_right_sibling = parent_idx < parent_len;

            if !has_right_sibling {
                return false;
            }

            let right_sibling_ptr =
                u64::from_fixed_size_bytes(&parent.read_child_ptr(parent_idx + 1));
            let mut right_sibling = unsafe { InternalBTreeNode::<K>::from_ptr(right_sibling_ptr) };
            let right_sibling_len = right_sibling.read_len();

            if right_sibling_len == CAPACITY {
                return false;
            }

            self.pass_to_right_sibling_internal(
                &mut parent,
                parent_idx,
                internal_node,
                &mut right_sibling,
                right_sibling_len,
                idx,
                key,
                child_ptr,
            );

            return true;
        }

        let left_sibling_ptr = u64::from_fixed_size_bytes(&parent.read_child_ptr(parent_idx - 1));
        let mut left_sibling = unsafe { InternalBTreeNode::<K>::from_ptr(left_sibling_ptr) };
        let left_sibling_len = left_sibling.read_len();

        if left_sibling_len < CAPACITY {
            self.pass_to_left_sibling_internal(
                &mut parent,
                parent_idx,
                internal_node,
                &mut left_sibling,
                left_sibling_len,
                idx,
                key,
                child_ptr,
            );

            return true;
        }

        let has_right_sibling = parent_idx < parent_len;
        if !has_right_sibling {
            return false;
        }

        let right_sibling_ptr = u64::from_fixed_size_bytes(&parent.read_child_ptr(parent_idx + 1));
        let mut right_sibling = unsafe { InternalBTreeNode::<K>::from_ptr(right_sibling_ptr) };
        let right_sibling_len = right_sibling.read_len();

        if right_sibling_len == CAPACITY {
            return false;
        }

        self.pass_to_right_sibling_internal(
            &mut parent,
            parent_idx,
            internal_node,
            &mut right_sibling,
            right_sibling_len,
            idx,
            key,
            child_ptr,
        );

        true
    }

    fn pass_to_right_sibling_internal(
        &mut self,
        p: &mut InternalBTreeNode<K>,
        p_idx: usize,
        node: &mut InternalBTreeNode<K>,
        rs: &mut InternalBTreeNode<K>,
        rs_len: usize,
        i_idx: usize,
        key: &[u8; K::SIZE],
        child_ptr: &[u8; u64::SIZE],
    ) {
        if i_idx != CAPACITY {
            rs.steal_from_left(rs_len, node, CAPACITY, p, p_idx, None, &mut self._buf);

            node.insert_key(i_idx, key, CAPACITY - 1, &mut self._buf);
            node.insert_child_ptr(i_idx + 1, child_ptr, CAPACITY, &mut self._buf);

            rs.write_len(rs_len + 1);
            return;
        }

        let last = Some((key, child_ptr));
        rs.steal_from_left(rs_len, node, CAPACITY, p, p_idx, last, &mut self._buf);
        rs.write_len(rs_len + 1);
    }

    fn pass_to_left_sibling_internal(
        &mut self,
        p: &mut InternalBTreeNode<K>,
        p_idx: usize,
        node: &mut InternalBTreeNode<K>,
        ls: &mut InternalBTreeNode<K>,
        ls_len: usize,
        i_idx: usize,
        key: &[u8; K::SIZE],
        child_ptr: &[u8; u64::SIZE],
    ) {
        if i_idx != 0 {
            ls.steal_from_right(ls_len, node, CAPACITY, p, p_idx - 1, None, &mut self._buf);

            node.insert_key(i_idx - 1, key, CAPACITY - 1, &mut self._buf);
            node.insert_child_ptr(i_idx, child_ptr, CAPACITY, &mut self._buf);

            ls.write_len(ls_len + 1);
            return;
        }

        let first = Some((key, child_ptr));
        ls.steal_from_right(ls_len, node, CAPACITY, p, p_idx - 1, first, &mut self._buf);
        ls.write_len(ls_len + 1);
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
    pub fn debug_print_stack(&self) {
        println!(
            "STACK: {:?}",
            self._stack
                .iter()
                .map(|(p, l, i)| (p.as_ptr(), *l, *i))
                .collect::<Vec<_>>()
        );
    }

    pub fn debug_print(&self) {
        if self.len == 0 {
            println!("EMPTY");
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
    use crate::collections::btree_map::SBTreeMap;
    use crate::{init_allocator, stable};
    use rand::seq::SliceRandom;
    use rand::thread_rng;

    #[test]
    fn random_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let iterations = 1000;
        let mut map = SBTreeMap::<u64, u64>::default();

        let mut example = Vec::new();
        for i in 0..iterations {
            example.push(i as u64);
        }
        example.shuffle(&mut thread_rng());

        for i in 0..iterations {
            println!("inserting {}", example[i]);
            map.debug_print_stack();
            assert!(map._stack.is_empty());
            assert!(map.insert(example[i], example[i]).is_none());

            map.debug_print();
            println!();
            println!();

            for j in 0..i {
                assert!(
                    map.contains_key(&example[j]),
                    "don't contain {}",
                    example[j]
                );
                assert_eq!(
                    map.get_copy(&example[j]),
                    Some(example[j]),
                    "unable to get {}",
                    example[j]
                );
            }
        }

        example.shuffle(&mut thread_rng());
        for i in 0..iterations {
            println!("removing {}", example[i]);
            map.debug_print_stack();
            assert!(map._stack.is_empty());

            assert_eq!(map.remove(&example[i]), Some(example[i]));

            map.debug_print();
            println!();
            println!();

            for j in (i + 1)..iterations {
                assert!(
                    map.contains_key(&example[j]),
                    "don't contain {}",
                    example[j]
                );
                assert_eq!(
                    map.get_copy(&example[j]),
                    Some(example[j]),
                    "unable to get {}",
                    example[j]
                );
            }
        }

        map.debug_print();
    }

    #[test]
    fn iters_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SBTreeMap::<u64, u64>::default();

        for i in 0..200 {
            map.insert(i, i);
        }

        let mut i = 0u64;

        for (k, v) in map.iter() {
            assert_eq!(i, k);
            assert_eq!(i, v);

            i += 1;
        }

        assert_eq!(i, 199);

        for (k, v) in map.iter().rev() {
            println!("{}", i);
            assert_eq!(i, k);
            assert_eq!(i, v);

            i -= 1;
        }

        assert_eq!(i, 0);
    }
}
