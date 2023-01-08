use crate::collections::btree_map::internal_node::{InternalBTreeNode, PtrRaw};
use crate::collections::btree_map::iter::SBTreeMapIter;
use crate::collections::btree_map::leaf_node::LeafBTreeNode;
use crate::collections::btree_map::{
    BTreeNode, IBTreeNode, B, CAPACITY, CHILDREN_CAPACITY, MIN_LEN_AFTER_SPLIT,
};
use crate::isoprint;
use crate::mem::allocator::EMPTY_PTR;
use crate::primitive::StableAllocated;
use crate::utils::certification::{AsHashTree, AsHashableBytes, Hash, HashTree, EMPTY_HASH};
use crate::utils::encoding::{AsFixedSizeBytes, FixedSize};
use std::fmt::Debug;

// LEFT CHILD - LESS THAN
// RIGHT CHILD - MORE OR EQUAL THAN
pub struct SCertifiedBTreeMap<K, V> {
    root: Option<BTreeNode<K, V>>,
    len: u64,
    root_hash: Hash,
    _stack: Vec<(InternalBTreeNode<K>, usize, usize)>,
    _buf: Vec<u8>,
}

impl<K: StableAllocated + Ord + AsHashableBytes, V: StableAllocated + AsHashableBytes>
    SCertifiedBTreeMap<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    #[inline]
    pub fn new() -> Self {
        Self {
            root: None,
            len: 0,
            root_hash: EMPTY_HASH,
            _stack: Vec::default(),
            _buf: Vec::default(),
        }
    }

    fn recalculate_root_hash(&mut self, mut last_node_hash: Hash) {
        while let Some((mut parent, _, parent_idx)) = self._stack.pop() {
            parent.write_child_hash(parent_idx, &last_node_hash, true);
            last_node_hash = parent.root_hash();
        }
        self.root_hash = last_node_hash;
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
                self.recalculate_root_hash(leaf.root_hash());

                return Some(v);
            }
            Err(right_leaf_opt) => {
                if let Some(right_leaf) = right_leaf_opt {
                    right_leaf
                } else {
                    self.recalculate_root_hash(leaf.root_hash());
                    self.len += 1;

                    return None;
                }
            }
        };

        let mut key_to_index = right_leaf.read_key(0);
        let mut ptr = right_leaf.as_ptr();
        let mut left_hash = leaf.root_hash();
        let mut right_hash = right_leaf.root_hash();

        while let Some((mut parent, parent_len, idx)) = self._stack.pop() {
            parent.write_child_hash(idx, &left_hash, true);

            if let Some((right, _k)) = self.insert_internal(
                &mut parent,
                parent_len,
                idx,
                key_to_index,
                ptr.as_fixed_size_bytes(),
                right_hash,
            ) {
                key_to_index = _k;
                ptr = right.as_ptr();
                left_hash = parent.root_hash();
                right_hash = right.root_hash();
                node = BTreeNode::Internal(parent);
            } else {
                self.recalculate_root_hash(parent.root_hash());
                self.len += 1;

                return None;
            }
        }

        let new_root = InternalBTreeNode::<K>::create(
            &key_to_index,
            &node.as_ptr().as_fixed_size_bytes(),
            &ptr.as_fixed_size_bytes(),
            Some((&node.root_hash(), &right_hash)),
        );
        self.root_hash = new_root.root_hash();
        self.root = Some(BTreeNode::Internal(new_root));
        self.len += 1;

        None
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
            let v = leaf.remove_by_idx(idx, leaf_len, &mut self._buf);
            leaf.write_len(leaf_len - 1);

            if let Some((mut fin, i)) = found_internal_node {
                fin.write_key(i, &leaf.read_key(0));
            }

            self.recalculate_root_hash(leaf.root_hash());

            return Some(v);
        };

        let stack_top_frame = self.peek_stack();

        // if the only node in the tree is the root - return early
        if stack_top_frame.is_none() {
            let v = leaf.remove_by_idx(idx, leaf_len, &mut self._buf);
            leaf.write_len(leaf_len - 1);
            self.root_hash = leaf.root_hash();

            return Some(v);
        }

        self.steal_from_sibling_leaf_or_merge(stack_top_frame, leaf, idx, found_internal_node)
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
        SBTreeMapIter::<K, V>::new(&self.root, self.len)
    }

    #[inline]
    pub fn len(&self) -> u64 {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
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
        child_ptr: PtrRaw,
        child_hash: Hash,
    ) -> Option<(InternalBTreeNode<K>, [u8; K::SIZE])> {
        if len < CAPACITY {
            internal_node.insert_key(idx, &key, len, &mut self._buf);
            internal_node.insert_child_ptr(idx + 1, &child_ptr, len + 1, &mut self._buf);
            internal_node.insert_child_hash(idx + 1, &child_hash, len + 1, &mut self._buf, true);

            internal_node.write_len(len + 1);
            return None;
        }

        if self.pass_elem_to_sibling_internal(internal_node, idx, &key, &child_ptr, &child_hash) {
            return None;
        }

        // TODO: possible to optimize when idx == MIN_LEN_AFTER_SPLIT
        let (mut right, mid) = internal_node.split_max_len(&mut self._buf, true);

        if idx <= MIN_LEN_AFTER_SPLIT {
            internal_node.insert_key(idx, &key, MIN_LEN_AFTER_SPLIT, &mut self._buf);
            internal_node.insert_child_ptr(idx + 1, &child_ptr, B, &mut self._buf);
            internal_node.insert_child_hash(idx + 1, &child_hash, B, &mut self._buf, true);

            internal_node.write_len(B);
            right.write_len(MIN_LEN_AFTER_SPLIT);
        } else {
            right.insert_key(idx - B, &key, MIN_LEN_AFTER_SPLIT, &mut self._buf);
            right.insert_child_ptr(idx - B + 1, &child_ptr, B, &mut self._buf);
            right.insert_child_hash(idx - B + 1, &child_hash, B, &mut self._buf, true);

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

        if let Some(mut left_sibling) = parent.read_left_sibling::<LeafBTreeNode<K, V>>(parent_idx)
        {
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
                parent.write_child_hash(parent_idx - 1, &left_sibling.root_hash(), true);
                parent.write_child_hash(parent_idx, &leaf_node.root_hash(), true);

                return true;
            }
        }

        if let Some(mut right_sibling) =
            parent.read_right_sibling::<LeafBTreeNode<K, V>>(parent_idx, parent_len)
        {
            let right_sibling_len = right_sibling.read_len();

            if right_sibling_len < CAPACITY {
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
                parent.write_child_hash(parent_idx, &leaf_node.root_hash(), true);
                parent.write_child_hash(parent_idx + 1, &right_sibling.root_hash(), true);

                return true;
            }
        }

        false
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
        child_ptr: &PtrRaw,
        child_hash: &Hash,
    ) -> bool {
        let stack_top_frame = self.peek_stack();
        if stack_top_frame.is_none() {
            return false;
        }

        let (mut parent, parent_len, parent_idx) = unsafe { stack_top_frame.unwrap_unchecked() };

        if let Some(mut left_sibling) = parent.read_left_sibling::<InternalBTreeNode<K>>(parent_idx)
        {
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
                    child_hash,
                );
                parent.write_child_hash(parent_idx - 1, &left_sibling.root_hash(), true);
                parent.write_child_hash(parent_idx, &internal_node.root_hash(), true);

                return true;
            }
        }

        if let Some(mut right_sibling) =
            parent.read_right_sibling::<InternalBTreeNode<K>>(parent_idx, parent_len)
        {
            let right_sibling_len = right_sibling.read_len();

            if right_sibling_len < CAPACITY {
                self.pass_to_right_sibling_internal(
                    &mut parent,
                    parent_idx,
                    internal_node,
                    &mut right_sibling,
                    right_sibling_len,
                    idx,
                    key,
                    child_ptr,
                    child_hash,
                );
                parent.write_child_hash(parent_idx, &internal_node.root_hash(), true);
                parent.write_child_hash(parent_idx + 1, &right_sibling.root_hash(), true);

                return true;
            }
        }

        false
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
        child_ptr: &PtrRaw,
        child_hash: &Hash,
    ) {
        if i_idx != CAPACITY {
            rs.steal_from_left(
                rs_len,
                node,
                CAPACITY,
                p,
                p_idx,
                None,
                None,
                &mut self._buf,
                true,
            );
            rs.write_len(rs_len + 1);

            node.insert_key(i_idx, key, CAPACITY - 1, &mut self._buf);
            node.insert_child_ptr(i_idx + 1, child_ptr, CAPACITY, &mut self._buf);
            node.insert_child_hash(i_idx + 1, child_hash, CAPACITY, &mut self._buf, true);

            return;
        }

        let last = Some((key, child_ptr));
        rs.write_len(rs_len + 1);
        rs.steal_from_left(
            rs_len,
            node,
            CAPACITY,
            p,
            p_idx,
            last,
            Some(child_hash),
            &mut self._buf,
            true,
        );
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
        child_ptr: &PtrRaw,
        child_hash: &Hash,
    ) {
        if i_idx != 0 {
            ls.steal_from_right(
                ls_len,
                node,
                CAPACITY,
                p,
                p_idx - 1,
                None,
                None,
                &mut self._buf,
                true,
            );

            node.insert_key(i_idx - 1, key, CAPACITY - 1, &mut self._buf);
            node.insert_child_ptr(i_idx, child_ptr, CAPACITY, &mut self._buf);
            node.insert_child_hash(i_idx, child_hash, CAPACITY, &mut self._buf, true);

            ls.write_len(ls_len + 1);
            return;
        }

        let first = Some((key, child_ptr));
        ls.steal_from_right(
            ls_len,
            node,
            CAPACITY,
            p,
            p_idx - 1,
            first,
            Some(child_hash),
            &mut self._buf,
            true,
        );
        ls.write_len(ls_len + 1);
    }

    fn steal_from_sibling_leaf_or_merge(
        &mut self,
        stack_top_frame: Option<(InternalBTreeNode<K>, usize, usize)>,
        mut leaf: LeafBTreeNode<K, V>,
        idx: usize,
        found_internal_node: Option<(InternalBTreeNode<K>, usize)>,
    ) -> Option<V> {
        let (mut parent, parent_len, parent_idx) = unsafe { stack_top_frame.unwrap_unchecked() };

        if let Some(mut left_sibling) = parent.read_left_sibling::<LeafBTreeNode<K, V>>(parent_idx)
        {
            let left_sibling_len = left_sibling.read_len();

            // if possible to steal - return early
            if left_sibling_len > MIN_LEN_AFTER_SPLIT {
                self.steal_from_left_sibling_leaf(
                    &mut leaf,
                    &mut left_sibling,
                    left_sibling_len,
                    &mut parent,
                    parent_idx - 1,
                    found_internal_node,
                );

                // idx + 1, because after the rotation the leaf has one more key added before
                let v = leaf.remove_by_idx(idx + 1, B, &mut self._buf);

                parent.write_child_hash(parent_idx - 1, &left_sibling.root_hash(), true);
                parent.write_child_hash(parent_idx, &leaf.root_hash(), true);

                self._stack.pop();
                self.recalculate_root_hash(parent.root_hash());

                return Some(v);
            }

            if let Some(mut right_sibling) =
                parent.read_right_sibling::<LeafBTreeNode<K, V>>(parent_idx, parent_len)
            {
                let right_sibling_len = right_sibling.read_len();

                // if possible to steal - return early
                if right_sibling_len > MIN_LEN_AFTER_SPLIT {
                    self.steal_from_right_sibling_leaf(
                        &mut leaf,
                        &mut right_sibling,
                        right_sibling_len,
                        &mut parent,
                        parent_idx,
                        found_internal_node,
                    );

                    // just idx, because after rotation leaf has one more key added to the end
                    let v = leaf.remove_by_idx(idx, B, &mut self._buf);

                    parent.write_child_hash(parent_idx, &leaf.root_hash(), true);
                    parent.write_child_hash(parent_idx + 1, &right_sibling.root_hash(), true);

                    self._stack.pop();
                    self.recalculate_root_hash(parent.root_hash());

                    return Some(v);
                }

                let result = self.merge_with_right_sibling_leaf(
                    &mut leaf,
                    right_sibling,
                    idx,
                    found_internal_node,
                );

                parent.write_child_hash(parent_idx, &leaf.root_hash(), true);
                self.handle_stack_after_merge(true, leaf);

                return result;
            }

            let result = self.merge_with_left_sibling_leaf(leaf, &mut left_sibling, idx);

            parent.write_child_hash(parent_idx - 1, &left_sibling.root_hash(), true);
            self.handle_stack_after_merge(false, left_sibling);

            return result;
        }

        if let Some(mut right_sibling) =
            parent.read_right_sibling::<LeafBTreeNode<K, V>>(parent_idx, parent_len)
        {
            let right_sibling_len = right_sibling.read_len();

            // if possible to steal - return early
            if right_sibling_len > MIN_LEN_AFTER_SPLIT {
                self.steal_from_right_sibling_leaf(
                    &mut leaf,
                    &mut right_sibling,
                    right_sibling_len,
                    &mut parent,
                    parent_idx,
                    found_internal_node,
                );

                // just idx, because after rotation leaf has one more key added to the end
                let v = leaf.remove_by_idx(idx, B, &mut self._buf);

                parent.write_child_hash(parent_idx, &leaf.root_hash(), true);
                parent.write_child_hash(parent_idx + 1, &right_sibling.root_hash(), true);

                self._stack.pop();
                self.recalculate_root_hash(parent.root_hash());

                return Some(v);
            }

            let result = self.merge_with_right_sibling_leaf(
                &mut leaf,
                right_sibling,
                idx,
                found_internal_node,
            );

            parent.write_child_hash(parent_idx, &leaf.root_hash(), true);
            self.handle_stack_after_merge(true, leaf);

            return result;
        }

        unreachable!();
    }

    fn merge_with_right_sibling_leaf(
        &mut self,
        leaf: &mut LeafBTreeNode<K, V>,
        right_sibling: LeafBTreeNode<K, V>,
        idx: usize,
        found_internal_node: Option<(InternalBTreeNode<K>, usize)>,
    ) -> Option<V> {
        // otherwise merge with right
        leaf.merge_min_len(right_sibling, &mut self._buf);

        // just idx, because leaf keys stay unchanged
        let v = leaf.remove_by_idx(idx, CAPACITY - 1, &mut self._buf);
        leaf.write_len(CAPACITY - 2);

        if let Some((mut fin, i)) = found_internal_node {
            fin.write_key(i, &leaf.read_key(0));
        }

        Some(v)
    }

    fn merge_with_left_sibling_leaf(
        &mut self,
        leaf: LeafBTreeNode<K, V>,
        left_sibling: &mut LeafBTreeNode<K, V>,
        idx: usize,
    ) -> Option<V> {
        // if there is no right sibling - merge with left
        left_sibling.merge_min_len(leaf, &mut self._buf);
        // idx + MIN_LEN_AFTER_SPLIT, because all keys of leaf are added to the
        // end of left_sibling
        let v = left_sibling.remove_by_idx(idx + MIN_LEN_AFTER_SPLIT, CAPACITY - 1, &mut self._buf);
        left_sibling.write_len(CAPACITY - 2);

        // no reason to handle 'found_internal_node', because the key is
        // guaranteed to be in the nearest parent and left_sibling keys are all
        // continue to present

        Some(v)
    }

    fn steal_from_left_sibling_leaf(
        &mut self,
        leaf: &mut LeafBTreeNode<K, V>,
        left_sibling: &mut LeafBTreeNode<K, V>,
        left_sibling_len: usize,
        parent: &mut InternalBTreeNode<K>,
        parent_idx: usize,
        found_internal_node: Option<(InternalBTreeNode<K>, usize)>,
    ) {
        leaf.steal_from_left(
            MIN_LEN_AFTER_SPLIT,
            left_sibling,
            left_sibling_len,
            parent,
            parent_idx,
            None,
            &mut self._buf,
        );

        left_sibling.write_len(left_sibling_len - 1);

        if let Some((mut fin, i)) = found_internal_node {
            fin.write_key(i, &leaf.read_key(0));
        }
    }

    fn steal_from_right_sibling_leaf(
        &mut self,
        leaf: &mut LeafBTreeNode<K, V>,
        right_sibling: &mut LeafBTreeNode<K, V>,
        right_sibling_len: usize,
        parent: &mut InternalBTreeNode<K>,
        parent_idx: usize,
        found_internal_node: Option<(InternalBTreeNode<K>, usize)>,
    ) {
        leaf.steal_from_right(
            MIN_LEN_AFTER_SPLIT,
            right_sibling,
            right_sibling_len,
            parent,
            parent_idx,
            None,
            &mut self._buf,
        );

        right_sibling.write_len(right_sibling_len - 1);

        if let Some((mut fin, i)) = found_internal_node {
            fin.write_key(i, &leaf.read_key(0));
        }
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
                node.remove_child_hash(child_idx_to_remove, node_len + 1, &mut self._buf, true);
                node.write_len(node_len - 1);

                self.recalculate_root_hash(node.root_hash());

                return;
            }

            let stack_top_frame = self.peek_stack();

            // if there is no parent, return early
            if stack_top_frame.is_none() {
                // if the root has only one key, make child the new root
                if node_len == 1 {
                    node.destroy();
                    self.root_hash = prev_node.root_hash();
                    self.root = Some(prev_node);

                    return;
                }

                // otherwise simply remove and return
                node.remove_key(idx_to_remove, node_len, &mut self._buf);
                node.remove_child_ptr(child_idx_to_remove, node_len + 1, &mut self._buf);
                node.remove_child_hash(child_idx_to_remove, node_len + 1, &mut self._buf, true);
                node.write_len(node_len - 1);

                self.root_hash = node.root_hash();

                return;
            }

            let (mut parent, parent_len, parent_idx) =
                unsafe { stack_top_frame.unwrap_unchecked() };

            if let Some(mut left_sibling) =
                parent.read_left_sibling::<InternalBTreeNode<K>>(parent_idx)
            {
                let left_sibling_len = left_sibling.read_len();

                // steal from left if it is possible
                if left_sibling_len > MIN_LEN_AFTER_SPLIT {
                    self.steal_from_left_sibling_internal(
                        &mut node,
                        node_len,
                        idx_to_remove,
                        child_idx_to_remove,
                        &mut left_sibling,
                        left_sibling_len,
                        &mut parent,
                        parent_idx,
                    );
                    parent.write_child_hash(parent_idx, &node.root_hash(), true);
                    parent.write_child_hash(parent_idx - 1, &left_sibling.root_hash(), true);

                    self._stack.pop();
                    self.recalculate_root_hash(parent.root_hash());

                    return;
                }

                if let Some(mut right_sibling) =
                    parent.read_right_sibling::<InternalBTreeNode<K>>(parent_idx, parent_len)
                {
                    let right_sibling_len = right_sibling.read_len();

                    // steal from right if it's possible
                    if right_sibling_len > MIN_LEN_AFTER_SPLIT {
                        self.steal_from_right_sibling_internal(
                            &mut node,
                            node_len,
                            idx_to_remove,
                            child_idx_to_remove,
                            &mut right_sibling,
                            right_sibling_len,
                            &mut parent,
                            parent_idx,
                        );
                        parent.write_child_hash(parent_idx + 1, &right_sibling.root_hash(), true);
                        parent.write_child_hash(parent_idx, &node.root_hash(), true);

                        self._stack.pop();
                        self.recalculate_root_hash(parent.root_hash());

                        return;
                    }

                    // otherwise merge with right
                    self.merge_with_right_sibling_internal(
                        &mut node,
                        idx_to_remove,
                        child_idx_to_remove,
                        right_sibling,
                        &mut parent,
                        parent_idx,
                    );

                    parent.write_child_hash(parent_idx, &node.root_hash(), true);

                    merged_right = true;
                    prev_node = BTreeNode::Internal(node);

                    continue;
                }

                // otherwise merge with left
                self.merge_with_left_sibling_internal(
                    node,
                    idx_to_remove,
                    child_idx_to_remove,
                    &mut left_sibling,
                    &mut parent,
                    parent_idx,
                );

                parent.write_child_hash(parent_idx - 1, &left_sibling.root_hash(), true);

                merged_right = false;
                prev_node = BTreeNode::Internal(left_sibling);

                continue;
            }

            if let Some(mut right_sibling) =
                parent.read_right_sibling::<InternalBTreeNode<K>>(parent_idx, parent_len)
            {
                let right_sibling_len = right_sibling.read_len();

                // steal from right if it's possible
                if right_sibling_len > MIN_LEN_AFTER_SPLIT {
                    self.steal_from_right_sibling_internal(
                        &mut node,
                        node_len,
                        idx_to_remove,
                        child_idx_to_remove,
                        &mut right_sibling,
                        right_sibling_len,
                        &mut parent,
                        parent_idx,
                    );
                    parent.write_child_hash(parent_idx, &node.root_hash(), true);
                    parent.write_child_hash(parent_idx + 1, &right_sibling.root_hash(), true);

                    self._stack.pop();
                    self.recalculate_root_hash(parent.root_hash());

                    return;
                }

                // otherwise merge with right
                self.merge_with_right_sibling_internal(
                    &mut node,
                    idx_to_remove,
                    child_idx_to_remove,
                    right_sibling,
                    &mut parent,
                    parent_idx,
                );

                parent.write_child_hash(parent_idx, &node.root_hash(), true);

                merged_right = true;
                prev_node = BTreeNode::Internal(node);

                continue;
            }
        }
    }

    fn steal_from_right_sibling_internal(
        &mut self,
        node: &mut InternalBTreeNode<K>,
        node_len: usize,
        idx_to_remove: usize,
        child_idx_to_remove: usize,
        right_sibling: &mut InternalBTreeNode<K>,
        right_sibling_len: usize,
        parent: &mut InternalBTreeNode<K>,
        parent_idx: usize,
    ) {
        node.steal_from_right(
            node_len,
            right_sibling,
            right_sibling_len,
            parent,
            parent_idx,
            None,
            None,
            &mut self._buf,
            true,
        );
        right_sibling.write_len(right_sibling_len - 1);
        node.remove_key(idx_to_remove, B, &mut self._buf);
        node.remove_child_ptr(child_idx_to_remove, B + 1, &mut self._buf);
        node.remove_child_hash(child_idx_to_remove, B + 1, &mut self._buf, true);
    }

    fn steal_from_left_sibling_internal(
        &mut self,
        node: &mut InternalBTreeNode<K>,
        node_len: usize,
        idx_to_remove: usize,
        child_idx_to_remove: usize,
        left_sibling: &mut InternalBTreeNode<K>,
        left_sibling_len: usize,
        parent: &mut InternalBTreeNode<K>,
        parent_idx: usize,
    ) {
        node.steal_from_left(
            node_len,
            left_sibling,
            left_sibling_len,
            parent,
            parent_idx - 1,
            None,
            None,
            &mut self._buf,
            true,
        );
        left_sibling.write_len(left_sibling_len - 1);
        node.remove_key(idx_to_remove + 1, B, &mut self._buf);
        node.remove_child_ptr(child_idx_to_remove + 1, B + 1, &mut self._buf);
        node.remove_child_hash(child_idx_to_remove + 1, B + 1, &mut self._buf, true);
    }

    fn merge_with_right_sibling_internal(
        &mut self,
        node: &mut InternalBTreeNode<K>,
        idx_to_remove: usize,
        child_idx_to_remove: usize,
        right_sibling: InternalBTreeNode<K>,
        parent: &mut InternalBTreeNode<K>,
        parent_idx: usize,
    ) {
        let mid_element = parent.read_key(parent_idx);
        node.merge_min_len(&mid_element, right_sibling, &mut self._buf, true);
        node.remove_key(idx_to_remove, CAPACITY, &mut self._buf);
        node.remove_child_ptr(child_idx_to_remove, CHILDREN_CAPACITY, &mut self._buf);
        node.remove_child_hash(child_idx_to_remove, CHILDREN_CAPACITY, &mut self._buf, true);
        node.write_len(CAPACITY - 1);
    }

    fn merge_with_left_sibling_internal(
        &mut self,
        node: InternalBTreeNode<K>,
        idx_to_remove: usize,
        child_idx_to_remove: usize,
        left_sibling: &mut InternalBTreeNode<K>,
        parent: &mut InternalBTreeNode<K>,
        parent_idx: usize,
    ) {
        let mid_element = parent.read_key(parent_idx - 1);
        left_sibling.merge_min_len(&mid_element, node, &mut self._buf, true);
        left_sibling.remove_key(idx_to_remove + B, CAPACITY, &mut self._buf);
        left_sibling.remove_child_ptr(child_idx_to_remove + B, CHILDREN_CAPACITY, &mut self._buf);
        left_sibling.remove_child_hash(
            child_idx_to_remove + B,
            CHILDREN_CAPACITY,
            &mut self._buf,
            true,
        );
        left_sibling.write_len(CAPACITY - 1);
    }

    fn peek_stack(&self) -> Option<(InternalBTreeNode<K>, usize, usize)> {
        self._stack
            .last()
            .map(|(n, l, i)| (unsafe { n.copy() }, *l, *i))
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

impl<K: StableAllocated + Ord + AsHashableBytes, V: StableAllocated + AsHashableBytes>
    AsHashTree<&K> for SCertifiedBTreeMap<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    fn root_hash(&self) -> Hash {
        self.root_hash
    }

    fn witness(&self, index: &K, indexed_subtree: Option<HashTree>) -> HashTree {
        let mut node = if let Some(root) = self.root.as_ref() {
            unsafe { root.copy() }
        } else {
            return HashTree::Empty;
        };

        let mut stack = Vec::new();

        let (leaf, idx) = loop {
            match node {
                BTreeNode::Internal(internal_node) => {
                    let node_len = internal_node.read_len();
                    let child_idx = match internal_node.binary_search(index, node_len) {
                        Ok(idx) => idx + 1,
                        Err(idx) => idx,
                    };

                    let child_ptr =
                        u64::from_fixed_size_bytes(&internal_node.read_child_ptr(child_idx));

                    stack.push((internal_node, child_idx));

                    node = BTreeNode::from_ptr(child_ptr);
                }
                BTreeNode::Leaf(leaf_node) => {
                    match leaf_node.binary_search(index, leaf_node.read_len()) {
                        Ok(idx) => break (leaf_node, idx),
                        _ => return HashTree::Empty,
                    }
                }
            }
        };

        let mut witness = leaf.witness(idx, indexed_subtree);
        while let Some((parent, parent_idx)) = stack.pop() {
            witness = parent.witness(parent_idx, Some(witness));
        }

        witness
    }
}

impl<K: StableAllocated + Ord + Debug, V: StableAllocated + Debug> SCertifiedBTreeMap<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    pub fn debug_print_stack(&self) {
        isoprint(&format!(
            "STACK: {:?}",
            self._stack
                .iter()
                .map(|(p, l, i)| (p.as_ptr(), *l, *i))
                .collect::<Vec<_>>()
        ));
    }

    pub fn debug_print(&self) {
        if self.len == 0 {
            isoprint("EMPTY BTREEMAP");
            return;
        }

        let mut level = Vec::new();
        level.push(unsafe { self.root.as_ref().unwrap_unchecked().copy() });

        loop {
            Self::print_level(&level);
            isoprint("");

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
        let mut result = String::new();

        for node in level {
            result += &match node {
                BTreeNode::Internal(i) => i.to_string(),
                BTreeNode::Leaf(l) => l.to_string(),
            }
        }

        isoprint(&result);
    }
}

impl<K: StableAllocated + Ord + AsHashableBytes, V: StableAllocated + AsHashableBytes> Default
    for SCertifiedBTreeMap<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> FixedSize for SCertifiedBTreeMap<K, V> {
    const SIZE: usize = u64::SIZE * 2 + Hash::SIZE;
}

impl<K, V> AsFixedSizeBytes for SCertifiedBTreeMap<K, V> {
    fn as_fixed_size_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];

        let ptr = if let Some(root) = &self.root {
            root.as_ptr().as_fixed_size_bytes()
        } else {
            EMPTY_PTR.as_fixed_size_bytes()
        };

        buf[..u64::SIZE].copy_from_slice(&ptr);
        buf[u64::SIZE..(u64::SIZE * 2)].copy_from_slice(&self.len.as_fixed_size_bytes());
        buf[(u64::SIZE * 2)..].copy_from_slice(&self.root_hash);

        buf
    }

    fn from_fixed_size_bytes(buf: &[u8; Self::SIZE]) -> Self {
        let mut ptr_buf = [0u8; u64::SIZE];
        let mut len_buf = [0u8; u64::SIZE];
        let mut root_hash = EMPTY_HASH;

        ptr_buf.copy_from_slice(&buf[..u64::SIZE]);
        len_buf.copy_from_slice(&buf[u64::SIZE..(u64::SIZE * 2)]);
        root_hash.copy_from_slice(&buf[(u64::SIZE * 2)..]);

        let ptr = u64::from_fixed_size_bytes(&ptr_buf);
        let len = u64::from_fixed_size_bytes(&len_buf);

        Self {
            root: if ptr == EMPTY_PTR {
                None
            } else {
                Some(BTreeNode::from_ptr(ptr))
            },
            len,
            root_hash,
            _buf: Vec::default(),
            _stack: Vec::default(),
        }
    }
}

impl<K: StableAllocated + Ord, V: StableAllocated> StableAllocated for SCertifiedBTreeMap<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    #[inline]
    fn move_to_stable(&mut self) {}

    #[inline]
    fn remove_from_stable(&mut self) {}

    unsafe fn stable_drop(self) {
        if self.root.is_none() {
            return;
        }

        let mut nodes = vec![unsafe { self.root.unwrap_unchecked() }];
        let mut new_nodes = Vec::new();

        loop {
            if nodes.is_empty() {
                return;
            }

            for i in 0..nodes.len() {
                match unsafe { nodes.pop().unwrap_unchecked() } {
                    BTreeNode::Internal(internal) => {
                        for j in 0..(internal.read_len() + 1) {
                            let child_ptr_raw = internal.read_child_ptr(j);
                            let child_ptr = u64::from_fixed_size_bytes(&child_ptr_raw);
                            let child = BTreeNode::<K, V>::from_ptr(child_ptr);

                            new_nodes.push(child);
                        }

                        nodes = new_nodes;
                        new_nodes = Vec::new();
                        internal.destroy();
                    }
                    BTreeNode::Leaf(leaf) => {
                        for j in 0..leaf.read_len() {
                            let key = K::from_fixed_size_bytes(&leaf.read_key(j));
                            let value = V::from_fixed_size_bytes(&leaf.read_value(j));

                            key.stable_drop();
                            value.stable_drop();
                        }

                        leaf.destroy();
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::certified_btree_map::SCertifiedBTreeMap;
    use crate::primitive::StableAllocated;
    use crate::utils::certification::{AsHashTree, AsHashableBytes, HashTree};
    use crate::utils::encoding::AsFixedSizeBytes;
    use crate::{get_allocated_size, init_allocator, stable};
    use rand::seq::SliceRandom;
    use rand::thread_rng;

    impl AsHashableBytes for u64 {
        fn as_hashable_bytes(&self) -> Vec<u8> {
            self.to_le_bytes().to_vec()
        }

        fn from_hashable_bytes(bytes: Vec<u8>) -> Self {
            u64::from_le_bytes(bytes.try_into().unwrap())
        }
    }

    #[test]
    fn random_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let iterations = 1000;
        let mut map = SCertifiedBTreeMap::<u64, u64>::default();

        let mut example = Vec::new();
        for i in 0..iterations {
            example.push(i as u64);
        }
        example.shuffle(&mut thread_rng());

        for i in 0..iterations {
            assert!(map._stack.is_empty());
            assert!(map.insert(example[i], example[i]).is_none());

            for j in 0..i {
                let wit = map.witness(&example[j], None);
                assert_eq!(
                    wit.reconstruct(),
                    map.root_hash,
                    "invalid witness {:?}",
                    wit
                );
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

        assert_eq!(map.len(), iterations as u64);
        assert_eq!(map.is_empty(), false);

        map.debug_print_stack();
        map.debug_print();
        println!();
        println!();

        assert_eq!(map.insert(0, 1).unwrap(), 0);
        assert_eq!(map.insert(0, 0).unwrap(), 1);

        example.shuffle(&mut thread_rng());
        for i in 0..iterations {
            assert!(map._stack.is_empty());

            assert_eq!(map.remove(&example[i]), Some(example[i]));

            for j in (i + 1)..iterations {
                let wit = map.witness(&example[j], None);
                assert_eq!(
                    wit.reconstruct(),
                    map.root_hash,
                    "invalid witness of {}: {:?}",
                    example[j],
                    wit
                );
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

        let mut map = SCertifiedBTreeMap::<u64, u64>::default();

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

    #[test]
    fn stable_drop_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SCertifiedBTreeMap::<u64, u64>::default();

        for i in 0..200 {
            map.insert(i, i);
        }

        unsafe { map.stable_drop() };
        assert_eq!(get_allocated_size(), 0);
    }

    #[test]
    fn encoding_works() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SCertifiedBTreeMap::<u64, u64>::default();

        for i in 0..50 {
            map.insert(i, i);
        }

        let bytes = map.as_fixed_size_bytes();
        let map = SCertifiedBTreeMap::<u64, u64>::from_fixed_size_bytes(&bytes);

        for i in 0..50 {
            assert!(map.contains_key(&i));
        }

        unsafe { map.stable_drop() };
    }
}
