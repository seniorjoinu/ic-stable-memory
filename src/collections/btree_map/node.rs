use crate::mem::allocator::EMPTY_PTR;
use crate::mem::s_slice::{SSlice, Side};
use crate::primitive::StableAllocated;
use crate::utils::phantom_data::SPhantomData;
use crate::{allocate, deallocate};
use copy_as_bytes::traits::{AsBytes, SuperSized};
use std::cmp::Ordering;
use std::fmt::{Debug, Formatter};

pub const B: usize = 6;
pub const CAPACITY: usize = 2 * B - 1;
pub const MIN_LEN_AFTER_SPLIT: usize = B - 1;

// DEFAULTS ARE
//
// parent: u64 = 0
// len: usize = 0
// is_leaf, is_root: bool = false
//
// keys: [K; CAPACITY] = [uninit; CAPACITY]
// values: [V; CAPACITY] = [uninit; CAPACITY]
// children: [u64; CAPACITY + 1] = [uninit; CAPACITY + 1]

const PARENT_OFFSET: usize = 0;
const LEN_OFFSET: usize = PARENT_OFFSET + u64::SIZE;
const IS_LEAF_OFFSET: usize = LEN_OFFSET + usize::SIZE;
const IS_ROOT_OFFSET: usize = IS_LEAF_OFFSET + bool::SIZE;
const KEYS_OFFSET: usize = IS_ROOT_OFFSET + bool::SIZE;

#[inline]
pub(crate) const fn VALUES_OFFSET<K: SuperSized>() -> usize {
    KEYS_OFFSET + CAPACITY * K::SIZE
}

#[inline]
pub(crate) const fn CHILDREN_OFFSET<K: SuperSized, V: SuperSized>() -> usize {
    VALUES_OFFSET::<K>() + CAPACITY * V::SIZE
}

#[inline]
pub(crate) const fn node_meta_size() -> usize {
    u64::SIZE + usize::SIZE + bool::SIZE * 2
}

pub(crate) struct BTreeNode<K, V> {
    ptr: u64,
    _marker_k: SPhantomData<K>,
    _marker_v: SPhantomData<V>,
}

impl<K: AsBytes, V: AsBytes> BTreeNode<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    pub fn new(is_leaf: bool, is_root: bool) -> Self {
        let slice =
            allocate(node_meta_size() + (K::SIZE + V::SIZE + u64::SIZE) * CAPACITY + u64::SIZE);
        let mut buf = [0u8; node_meta_size()];

        // FIXME: THIS IS UNSAFE - IN GENERAL WE DON'T KNOW HOW THE SERIALIZATION IS IMPLEMENTED INTERNALLY
        buf[IS_LEAF_OFFSET] = u8::from(is_leaf);
        buf[IS_ROOT_OFFSET] = u8::from(is_root);

        slice.write_bytes(0, &buf);

        Self {
            ptr: slice.get_ptr(),
            _marker_k: SPhantomData::new(),
            _marker_v: SPhantomData::new(),
        }
    }

    #[inline]
    pub fn as_ptr(&self) -> u64 {
        self.ptr
    }

    #[inline]
    pub unsafe fn from_ptr(ptr: u64) -> Self {
        Self {
            ptr,
            _marker_k: SPhantomData::new(),
            _marker_v: SPhantomData::new(),
        }
    }

    #[inline]
    pub unsafe fn copy(&self) -> Self {
        Self::from_ptr(self.as_ptr())
    }

    #[inline]
    pub fn set_parent(&mut self, it: u64) {
        SSlice::_as_bytes_write(self.ptr, PARENT_OFFSET, it)
    }

    #[inline]
    pub fn get_parent(&self) -> u64 {
        SSlice::_as_bytes_read(self.ptr, PARENT_OFFSET)
    }

    #[inline]
    pub fn set_len(&mut self, it: usize) {
        SSlice::_as_bytes_write(self.ptr, LEN_OFFSET, it);
    }

    #[inline]
    pub fn len(&self) -> usize {
        SSlice::_as_bytes_read(self.ptr, LEN_OFFSET)
    }

    #[inline]
    pub fn is_leaf(&self) -> bool {
        SSlice::_as_bytes_read(self.ptr, IS_LEAF_OFFSET)
    }

    #[inline]
    pub fn set_is_leaf(&mut self, it: bool) {
        SSlice::_as_bytes_write(self.ptr, IS_LEAF_OFFSET, it);
    }

    #[inline]
    pub fn is_root(&self) -> bool {
        SSlice::_as_bytes_read(self.ptr, IS_ROOT_OFFSET)
    }

    #[inline]
    pub fn set_is_root(&mut self, it: bool) {
        SSlice::_as_bytes_write(self.ptr, IS_ROOT_OFFSET, it)
    }

    #[inline]
    pub fn set_key(&mut self, idx: usize, k: K) {
        SSlice::_as_bytes_write(self.ptr, KEYS_OFFSET + idx * K::SIZE, k);
    }

    #[inline]
    pub fn get_key(&self, idx: usize) -> K {
        SSlice::_as_bytes_read(self.ptr, KEYS_OFFSET + idx * K::SIZE)
    }

    #[inline]
    pub fn set_value(&mut self, idx: usize, v: V) {
        SSlice::_as_bytes_write(self.ptr, VALUES_OFFSET::<K>() + idx * V::SIZE, v);
    }

    #[inline]
    pub fn get_value(&self, idx: usize) -> V {
        SSlice::_as_bytes_read(self.ptr, VALUES_OFFSET::<K>() + idx * V::SIZE)
    }

    #[inline]
    pub fn set_child_ptr(&mut self, idx: usize, c: u64) {
        SSlice::_as_bytes_write(self.ptr, CHILDREN_OFFSET::<K, V>() + idx * u64::SIZE, c);
    }

    #[inline]
    pub fn get_child_ptr(&self, idx: usize) -> u64 {
        SSlice::_as_bytes_read(self.ptr, CHILDREN_OFFSET::<K, V>() + idx * u64::SIZE)
    }

    #[inline]
    fn keys_shr(&mut self, idx1: usize, idx2: usize) {
        let mut buf = vec![0u8; (idx2 - idx1 + 1) * K::SIZE];

        SSlice::_read_bytes(self.ptr, KEYS_OFFSET + idx1 * K::SIZE, &mut buf);
        SSlice::_write_bytes(self.ptr, KEYS_OFFSET + (idx1 + 1) * K::SIZE, &buf);
    }

    #[inline]
    fn keys_shl(&mut self, idx1: usize, idx2: usize) {
        let mut buf = vec![0u8; (idx2 - idx1 + 1) * K::SIZE];

        SSlice::_read_bytes(self.ptr, KEYS_OFFSET + idx1 * K::SIZE, &mut buf);
        SSlice::_write_bytes(self.ptr, KEYS_OFFSET + (idx1 - 1) * K::SIZE, &buf);
    }

    #[inline]
    fn values_shr(&mut self, idx1: usize, idx2: usize) {
        let mut buf = vec![0u8; (idx2 - idx1 + 1) * V::SIZE];

        SSlice::_read_bytes(self.ptr, VALUES_OFFSET::<K>() + idx1 * V::SIZE, &mut buf);
        SSlice::_write_bytes(self.ptr, VALUES_OFFSET::<K>() + (idx1 + 1) * V::SIZE, &buf);
    }

    #[inline]
    fn values_shl(&mut self, idx1: usize, idx2: usize) {
        let mut buf = vec![0u8; (idx2 - idx1 + 1) * V::SIZE];

        SSlice::_read_bytes(self.ptr, VALUES_OFFSET::<K>() + idx1 * V::SIZE, &mut buf);
        SSlice::_write_bytes(self.ptr, VALUES_OFFSET::<K>() + (idx1 - 1) * V::SIZE, &buf);
    }

    #[inline]
    fn children_shr(&mut self, idx1: usize, idx2: usize) {
        let mut buf = vec![0u8; (idx2 - idx1 + 1) * u64::SIZE];

        SSlice::_read_bytes(
            self.ptr,
            CHILDREN_OFFSET::<K, V>() + idx1 * u64::SIZE,
            &mut buf,
        );
        SSlice::_write_bytes(
            self.ptr,
            CHILDREN_OFFSET::<K, V>() + (idx1 + 1) * u64::SIZE,
            &buf,
        );
    }

    #[inline]
    fn children_shl(&mut self, idx1: usize, idx2: usize) {
        let mut buf = vec![0u8; (idx2 - idx1 + 1) * u64::SIZE];

        SSlice::_read_bytes(
            self.ptr,
            CHILDREN_OFFSET::<K, V>() + idx1 * u64::SIZE,
            &mut buf,
        );
        SSlice::_write_bytes(
            self.ptr,
            CHILDREN_OFFSET::<K, V>() + (idx1 - 1) * u64::SIZE,
            &buf,
        );
    }
}

impl<K: AsBytes + Ord, V: AsBytes> BTreeNode<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
    [(); MIN_LEN_AFTER_SPLIT * K::SIZE]: Sized,
    [(); MIN_LEN_AFTER_SPLIT * V::SIZE]: Sized,
{
    pub fn merge_min_len(
        &mut self,
        is_leaf: bool,
        right: Self,
        parent: &mut Self,
        p_idx: usize,
        p_len: usize,
    ) {
        let parent_k = parent.get_key(p_idx);
        parent.remove_key(p_idx, p_len);

        let parent_v = parent.get_value(p_idx);
        parent.remove_value(p_idx, p_len);

        parent.remove_child_ptr(p_idx + 1, p_len + 1);
        parent.set_len(p_len - 1);

        let mut keys_buf = [0u8; MIN_LEN_AFTER_SPLIT * K::SIZE];
        SSlice::_read_bytes(right.ptr, KEYS_OFFSET, &mut keys_buf);
        SSlice::_as_bytes_write(
            self.ptr,
            KEYS_OFFSET + MIN_LEN_AFTER_SPLIT * K::SIZE,
            parent_k,
        );
        SSlice::_write_bytes(
            self.ptr,
            KEYS_OFFSET + MIN_LEN_AFTER_SPLIT * K::SIZE + K::SIZE,
            &keys_buf,
        );

        let mut values_buf = [0u8; MIN_LEN_AFTER_SPLIT * V::SIZE];
        SSlice::_read_bytes(right.ptr, VALUES_OFFSET::<K>(), &mut values_buf);
        SSlice::_as_bytes_write(
            self.ptr,
            VALUES_OFFSET::<K>() + MIN_LEN_AFTER_SPLIT * V::SIZE,
            parent_v,
        );
        SSlice::_write_bytes(
            self.ptr,
            VALUES_OFFSET::<K>() + MIN_LEN_AFTER_SPLIT * V::SIZE + V::SIZE,
            &values_buf,
        );

        if !is_leaf {
            let mut children_buf = [0u8; B * u64::SIZE];
            SSlice::_read_bytes(right.ptr, CHILDREN_OFFSET::<K, V>(), &mut children_buf);
            SSlice::_write_bytes(
                self.ptr,
                CHILDREN_OFFSET::<K, V>() + B * u64::SIZE,
                &children_buf,
            );
        }

        let slice = unsafe { SSlice::from_ptr(right.ptr, Side::Start).unwrap_unchecked() };
        deallocate(slice);
    }

    pub fn split_full_in_middle_no_pop(&mut self, is_leaf: bool) -> Self {
        let new_node = Self::new(is_leaf, false);

        let mut keys_buf = [0u8; MIN_LEN_AFTER_SPLIT * K::SIZE];
        SSlice::_read_bytes(self.ptr, KEYS_OFFSET + B * K::SIZE, &mut keys_buf);
        SSlice::_write_bytes(new_node.ptr, KEYS_OFFSET, &keys_buf);

        let mut values_buf = [0u8; MIN_LEN_AFTER_SPLIT * V::SIZE];
        SSlice::_read_bytes(
            self.ptr,
            VALUES_OFFSET::<K>() + B * V::SIZE,
            &mut values_buf,
        );
        SSlice::_write_bytes(new_node.ptr, VALUES_OFFSET::<K>(), &values_buf);

        if !is_leaf {
            let mut childrent_buf = [0u8; B * u64::SIZE];
            SSlice::_read_bytes(
                self.ptr,
                CHILDREN_OFFSET::<K, V>() + B * u64::SIZE,
                &mut childrent_buf,
            );
            SSlice::_write_bytes(new_node.ptr, CHILDREN_OFFSET::<K, V>(), &childrent_buf);
        }

        new_node
    }

    pub fn split_full_in_middle(&mut self, is_leaf: bool) -> (Self, K, V) {
        let k = self.get_key(B);
        let v = self.get_value(B);

        let new_node = self.split_full_in_middle_no_pop(is_leaf);

        (new_node, k, v)
    }

    pub fn insert_key(&mut self, k: K, idx: usize, len: usize) {
        debug_assert!(len < CAPACITY && idx <= len);

        if idx != len {
            self.keys_shr(idx, len);
        }

        self.set_key(idx, k);
    }

    pub fn remove_key(&mut self, idx: usize, len: usize) {
        debug_assert!(len <= CAPACITY && idx < len);

        if idx != len {
            self.keys_shl(idx + 1, len);
        }
    }

    pub fn insert_value(&mut self, v: V, idx: usize, len: usize) {
        debug_assert!(len < CAPACITY && idx <= len);

        if idx != len {
            self.values_shr(idx, len);
        }

        self.set_value(idx, v);
    }

    pub fn remove_value(&mut self, idx: usize, len: usize) {
        debug_assert!(len <= CAPACITY && idx < len);

        if idx != len {
            self.values_shl(idx + 1, len);
        }
    }

    pub fn insert_child_ptr(&mut self, c: u64, idx: usize, children_len: usize) {
        debug_assert!(children_len < CAPACITY + 1 && idx <= children_len);

        if idx != children_len {
            self.children_shr(idx, children_len);
        }

        self.set_child_ptr(idx, c);
    }

    pub fn remove_child_ptr(&mut self, idx: usize, children_len: usize) {
        debug_assert!(children_len <= CAPACITY && idx < children_len);

        if idx != children_len {
            self.children_shl(idx + 1, children_len);
        }
    }

    pub fn find_idx(&self, k: &K, len: usize) -> Result<usize, usize> {
        let mut min = 0;
        let mut max = len;
        let mut mid = (max - min) / 2;

        let mut buf = K::super_size_u8_arr();

        loop {
            SSlice::_read_bytes(self.ptr, KEYS_OFFSET + mid * K::SIZE, &mut buf);
            let key = K::from_bytes(buf);

            match key.cmp(k) {
                Ordering::Equal => return Ok(mid),
                // actually LESS
                Ordering::Greater => {
                    max = mid;
                    let new_mid = (max - min) / 2 + min;

                    if new_mid == mid {
                        return Err(mid);
                    }

                    mid = new_mid;
                    continue;
                }
                // actually GREATER
                Ordering::Less => {
                    min = mid;
                    let new_mid = (max - min) / 2 + min;

                    if new_mid == mid {
                        return Err(mid + 1);
                    }

                    mid = new_mid;
                    continue;
                }
            }
        }
    }
}

impl<K: StableAllocated + Ord, V: StableAllocated> BTreeNode<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
    [(); MIN_LEN_AFTER_SPLIT * K::SIZE]: Sized,
    [(); MIN_LEN_AFTER_SPLIT * V::SIZE]: Sized,
{
    fn handle_violating_internal(mut node: Self, stop_ptr: u64) {
        let parent_ptr = node.get_parent();

        // TODO: MAKE SURE PARENTS ARE SET CORRECTLY
        // TODO: NOT ONLY EMPTY BUT ALSO OTHER PTR

        if parent_ptr == stop_ptr {
            return;
        }

        let mut parent = unsafe { BTreeNode::<K, V>::from_ptr(parent_ptr) };
        let p_len = parent.len();

        // FIXME: not good - we have this in our stack
        let mut i = 0;
        let p_idx = loop {
            let n_ptr = parent.get_child_ptr(i);
            if n_ptr == node.ptr {
                break i;
            }

            i += 1;
            if i > p_len {
                unreachable!();
            }
        };

        if p_idx > 0 {
            // at first let's try rotating if it's possible

            let mut left = unsafe { BTreeNode::<K, V>::from_ptr(parent.get_child_ptr(p_idx - 1)) };
            let left_len = left.len();

            if left_len > MIN_LEN_AFTER_SPLIT {
                node.internal_violating_rotate_right(left, &mut parent, left_len, p_idx);
                return;
            }

            if p_idx < p_len {
                let right = unsafe { BTreeNode::<K, V>::from_ptr(parent.get_child_ptr(p_idx + 1)) };
                let right_len = right.len();

                if right_len > MIN_LEN_AFTER_SPLIT {
                    node.internal_violating_rotate_left(right, &mut parent, right_len, p_idx);
                    return;
                }
            }

            // if it is impossible to rotate, let's merge with the right neighbor,
            // stealing an element from the parent

            left.merge_min_len(false, node, &mut parent, p_idx, p_len);

            if p_len == MIN_LEN_AFTER_SPLIT {
                Self::handle_violating_internal(parent, stop_ptr);
            }

            return;
        }

        // the same goes here, but here we can only use the right neighbor

        let right = unsafe { BTreeNode::<K, V>::from_ptr(parent.get_child_ptr(1)) };
        let right_len = right.len();

        if right_len > MIN_LEN_AFTER_SPLIT {
            node.internal_violating_rotate_left(right, &mut parent, right_len, 0);
            return;
        }

        node.merge_min_len(false, right, &mut parent, 1, p_len);

        if p_len == MIN_LEN_AFTER_SPLIT {
            Self::handle_violating_internal(parent, stop_ptr);
        }
    }

    pub fn delete_in_violating_leaf(
        mut node: Self,
        mut parent: Self,
        p_idx: usize,
        p_len: usize,
        idx: usize,
        stop_ptr: u64,
    ) -> (K, V) {
        if p_idx > 0 {
            // at first let's try rotating if it's possible
            let mut left = unsafe { BTreeNode::<K, V>::from_ptr(parent.get_child_ptr(p_idx - 1)) };
            let left_len = left.len();

            if left_len > MIN_LEN_AFTER_SPLIT {
                let k = node.get_key(idx);
                let v = node.leaf_rotate_right(left, &mut parent, left_len, p_idx, idx);

                return (k, v);
            }

            if p_idx < p_len {
                let right = unsafe { BTreeNode::<K, V>::from_ptr(parent.get_child_ptr(p_idx + 1)) };
                let right_len = right.len();

                if right_len > MIN_LEN_AFTER_SPLIT {
                    let k = node.get_key(idx);
                    let v = node.leaf_rotate_left(right, &mut parent, right_len, p_idx, idx);

                    return (k, v);
                }
            }

            // if it is impossible to rotate, let's merge with the right neighbor,
            // stealing an element from the parent

            left.merge_min_len(true, node, &mut parent, p_idx, p_len);

            if p_len == MIN_LEN_AFTER_SPLIT {
                Self::handle_violating_internal(parent, stop_ptr);
            }

            let k = left.get_key(idx + MIN_LEN_AFTER_SPLIT + 1);
            let v = left.get_value(idx + MIN_LEN_AFTER_SPLIT + 1);

            left.remove_key(idx + MIN_LEN_AFTER_SPLIT + 1, CAPACITY);
            left.remove_value(idx + MIN_LEN_AFTER_SPLIT + 1, CAPACITY);
            left.set_len(CAPACITY - 1);

            return (k, v);
        }

        // the same goes here, but here we can only use the right neighbor

        let right = unsafe { BTreeNode::<K, V>::from_ptr(parent.get_child_ptr(1)) };
        let right_len = right.len();

        if right_len > MIN_LEN_AFTER_SPLIT {
            let mut k = node.get_key(idx);
            let mut v = node.leaf_rotate_left(right, &mut parent, right_len, 0, idx);

            k.remove_from_stable();
            v.remove_from_stable();

            return (k, v);
        }

        node.merge_min_len(true, right, &mut parent, 0, p_len);

        if p_len == MIN_LEN_AFTER_SPLIT {
            Self::handle_violating_internal(parent, stop_ptr);
        }

        let mut k = node.get_key(idx);
        let mut v = node.get_value(idx);

        node.remove_key(idx, CAPACITY);
        node.remove_value(idx, CAPACITY);
        node.set_len(CAPACITY - 1);

        k.remove_from_stable();
        v.remove_from_stable();

        (k, v)
    }

    pub fn leaf_rotate_right(
        &mut self,
        mut left: Self,
        parent: &mut Self,
        left_len: usize,
        parent_idx: usize,
        self_idx: usize,
    ) -> V {
        let left_last_k = left.get_key(left_len - 1);
        left.remove_key(left_len - 1, left_len);

        let left_last_v = left.get_value(left_len - 1);
        left.remove_value(left_len - 1, left_len);

        left.set_len(left_len - 1);

        let parent_k = parent.get_key(parent_idx - 1);
        let parent_v = parent.get_value(parent_idx - 1);

        parent.set_key(parent_idx - 1, left_last_k);
        parent.set_value(parent_idx - 1, left_last_v);

        let v = self.get_value(self_idx);
        self.set_key(self_idx, parent_k);
        self.set_value(self_idx, parent_v);

        v
    }

    pub fn internal_violating_rotate_right(
        &mut self,
        mut left: Self,
        parent: &mut Self,
        left_len: usize,
        parent_idx: usize,
    ) {
        let left_last_k = left.get_key(left_len - 1);
        left.remove_key(left_len - 1, left_len);

        let left_last_v = left.get_value(left_len - 1);
        left.remove_value(left_len - 1, left_len);

        let left_last_c = left.get_child_ptr(left_len);
        left.remove_child_ptr(left_len, left_len + 1);

        left.set_len(left_len - 1);

        let parent_k = parent.get_key(parent_idx - 1);
        let parent_v = parent.get_value(parent_idx - 1);

        parent.set_key(parent_idx - 1, left_last_k);
        parent.set_value(parent_idx - 1, left_last_v);

        self.insert_key(parent_k, 0, MIN_LEN_AFTER_SPLIT - 1);
        self.insert_value(parent_v, 0, MIN_LEN_AFTER_SPLIT - 1);
        self.insert_child_ptr(left_last_c, 0, MIN_LEN_AFTER_SPLIT);

        self.set_len(MIN_LEN_AFTER_SPLIT);
    }

    pub fn leaf_rotate_left(
        &mut self,
        mut right: Self,
        parent: &mut Self,
        right_len: usize,
        parent_idx: usize,
        self_idx: usize,
    ) -> V {
        let right_first_k = right.get_key(0);
        right.remove_key(0, right_len);

        let right_first_v = right.get_value(0);
        right.remove_value(0, right_len);

        right.set_len(right_len - 1);

        let parent_k = parent.get_key(parent_idx);
        let parent_v = parent.get_value(parent_idx);

        parent.set_key(parent_idx, right_first_k);
        parent.set_value(parent_idx, right_first_v);

        let v = self.get_value(self_idx);
        self.set_key(self_idx, parent_k);
        self.set_value(self_idx, parent_v);

        v
    }

    pub fn internal_violating_rotate_left(
        &mut self,
        mut right: Self,
        parent: &mut Self,
        right_len: usize,
        parent_idx: usize,
    ) {
        let right_first_k = right.get_key(0);
        right.remove_key(0, right_len);

        let right_first_v = right.get_value(0);
        right.remove_value(0, right_len);

        let right_first_c = right.get_child_ptr(0);
        right.remove_child_ptr(0, right_len + 1);

        right.set_len(right_len - 1);

        let parent_k = parent.get_key(parent_idx);
        let parent_v = parent.get_value(parent_idx);

        parent.set_key(parent_idx, right_first_k);
        parent.set_value(parent_idx, right_first_v);

        self.insert_key(parent_k, MIN_LEN_AFTER_SPLIT - 1, MIN_LEN_AFTER_SPLIT - 1);
        self.insert_value(parent_v, MIN_LEN_AFTER_SPLIT - 1, MIN_LEN_AFTER_SPLIT - 1);
        self.insert_child_ptr(right_first_c, MIN_LEN_AFTER_SPLIT, MIN_LEN_AFTER_SPLIT);

        self.set_len(MIN_LEN_AFTER_SPLIT);
    }

    // we definitely know, that this method is only called on non-leaves
    pub fn insert_up(&mut self, k: K, v: V, mut new_node: Self) -> Result<(), (K, V, Self)> {
        let len = self.len();

        match self.find_idx(&k, len) {
            Ok(_) => unreachable!(),
            Err(idx) => {
                if len == CAPACITY {
                    // optimization - when we insert directly in the middle,
                    // there is no point in popping the middle element up
                    // we can simply split in half and say that the newly inserted element should go up
                    if idx == MIN_LEN_AFTER_SPLIT {
                        let mut another_new_node = self.split_full_in_middle_no_pop(false);
                        another_new_node.set_len(MIN_LEN_AFTER_SPLIT);

                        self.insert_child_ptr(new_node.as_ptr(), B, B);
                        new_node.set_parent(self.as_ptr());
                        self.set_len(B);

                        return Err((k, v, another_new_node));
                    }

                    let (mut another_new_node, mid_k, mid_v) = self.split_full_in_middle(false);

                    if idx < MIN_LEN_AFTER_SPLIT {
                        self.insert_key(k, idx, MIN_LEN_AFTER_SPLIT);
                        self.insert_value(v, idx, MIN_LEN_AFTER_SPLIT);

                        self.insert_child_ptr(new_node.as_ptr(), idx + 1, B);
                        new_node.set_parent(self.as_ptr());

                        self.set_len(B);

                        another_new_node.set_len(MIN_LEN_AFTER_SPLIT);
                    } else {
                        another_new_node.insert_key(
                            k,
                            idx - MIN_LEN_AFTER_SPLIT,
                            MIN_LEN_AFTER_SPLIT,
                        );
                        another_new_node.insert_value(
                            v,
                            idx - MIN_LEN_AFTER_SPLIT,
                            MIN_LEN_AFTER_SPLIT,
                        );

                        another_new_node.insert_child_ptr(
                            new_node.as_ptr(),
                            idx - MIN_LEN_AFTER_SPLIT + 1,
                            B,
                        );
                        new_node.set_parent(another_new_node.as_ptr());
                        another_new_node.set_len(B);

                        self.set_len(MIN_LEN_AFTER_SPLIT);
                    }

                    return Err((mid_k, mid_v, another_new_node));
                }

                self.insert_key(k, idx, len);
                self.insert_value(v, idx, len);
                self.insert_child_ptr(new_node.as_ptr(), idx + 1, len + 1);
                new_node.set_parent(self.as_ptr());

                self.set_len(len + 1);

                Ok(())
            }
        }
    }

    pub fn insert_down(
        &mut self,
        mut k: K,
        mut v: V,
    ) -> Result<Result<Option<V>, (K, V, Self)>, (K, V, Self)> {
        let len = self.len();

        match self.find_idx(&k, len) {
            Ok(idx) => {
                v.move_to_stable();

                let mut prev_value = self.get_value(idx);
                self.set_value(idx, v);

                prev_value.remove_from_stable();

                Ok(Ok(Some(prev_value)))
            }
            Err(idx) => {
                if !self.is_leaf() {
                    let node = unsafe { BTreeNode::<K, V>::from_ptr(self.get_child_ptr(idx)) };

                    return Err((k, v, node));
                }

                k.move_to_stable();
                v.move_to_stable();

                if len == CAPACITY {
                    // optimization - when we insert directly in the middle,
                    // there is no point in popping the middle element up
                    // we can simply split in half and say that the newly inserted element should go up
                    if idx == MIN_LEN_AFTER_SPLIT {
                        let mut new_node = self.split_full_in_middle_no_pop(true);
                        new_node.set_len(MIN_LEN_AFTER_SPLIT);
                        self.set_len(B);

                        return Ok(Err((k, v, new_node)));
                    }

                    let (mut new_node, mid_k, mid_v) = self.split_full_in_middle(true);

                    if idx < MIN_LEN_AFTER_SPLIT {
                        self.insert_key(k, idx, MIN_LEN_AFTER_SPLIT);
                        self.insert_value(v, idx, MIN_LEN_AFTER_SPLIT);
                        self.set_len(B);

                        new_node.set_len(MIN_LEN_AFTER_SPLIT);
                    } else {
                        new_node.insert_key(k, idx - MIN_LEN_AFTER_SPLIT, MIN_LEN_AFTER_SPLIT);
                        new_node.insert_value(v, idx - MIN_LEN_AFTER_SPLIT, MIN_LEN_AFTER_SPLIT);
                        new_node.set_len(B);

                        self.set_len(MIN_LEN_AFTER_SPLIT);
                    }

                    return Ok(Err((mid_k, mid_v, new_node)));
                }

                self.insert_key(k, idx, len);
                self.insert_value(v, idx, len);
                self.set_len(len + 1);

                Ok(Ok(None))
            }
        }
    }
}

impl<K: AsBytes, V: AsBytes> Default for BTreeNode<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    fn default() -> Self {
        Self::new(false, false)
    }
}

impl<K, V> PartialEq for BTreeNode<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.ptr.eq(&other.ptr)
    }
}

impl<K, V> Eq for BTreeNode<K, V> {}

impl<K: AsBytes + Debug, V: AsBytes + Debug> Debug for BTreeNode<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("BTreeNode[")?;

        for i in 0..self.len() {
            let k = self.get_key(i);
            let v = self.get_value(i);

            f.write_str("(")?;
            k.fmt(f)?;
            f.write_str(", ")?;
            v.fmt(f)?;
            f.write_str(")")?;

            if i < self.len() - 1 {
                f.write_str(", ")?;
            }
        }

        f.write_str("]")
    }
}
