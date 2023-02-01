use crate::collections::btree_map::internal_node::{InternalBTreeNode, PtrRaw};
use crate::collections::btree_map::iter::SBTreeMapIter;
use crate::collections::btree_map::leaf_node::LeafBTreeNode;
use crate::encoding::{AsDynSizeBytes, AsFixedSizeBytes, Buffer};
use crate::mem::allocator::EMPTY_PTR;
use crate::primitive::s_ref::SRef;
use crate::primitive::s_ref_mut::SRefMut;
use crate::primitive::{StableAllocated, StableDrop};
use crate::{isoprint, SSlice};
use std::borrow::Borrow;
use std::fmt::Debug;
use std::mem;

pub const B: usize = 8;
pub const CAPACITY: usize = 2 * B - 1;
pub const MIN_LEN_AFTER_SPLIT: usize = B - 1;

pub const CHILDREN_CAPACITY: usize = 2 * B;
pub const CHILDREN_MIN_LEN_AFTER_SPLIT: usize = B;

pub const NODE_TYPE_INTERNAL: u8 = 127;
pub const NODE_TYPE_LEAF: u8 = 255;
pub const NODE_TYPE_OFFSET: usize = 0;

pub(crate) mod internal_node;
pub mod iter;
pub(crate) mod leaf_node;

// LEFT CHILD - LESS THAN
// RIGHT CHILD - MORE OR EQUAL THAN
pub struct SBTreeMap<K, V> {
    pub(crate) root: Option<BTreeNode<K, V>>,
    pub(crate) len: u64,
    pub(crate) certified: bool,
    _stack: Vec<(InternalBTreeNode<K>, usize, usize)>,
    _buf: Vec<u8>,
}

impl<K: StableAllocated + Ord, V: StableAllocated> SBTreeMap<K, V> {
    #[inline]
    pub fn new() -> Self {
        Self {
            root: None,
            len: 0,
            certified: false,
            _stack: Vec::default(),
            _buf: Vec::default(),
        }
    }

    #[inline]
    pub(crate) fn new_certified() -> Self {
        Self {
            root: None,
            len: 0,
            certified: true,
            _stack: Vec::default(),
            _buf: Vec::default(),
        }
    }

    #[inline]
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self._insert(key, value, &mut LeveledList::None)
    }

    pub(crate) fn _insert(&mut self, key: K, value: V, modified: &mut LeveledList) -> Option<V> {
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
                    self.push_stack(internal_node, node_len, child_idx);

                    node = BTreeNode::<K, V>::from_ptr(u64::from_fixed_size_bytes(&child_ptr));
                }
                BTreeNode::Leaf(leaf_node) => break unsafe { leaf_node.copy() },
            }
        };

        let right_leaf = match self.insert_leaf(&mut leaf, key, value, modified) {
            Ok(v) => {
                self.clear_stack(modified);

                return Some(v);
            }
            Err(right_leaf_opt) => {
                if let Some(right_leaf) = right_leaf_opt {
                    right_leaf
                } else {
                    self.clear_stack(modified);
                    self.len += 1;

                    return None;
                }
            }
        };

        let mut key_to_index = right_leaf.read_key(0);
        let mut ptr = right_leaf.as_ptr();

        while let Some((mut parent, parent_len, idx)) = self.pop_stack() {
            if let Some((right, _k)) = self.insert_internal(
                &mut parent,
                parent_len,
                idx,
                key_to_index,
                ptr.as_new_fixed_size_bytes(),
                modified,
            ) {
                key_to_index = _k;
                ptr = right.as_ptr();
                node = BTreeNode::Internal(parent);
            } else {
                self.clear_stack(modified);
                self.len += 1;

                return None;
            }
        }

        // stack is empty now

        let new_root = InternalBTreeNode::<K>::create(
            &key_to_index,
            &node.as_ptr().as_new_fixed_size_bytes(),
            &ptr.as_new_fixed_size_bytes(),
            self.certified,
        );

        modified.insert_root(new_root.as_ptr());

        self.root = Some(BTreeNode::Internal(new_root));
        self.len += 1;

        None
    }

    #[inline]
    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        self._remove(key, &mut LeveledList::None)
    }

    pub(crate) fn _remove<Q>(&mut self, key: &Q, modified: &mut LeveledList) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
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
                    self.push_stack(internal_node, node_len, child_idx);

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

            modified.push(self.current_depth(), leaf.as_ptr());
            self.clear_stack(modified);

            return Some(v);
        };

        let stack_top_frame = self.peek_stack();

        // if the only node in the tree is the root - return early
        if stack_top_frame.is_none() {
            let v = leaf.remove_by_idx(idx, leaf_len, &mut self._buf);
            leaf.write_len(leaf_len - 1);

            modified.push(0, leaf.as_ptr());

            return Some(v);
        }

        self.steal_from_sibling_leaf_or_merge(
            stack_top_frame,
            leaf,
            idx,
            found_internal_node,
            modified,
        )
    }

    pub unsafe fn get_copy<Q>(&self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        let (leaf_node, idx) = self.lookup(key, false)?;
        let v = V::from_fixed_size_bytes(leaf_node.read_value(idx)._deref());

        Some(v)
    }

    #[inline]
    pub fn get<Q>(&self, key: &Q) -> Option<SRef<V>>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        let (leaf_node, idx) = self.lookup(key, false)?;
        let ptr = leaf_node.get_value_ptr(idx);

        Some(SRef::new(ptr))
    }

    #[inline]
    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<SRefMut<V>>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        let (leaf_node, idx) = self.lookup(key, false)?;
        let ptr = leaf_node.get_value_ptr(idx);

        Some(SRefMut::new(ptr))
    }

    #[inline]
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Ord,
    {
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

    pub unsafe fn first_copy(&self) -> Option<(K, V)> {
        let mut node = self.get_root()?;

        loop {
            match node {
                BTreeNode::Internal(n) => {
                    let ptr = u64::from_fixed_size_bytes(&n.read_child_ptr(0));
                    node = BTreeNode::from_ptr(ptr);
                }
                BTreeNode::Leaf(n) => {
                    return Some(n.read_entry(0));
                }
            }
        }
    }

    pub unsafe fn last_copy(&self) -> Option<(K, V)> {
        let mut node = self.get_root()?;

        loop {
            match node {
                BTreeNode::Internal(n) => {
                    let len = n.read_len();

                    let ptr = u64::from_fixed_size_bytes(&n.read_child_ptr(len));
                    node = BTreeNode::from_ptr(ptr);
                }
                BTreeNode::Leaf(n) => {
                    let len = n.read_len();

                    return Some(n.read_entry(len - 1));
                }
            }
        }
    }

    #[inline]
    fn clear_stack(&mut self, modified: &mut LeveledList) {
        match modified {
            LeveledList::None => {
                self._stack.clear();
            }
            LeveledList::Some(_) => {
                while let Some((p, _, _)) = self._stack.pop() {
                    modified.push(self.current_depth(), p.as_ptr());
                }
            }
        }
    }

    #[inline]
    fn current_depth(&self) -> usize {
        self._stack.len()
    }

    #[inline]
    fn push_stack(&mut self, node: InternalBTreeNode<K>, len: usize, child_idx: usize) {
        self._stack.push((node, len, child_idx));
    }

    #[inline]
    fn pop_stack(&mut self) -> Option<(InternalBTreeNode<K>, usize, usize)> {
        self._stack.pop()
    }

    pub(crate) fn get_root(&self) -> Option<BTreeNode<K, V>> {
        unsafe { self.root.as_ref().map(|it| it.copy()) }
    }

    // WARNING: return_early == true will return nonsense leaf node and idx
    fn lookup<Q>(&self, key: &Q, return_early: bool) -> Option<(LeafBTreeNode<K, V>, usize)>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        let mut node = self.get_root()?;
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
        modified: &mut LeveledList,
    ) -> Result<V, Option<LeafBTreeNode<K, V>>> {
        let leaf_node_len = leaf_node.read_len();
        let insert_idx = match leaf_node.binary_search(&key, leaf_node_len) {
            Ok(existing_idx) => {
                // if there is already a key like that, return early
                let mut prev_value =
                    V::from_fixed_size_bytes(leaf_node.read_value(existing_idx)._deref());
                prev_value.remove_from_stable();
                value.move_to_stable();

                leaf_node.write_value(existing_idx, &value.as_new_fixed_size_bytes());

                modified.push(self.current_depth(), leaf_node.as_ptr());

                return Ok(prev_value);
            }
            Err(idx) => idx,
        };

        key.move_to_stable();
        let k = key.as_new_fixed_size_bytes();

        value.move_to_stable();
        let v = value.as_new_fixed_size_bytes();

        // if there is enough space - simply insert and return early
        if leaf_node_len < CAPACITY {
            leaf_node.insert_key(insert_idx, &k, leaf_node_len, &mut self._buf);
            leaf_node.insert_value(insert_idx, &v, leaf_node_len, &mut self._buf);

            leaf_node.write_len(leaf_node_len + 1);

            modified.push(self.current_depth(), leaf_node.as_ptr());

            return Err(None);
        }

        // try passing an element to a neighbor, to make room for a new one
        if self.pass_elem_to_sibling_leaf(leaf_node, &k, &v, insert_idx, modified) {
            return Err(None);
        }

        // split the leaf and insert so both leaves now have length of B
        let mut right = if insert_idx < B {
            let right = leaf_node.split_max_len(true, &mut self._buf, self.certified);
            leaf_node.insert_key(insert_idx, &k, MIN_LEN_AFTER_SPLIT, &mut self._buf);
            leaf_node.insert_value(insert_idx, &v, MIN_LEN_AFTER_SPLIT, &mut self._buf);

            right
        } else {
            let mut right = leaf_node.split_max_len(false, &mut self._buf, self.certified);
            right.insert_key(insert_idx - B, &k, MIN_LEN_AFTER_SPLIT, &mut self._buf);
            right.insert_value(insert_idx - B, &v, MIN_LEN_AFTER_SPLIT, &mut self._buf);

            right
        };

        leaf_node.write_len(B);
        right.write_len(B);

        modified.push(self.current_depth(), leaf_node.as_ptr());
        modified.push(self.current_depth(), right.as_ptr());

        Err(Some(right))
    }

    fn insert_internal(
        &mut self,
        internal_node: &mut InternalBTreeNode<K>,
        len: usize,
        idx: usize,
        key: K::Buf,
        child_ptr: PtrRaw,
        modified: &mut LeveledList,
    ) -> Option<(InternalBTreeNode<K>, K::Buf)> {
        if len < CAPACITY {
            internal_node.insert_key(idx, &key, len, &mut self._buf);
            internal_node.insert_child_ptr(idx + 1, &child_ptr, len + 1, &mut self._buf);

            internal_node.write_len(len + 1);

            modified.push(self.current_depth(), internal_node.as_ptr());

            return None;
        }

        if self.pass_elem_to_sibling_internal(internal_node, idx, &key, &child_ptr, modified) {
            return None;
        }

        // TODO: possible to optimize when idx == MIN_LEN_AFTER_SPLIT
        let (mut right, mid) = internal_node.split_max_len(&mut self._buf, self.certified);

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

        modified.push(self.current_depth(), internal_node.as_ptr());
        modified.push(self.current_depth(), right.as_ptr());

        Some((right, mid))
    }

    fn pass_elem_to_sibling_leaf(
        &mut self,
        leaf_node: &mut LeafBTreeNode<K, V>,
        key: &K::Buf,
        value: &V::Buf,
        insert_idx: usize,
        modified: &mut LeveledList,
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

                modified.push(self.current_depth(), leaf_node.as_ptr());
                modified.push(self.current_depth(), left_sibling.as_ptr());

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

                modified.push(self.current_depth(), leaf_node.as_ptr());
                modified.push(self.current_depth(), right_sibling.as_ptr());

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
        key: &K::Buf,
        value: &V::Buf,
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
        key: &K::Buf,
        value: &V::Buf,
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
        key: &K::Buf,
        child_ptr: &PtrRaw,
        modified: &mut LeveledList,
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
                );

                modified.push(self.current_depth(), internal_node.as_ptr());
                modified.push(self.current_depth(), left_sibling.as_ptr());

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
                );

                modified.push(self.current_depth(), internal_node.as_ptr());
                modified.push(self.current_depth(), right_sibling.as_ptr());

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
        key: &K::Buf,
        child_ptr: &PtrRaw,
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
        key: &K::Buf,
        child_ptr: &PtrRaw,
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

    fn steal_from_sibling_leaf_or_merge(
        &mut self,
        stack_top_frame: Option<(InternalBTreeNode<K>, usize, usize)>,
        mut leaf: LeafBTreeNode<K, V>,
        idx: usize,
        found_internal_node: Option<(InternalBTreeNode<K>, usize)>,
        modified: &mut LeveledList,
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

                modified.push(self.current_depth(), leaf.as_ptr());
                modified.push(self.current_depth(), left_sibling.as_ptr());
                self.clear_stack(modified);

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

                    modified.push(self.current_depth(), leaf.as_ptr());
                    modified.push(self.current_depth(), right_sibling.as_ptr());
                    self.clear_stack(modified);

                    return Some(v);
                }

                return self.merge_with_right_sibling_leaf(
                    leaf,
                    right_sibling,
                    idx,
                    found_internal_node,
                    modified,
                );
            }

            return self.merge_with_left_sibling_leaf(leaf, left_sibling, idx, modified);
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

                modified.push(self.current_depth(), leaf.as_ptr());
                modified.push(self.current_depth(), right_sibling.as_ptr());
                self.clear_stack(modified);

                return Some(v);
            }

            return self.merge_with_right_sibling_leaf(
                leaf,
                right_sibling,
                idx,
                found_internal_node,
                modified,
            );
        }

        unreachable!();
    }

    fn merge_with_right_sibling_leaf(
        &mut self,
        mut leaf: LeafBTreeNode<K, V>,
        right_sibling: LeafBTreeNode<K, V>,
        idx: usize,
        found_internal_node: Option<(InternalBTreeNode<K>, usize)>,
        modified: &mut LeveledList,
    ) -> Option<V> {
        modified.remove(self.current_depth(), right_sibling.as_ptr());
        modified.push(self.current_depth(), leaf.as_ptr());

        // otherwise merge with right
        leaf.merge_min_len(right_sibling, &mut self._buf);

        // just idx, because leaf keys stay unchanged
        let v = leaf.remove_by_idx(idx, CAPACITY - 1, &mut self._buf);
        leaf.write_len(CAPACITY - 2);

        if let Some((mut fin, i)) = found_internal_node {
            fin.write_key(i, &leaf.read_key(0));
        }

        self.handle_stack_after_merge(true, leaf, modified);

        Some(v)
    }

    fn merge_with_left_sibling_leaf(
        &mut self,
        leaf: LeafBTreeNode<K, V>,
        mut left_sibling: LeafBTreeNode<K, V>,
        idx: usize,
        modified: &mut LeveledList,
    ) -> Option<V> {
        modified.remove(self.current_depth(), leaf.as_ptr());
        modified.push(self.current_depth(), left_sibling.as_ptr());

        // if there is no right sibling - merge with left
        left_sibling.merge_min_len(leaf, &mut self._buf);
        // idx + MIN_LEN_AFTER_SPLIT, because all keys of leaf are added to the
        // end of left_sibling
        let v = left_sibling.remove_by_idx(idx + MIN_LEN_AFTER_SPLIT, CAPACITY - 1, &mut self._buf);
        left_sibling.write_len(CAPACITY - 2);

        // no reason to handle 'found_internal_node', because the key is
        // guaranteed to be in the nearest parent and left_sibling keys are all
        // continue to present

        self.handle_stack_after_merge(false, left_sibling, modified);

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

    fn handle_stack_after_merge(
        &mut self,
        mut merged_right: bool,
        leaf: LeafBTreeNode<K, V>,
        modified: &mut LeveledList,
    ) {
        let mut prev_node = BTreeNode::Leaf(leaf);

        while let Some((mut node, node_len, remove_idx)) = self.pop_stack() {
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

                modified.push(self.current_depth(), node.as_ptr());
                self.clear_stack(modified);

                return;
            }

            let stack_top_frame = self.peek_stack();

            // if there is no parent, return early
            if stack_top_frame.is_none() {
                // if the root has only one key, make child the new root
                if node_len == 1 {
                    modified.remove_root();

                    node.destroy();
                    self.root = Some(prev_node);

                    return;
                }

                // otherwise simply remove and return
                node.remove_key(idx_to_remove, node_len, &mut self._buf);
                node.remove_child_ptr(child_idx_to_remove, node_len + 1, &mut self._buf);
                node.write_len(node_len - 1);

                modified.push(self.current_depth(), node.as_ptr());

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
                    modified.push(self.current_depth(), node.as_ptr());
                    modified.push(self.current_depth(), left_sibling.as_ptr());

                    self.steal_from_left_sibling_internal(
                        node,
                        node_len,
                        idx_to_remove,
                        child_idx_to_remove,
                        left_sibling,
                        left_sibling_len,
                        parent,
                        parent_idx,
                    );

                    self.clear_stack(modified);

                    return;
                }

                if let Some(right_sibling) =
                    parent.read_right_sibling::<InternalBTreeNode<K>>(parent_idx, parent_len)
                {
                    let right_sibling_len = right_sibling.read_len();

                    // steal from right if it's possible
                    if right_sibling_len > MIN_LEN_AFTER_SPLIT {
                        modified.push(self.current_depth(), node.as_ptr());
                        modified.push(self.current_depth(), right_sibling.as_ptr());

                        self.steal_from_right_sibling_internal(
                            node,
                            node_len,
                            idx_to_remove,
                            child_idx_to_remove,
                            right_sibling,
                            right_sibling_len,
                            parent,
                            parent_idx,
                        );

                        self.clear_stack(modified);

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
                        modified,
                    );

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
                    modified,
                );

                merged_right = false;
                prev_node = BTreeNode::Internal(left_sibling);

                continue;
            }

            if let Some(right_sibling) =
                parent.read_right_sibling::<InternalBTreeNode<K>>(parent_idx, parent_len)
            {
                let right_sibling_len = right_sibling.read_len();

                // steal from right if it's possible
                if right_sibling_len > MIN_LEN_AFTER_SPLIT {
                    modified.push(self.current_depth(), node.as_ptr());
                    modified.push(self.current_depth(), right_sibling.as_ptr());

                    self.steal_from_right_sibling_internal(
                        node,
                        node_len,
                        idx_to_remove,
                        child_idx_to_remove,
                        right_sibling,
                        right_sibling_len,
                        parent,
                        parent_idx,
                    );

                    self.clear_stack(modified);

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
                    modified,
                );

                merged_right = true;
                prev_node = BTreeNode::Internal(node);

                continue;
            }
        }
    }

    fn steal_from_right_sibling_internal(
        &mut self,
        mut node: InternalBTreeNode<K>,
        node_len: usize,
        idx_to_remove: usize,
        child_idx_to_remove: usize,
        mut right_sibling: InternalBTreeNode<K>,
        right_sibling_len: usize,
        mut parent: InternalBTreeNode<K>,
        parent_idx: usize,
    ) {
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
    }

    fn steal_from_left_sibling_internal(
        &mut self,
        mut node: InternalBTreeNode<K>,
        node_len: usize,
        idx_to_remove: usize,
        child_idx_to_remove: usize,
        mut left_sibling: InternalBTreeNode<K>,
        left_sibling_len: usize,
        mut parent: InternalBTreeNode<K>,
        parent_idx: usize,
    ) {
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
    }

    fn merge_with_right_sibling_internal(
        &mut self,
        node: &mut InternalBTreeNode<K>,
        idx_to_remove: usize,
        child_idx_to_remove: usize,
        right_sibling: InternalBTreeNode<K>,
        parent: &mut InternalBTreeNode<K>,
        parent_idx: usize,
        modified: &mut LeveledList,
    ) {
        modified.remove(self.current_depth(), right_sibling.as_ptr());
        modified.push(self.current_depth(), node.as_ptr());

        let mid_element = parent.read_key(parent_idx);
        node.merge_min_len(&mid_element, right_sibling, &mut self._buf);
        node.remove_key(idx_to_remove, CAPACITY, &mut self._buf);
        node.remove_child_ptr(child_idx_to_remove, CHILDREN_CAPACITY, &mut self._buf);
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
        modified: &mut LeveledList,
    ) {
        modified.remove(self.current_depth(), node.as_ptr());
        modified.push(self.current_depth(), left_sibling.as_ptr());

        let mid_element = parent.read_key(parent_idx - 1);
        left_sibling.merge_min_len(&mid_element, node, &mut self._buf);
        left_sibling.remove_key(idx_to_remove + B, CAPACITY, &mut self._buf);
        left_sibling.remove_child_ptr(child_idx_to_remove + B, CHILDREN_CAPACITY, &mut self._buf);
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
                let mut new_root = BTreeNode::<K, V>::Leaf(LeafBTreeNode::create(self.certified));

                self.root = Some(new_root);
                unsafe { self.root.as_ref().unwrap_unchecked().copy() }
            }
        }
    }
}

impl<K: StableAllocated + Ord + StableDrop, V: StableAllocated + StableDrop> SBTreeMap<K, V> {
    #[inline]
    pub fn clear(&mut self) {
        let old = mem::replace(self, Self::new());

        unsafe { old.stable_drop() };
    }
}

impl<K: StableAllocated + Ord + StableDrop, V: StableAllocated + StableDrop> StableDrop
    for SBTreeMap<K, V>
{
    type Output = ();

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

            for _ in 0..nodes.len() {
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
                            let key = K::from_fixed_size_bytes(leaf.read_key(j)._deref());
                            let value = V::from_fixed_size_bytes(leaf.read_value(j)._deref());

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

impl<K: StableAllocated + Ord + Debug, V: StableAllocated + Debug> SBTreeMap<K, V> {
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

impl<K: StableAllocated + Ord, V: StableAllocated> Default for SBTreeMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> AsFixedSizeBytes for SBTreeMap<K, V> {
    const SIZE: usize = u64::SIZE * 2;
    type Buf = [u8; u64::SIZE * 2];

    fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
        let ptr = if let Some(root) = &self.root {
            root.as_ptr()
        } else {
            EMPTY_PTR
        };

        ptr.as_fixed_size_bytes(&mut buf[0..u64::SIZE]);
        self.len
            .as_fixed_size_bytes(&mut buf[u64::SIZE..(u64::SIZE * 2)]);
    }

    fn from_fixed_size_bytes(buf: &[u8]) -> Self {
        let ptr = u64::from_fixed_size_bytes(&buf[0..u64::SIZE]);
        let len = u64::from_fixed_size_bytes(&buf[u64::SIZE..(u64::SIZE * 2)]);

        Self {
            root: if ptr == EMPTY_PTR {
                None
            } else {
                Some(BTreeNode::from_ptr(ptr))
            },
            certified: false,
            len,
            _buf: Vec::default(),
            _stack: Vec::default(),
        }
    }
}

impl<K: StableAllocated + Ord, V: StableAllocated> StableAllocated for SBTreeMap<K, V> {
    #[inline]
    fn move_to_stable(&mut self) {}

    #[inline]
    fn remove_from_stable(&mut self) {}
}

pub(crate) enum LeveledList {
    None,
    Some((Vec<Vec<u64>>, usize)),
}

impl LeveledList {
    pub(crate) fn new() -> Self {
        Self::Some((vec![Vec::new()], 0))
    }

    fn insert_root(&mut self, ptr: u64) {
        match self {
            LeveledList::None => {}
            LeveledList::Some((v, max_level)) => {
                let root = vec![ptr];
                v.insert(0, root);
                *max_level += 1;
            }
        }
    }

    fn remove_root(&mut self) {
        match self {
            LeveledList::None => {}
            LeveledList::Some((v, max_level)) => {
                v.remove(0);
                *max_level -= 1;
            }
        }
    }

    fn push(&mut self, level: usize, ptr: u64) {
        match self {
            LeveledList::None => {}
            LeveledList::Some((v, max_level)) => {
                if level.gt(max_level) {
                    *max_level = level;
                }

                match v[level].binary_search(&ptr) {
                    Ok(_) => {}
                    Err(idx) => v[level].insert(idx, ptr),
                };
            }
        }
    }

    fn remove(&mut self, level: usize, ptr: u64) {
        match self {
            LeveledList::None => {}
            LeveledList::Some((v, _)) => {
                if let Ok(idx) = v[level].binary_search(&ptr) {
                    v[level].remove(idx);
                }
            }
        }
    }

    pub(crate) fn pop(&mut self) -> Option<u64> {
        match self {
            LeveledList::None => unreachable!(),
            LeveledList::Some((v, max_level)) => {
                let mut ptr = v[*max_level].pop();

                while ptr.is_none() {
                    if *max_level == 0 {
                        return None;
                    }

                    *max_level -= 1;

                    ptr = v[*max_level].pop();
                }

                ptr
            }
        }
    }

    pub(crate) fn debug_print(&self) {
        match self {
            LeveledList::None => isoprint("LeveledList [Dummy]"),
            LeveledList::Some((v, max_level)) => {
                let mut str = String::from("LeveledList [");
                for i in 0..(*max_level + 1) {
                    str += format!("{} - ({:?})", i, v[i]).as_str();
                    if i < *max_level {
                        str += ", ";
                    }
                }
                str += "]";

                isoprint(&str);
            }
        }
    }
}

pub trait IBTreeNode {
    unsafe fn from_ptr(ptr: u64) -> Self;
    fn as_ptr(&self) -> u64;
    unsafe fn copy(&self) -> Self;
}

pub(crate) enum BTreeNode<K, V> {
    Internal(InternalBTreeNode<K>),
    Leaf(LeafBTreeNode<K, V>),
}

impl<K, V> BTreeNode<K, V> {
    pub(crate) fn from_ptr(ptr: u64) -> Self {
        let node_type: u8 = SSlice::_as_fixed_size_bytes_read::<u8>(ptr, NODE_TYPE_OFFSET);

        unsafe {
            match node_type {
                NODE_TYPE_INTERNAL => Self::Internal(InternalBTreeNode::<K>::from_ptr(ptr)),
                NODE_TYPE_LEAF => Self::Leaf(LeafBTreeNode::<K, V>::from_ptr(ptr)),
                _ => unreachable!(),
            }
        }
    }

    pub(crate) fn as_ptr(&self) -> u64 {
        match self {
            Self::Internal(i) => i.as_ptr(),
            Self::Leaf(l) => l.as_ptr(),
        }
    }

    pub(crate) unsafe fn copy(&self) -> Self {
        match self {
            Self::Internal(i) => Self::Internal(i.copy()),
            Self::Leaf(l) => Self::Leaf(l.copy()),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::btree_map::SBTreeMap;
    use crate::primitive::StableDrop;
    use crate::{get_allocated_size, init_allocator, stable};
    use rand::seq::SliceRandom;
    use rand::thread_rng;
    use std::ops::Deref;

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
            map.debug_print_stack();
            assert!(map._stack.is_empty());
            assert!(map.insert(example[i], example[i]).is_none());

            for j in 0..i {
                assert!(
                    map.contains_key(&example[j]),
                    "don't contain {}",
                    example[j]
                );
                assert_eq!(
                    *map.get(&example[j]).unwrap(),
                    example[j],
                    "unable to get {}",
                    example[j]
                );
            }
        }

        assert_eq!(map.insert(0, 1).unwrap(), 0);
        assert_eq!(map.insert(0, 0).unwrap(), 1);

        map.debug_print();

        example.shuffle(&mut thread_rng());
        for i in 0..iterations {
            assert!(map._stack.is_empty());

            assert_eq!(map.remove(&example[i]), Some(example[i]));

            for j in (i + 1)..iterations {
                assert!(
                    map.contains_key(&example[j]),
                    "don't contain {}",
                    example[j]
                );
                assert_eq!(
                    *map.get(&example[j]).unwrap(),
                    example[j],
                    "unable to get {}",
                    example[j]
                );
            }
        }
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

        for (mut k, mut v) in map.iter() {
            assert_eq!(i, *k);
            assert_eq!(i, *v);

            i += 1;
        }

        assert_eq!(i, 199);

        for (mut k, mut v) in map.iter().rev() {
            println!("{}", i);
            assert_eq!(i, *k);
            assert_eq!(i, *v);

            i -= 1;
        }

        assert_eq!(i, 0);
    }

    #[test]
    fn stable_drop_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SBTreeMap::<u64, u64>::default();

        for i in 0..200 {
            map.insert(i, i);
        }

        unsafe { map.stable_drop() };
        assert_eq!(get_allocated_size(), 0);
    }
}
