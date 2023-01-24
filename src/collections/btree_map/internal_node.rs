use crate::collections::btree_map::{BTreeNode, IBTreeNode};
use crate::collections::btree_map::{
    B, CAPACITY, CHILDREN_CAPACITY, CHILDREN_MIN_LEN_AFTER_SPLIT, MIN_LEN_AFTER_SPLIT,
    NODE_TYPE_INTERNAL, NODE_TYPE_OFFSET, PARENT_OFFSET,
};
use crate::mem::s_slice::Side;
use crate::primitive::StableAllocated;
use crate::utils::certification::{fork_hash, pruned, AsHashTree, AsHashableBytes, Hash, HashTree};
use crate::utils::encoding::{AsFixedSizeBytes, FixedSize};
use crate::{allocate, deallocate, mark_for_lazy_deallocation, SSlice};
use std::cmp::Ordering;
use std::fmt::Debug;
use std::marker::PhantomData;

pub type PtrRaw = [u8; u64::SIZE];

// LAYOUT:
// node_type: u8
// parent: u64
// len: usize
// children: [u64; CHILDREN_CAPACITY]
// keys: [K; CAPACITY]
// root_hash: Hash -- ONLY IF certified == true

const LEN_OFFSET: usize = PARENT_OFFSET + u64::SIZE;
const CHILDREN_OFFSET: usize = LEN_OFFSET + usize::SIZE;
const KEYS_OFFSET: usize = CHILDREN_OFFSET + u64::SIZE * CHILDREN_CAPACITY;

const fn root_hash_offset<K: FixedSize>() -> usize {
    KEYS_OFFSET + K::SIZE * CAPACITY
}

pub struct InternalBTreeNode<K> {
    ptr: u64,
    _marker_k: PhantomData<K>,
}

impl<K: StableAllocated + Ord> InternalBTreeNode<K>
where
    [(); K::SIZE]: Sized,
{
    #[inline]
    const fn calc_byte_size(certified: bool) -> usize {
        let mut size = root_hash_offset::<K>();

        if certified {
            size += Hash::SIZE
        }

        size
    }

    pub fn create_empty(certified: bool) -> Self {
        let slice = allocate(Self::calc_byte_size(certified));
        let mut it = Self {
            ptr: slice.get_ptr(),
            _marker_k: PhantomData::default(),
        };

        it.write_len(0);
        it.init_node_type();

        it
    }

    pub fn create(key: &[u8; K::SIZE], lcp: &PtrRaw, rcp: &PtrRaw, certified: bool) -> Self {
        let slice = allocate(Self::calc_byte_size(certified));
        let mut it = Self {
            ptr: slice.get_ptr(),
            _marker_k: PhantomData::default(),
        };

        it.write_len(1);
        it.init_node_type();

        it.write_key(0, key);

        it.write_child_ptr(0, lcp);
        it.write_child_ptr(1, rcp);

        it
    }

    #[inline]
    pub fn destroy(self) {
        mark_for_lazy_deallocation(self.ptr);
    }

    pub fn binary_search(&self, k: &K, len: usize) -> Result<usize, usize> {
        let mut min = 0;
        let mut max = len;
        let mut mid = (max - min) / 2;

        let mut buf = K::_u8_arr_of_size();

        loop {
            SSlice::_read_bytes(self.ptr, KEYS_OFFSET + mid * K::SIZE, &mut buf);
            let key = K::from_fixed_size_bytes(&buf);

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

    pub fn steal_from_left(
        &mut self,
        self_len: usize,
        left_sibling: &mut Self,
        left_sibling_len: usize,
        parent: &mut Self,
        parent_idx: usize,
        left_insert_last_element: Option<(&[u8; K::SIZE], &PtrRaw)>,
        buf: &mut Vec<u8>,
    ) {
        let pk = parent.read_key(parent_idx);

        if let Some((k, c)) = left_insert_last_element {
            parent.write_key(parent_idx, k);
            self.insert_child_ptr(0, c, self_len + 1, buf);
        } else {
            let lsk = left_sibling.read_key(left_sibling_len - 1);
            parent.write_key(parent_idx, &lsk);

            let lsc = left_sibling.read_child_ptr(left_sibling_len);
            self.insert_child_ptr(0, &lsc, self_len + 1, buf);
        };

        self.insert_key(0, &pk, self_len, buf);
    }

    pub fn steal_from_right(
        &mut self,
        self_len: usize,
        right_sibling: &mut Self,
        right_sibling_len: usize,
        parent: &mut Self,
        parent_idx: usize,
        right_insert_first_element: Option<(&[u8; K::SIZE], &PtrRaw)>,
        buf: &mut Vec<u8>,
    ) {
        let pk = parent.read_key(parent_idx);

        let rsc = if let Some((k, c)) = right_insert_first_element {
            let rsc = right_sibling.read_child_ptr(0);
            right_sibling.write_child_ptr(0, c);

            parent.write_key(parent_idx, k);

            rsc
        } else {
            let rsk = right_sibling.read_key(0);
            right_sibling.remove_key(0, right_sibling_len, buf);

            let rsc = right_sibling.read_child_ptr(0);
            right_sibling.remove_child_ptr(0, right_sibling_len + 1, buf);

            parent.write_key(parent_idx, &rsk);

            rsc
        };

        self.push_key(&pk, self_len);
        self.push_child_ptr(&rsc, self_len + 1);
    }

    pub fn split_max_len(
        &mut self,
        buf: &mut Vec<u8>,
        certified: bool,
    ) -> (InternalBTreeNode<K>, [u8; K::SIZE]) {
        let mut right = InternalBTreeNode::<K>::create_empty(certified);

        self.read_keys_to_buf(B, MIN_LEN_AFTER_SPLIT, buf);
        right.write_keys_from_buf(0, buf);

        self.read_child_ptrs_to_buf(B, CHILDREN_MIN_LEN_AFTER_SPLIT, buf);

        // change parent of right's new children to right
        let right_ptr_buf = right.ptr.as_fixed_size_bytes();

        for i in 0..CHILDREN_MIN_LEN_AFTER_SPLIT {
            let mut ptr_buf = u64::_u8_arr_of_size();
            ptr_buf.copy_from_slice(&buf[(i * u64::SIZE)..((i + 1) * u64::SIZE)]);

            let ptr = u64::from_fixed_size_bytes(&ptr_buf);
            let mut child = BTreeNode::<u64, u64>::from_ptr(ptr);

            child.write_parent(&right_ptr_buf);
        }

        right.write_child_ptrs_from_buf(0, buf);
        right.write_parent(&self.read_parent());

        (right, self.read_key(MIN_LEN_AFTER_SPLIT))
    }

    pub fn merge_min_len(
        &mut self,
        mid: &[u8; K::SIZE],
        right: InternalBTreeNode<K>,
        buf: &mut Vec<u8>,
    ) {
        self.push_key(mid, MIN_LEN_AFTER_SPLIT);

        right.read_keys_to_buf(0, MIN_LEN_AFTER_SPLIT, buf);
        self.write_keys_from_buf(B, buf);

        right.read_child_ptrs_to_buf(0, CHILDREN_MIN_LEN_AFTER_SPLIT, buf);

        // change parent of right's children to self
        let self_ptr_buf = self.ptr.as_fixed_size_bytes();

        for i in 0..CHILDREN_MIN_LEN_AFTER_SPLIT {
            let mut ptr_buf = u64::_u8_arr_of_size();
            ptr_buf.copy_from_slice(&buf[(i * u64::SIZE)..((i + 1) * u64::SIZE)]);

            let ptr = u64::from_fixed_size_bytes(&ptr_buf);
            let mut child = BTreeNode::<u64, u64>::from_ptr(ptr);

            child.write_parent(&self_ptr_buf);
        }

        self.write_child_ptrs_from_buf(B, buf);

        right.destroy();
    }

    #[inline]
    pub fn push_key(&mut self, key: &[u8; K::SIZE], len: usize) {
        self.write_key(len, key);
    }

    pub fn insert_key(&mut self, idx: usize, key: &[u8; K::SIZE], len: usize, buf: &mut Vec<u8>) {
        if idx == len {
            self.push_key(key, len);
            return;
        }

        self.read_keys_to_buf(idx, len - idx, buf);
        self.write_keys_from_buf(idx + 1, buf);

        self.write_key(idx, key);
    }

    pub fn remove_key(&mut self, idx: usize, len: usize, buf: &mut Vec<u8>) {
        if idx == len - 1 {
            return;
        }

        self.read_keys_to_buf(idx + 1, len - idx - 1, buf);
        self.write_keys_from_buf(idx, buf);
    }

    #[inline]
    pub fn push_child_ptr(&mut self, ptr: &PtrRaw, children_len: usize) {
        self.write_child_ptr(children_len, ptr);
    }

    pub fn insert_child_ptr(
        &mut self,
        idx: usize,
        ptr: &PtrRaw,
        children_len: usize,
        buf: &mut Vec<u8>,
    ) {
        if idx == children_len {
            self.push_child_ptr(ptr, children_len);
            return;
        }

        self.read_child_ptrs_to_buf(idx, children_len - idx, buf);
        self.write_child_ptrs_from_buf(idx + 1, buf);

        self.write_child_ptr(idx, ptr);
    }

    pub fn remove_child_ptr(&mut self, idx: usize, children_len: usize, buf: &mut Vec<u8>) {
        if idx == children_len - 1 {
            return;
        }

        self.read_child_ptrs_to_buf(idx + 1, children_len - idx - 1, buf);
        self.write_child_ptrs_from_buf(idx, buf);
    }

    pub fn read_left_sibling<T: IBTreeNode>(&self, idx: usize) -> Option<T> {
        if idx == 0 {
            return None;
        }

        let left_sibling_ptr = u64::from_fixed_size_bytes(&self.read_child_ptr(idx - 1));

        unsafe { Some(T::from_ptr(left_sibling_ptr)) }
    }

    pub fn read_right_sibling<T: IBTreeNode>(&self, idx: usize, len: usize) -> Option<T> {
        if idx == len {
            return None;
        }

        let right_sibling_ptr = u64::from_fixed_size_bytes(&self.read_child_ptr(idx + 1));

        unsafe { Some(T::from_ptr(right_sibling_ptr)) }
    }

    #[inline]
    pub fn read_key(&self, idx: usize) -> [u8; K::SIZE] {
        SSlice::_read_const_u8_array_of_size::<K>(self.ptr, KEYS_OFFSET + idx * K::SIZE)
    }

    #[inline]
    fn read_keys_to_buf(&self, from_idx: usize, len: usize, buf: &mut Vec<u8>) {
        buf.resize(len * K::SIZE, 0);
        SSlice::_read_bytes(self.ptr, KEYS_OFFSET + from_idx * K::SIZE, buf);
    }

    #[inline]
    pub fn read_child_ptr(&self, idx: usize) -> PtrRaw {
        SSlice::_read_const_u8_array_of_size::<u64>(self.ptr, CHILDREN_OFFSET + idx * u64::SIZE)
    }

    #[inline]
    fn read_child_ptrs_to_buf(&self, from_idx: usize, len: usize, buf: &mut Vec<u8>) {
        buf.resize(len * u64::SIZE, 0);
        SSlice::_read_bytes(self.ptr, CHILDREN_OFFSET + from_idx * u64::SIZE, buf);
    }

    #[inline]
    pub fn write_key(&mut self, idx: usize, key: &[u8; K::SIZE]) {
        SSlice::_write_bytes(self.ptr, KEYS_OFFSET + idx * K::SIZE, key);
    }

    #[inline]
    fn write_keys_from_buf(&mut self, from_idx: usize, buf: &Vec<u8>) {
        SSlice::_write_bytes(self.ptr, KEYS_OFFSET + from_idx * K::SIZE, buf);
    }

    #[inline]
    pub fn write_child_ptr(&mut self, idx: usize, ptr: &PtrRaw) {
        SSlice::_write_bytes(self.ptr, CHILDREN_OFFSET + idx * u64::SIZE, ptr);
    }

    #[inline]
    fn write_child_ptrs_from_buf(&mut self, from_idx: usize, buf: &Vec<u8>) {
        SSlice::_write_bytes(self.ptr, CHILDREN_OFFSET + from_idx * u64::SIZE, buf);
    }

    #[inline]
    pub fn write_root_hash(&mut self, root_hash: &Hash, certified: bool) {
        debug_assert!(certified);
        SSlice::_write_bytes(self.ptr, root_hash_offset::<K>(), root_hash);
    }

    #[inline]
    pub fn read_root_hash(&self, certified: bool) -> Hash {
        debug_assert!(certified);
        SSlice::_as_fixed_size_bytes_read(self.ptr, root_hash_offset::<K>())
    }

    #[inline]
    pub fn write_len(&mut self, len: usize) {
        SSlice::_as_fixed_size_bytes_write::<usize>(self.ptr, LEN_OFFSET, len)
    }

    #[inline]
    pub fn read_len(&self) -> usize {
        SSlice::_as_fixed_size_bytes_read::<usize>(self.ptr, LEN_OFFSET)
    }

    #[inline]
    fn init_node_type(&mut self) {
        SSlice::_as_fixed_size_bytes_write::<u8>(self.ptr, NODE_TYPE_OFFSET, NODE_TYPE_INTERNAL)
    }
}

impl<K: AsHashableBytes + Ord + StableAllocated> InternalBTreeNode<K>
where
    [(); K::SIZE]: Sized,
{
    #[inline]
    pub fn read_child_root_hash<V: StableAllocated + AsHashableBytes>(
        &self,
        idx: usize,
        certified: bool,
    ) -> Hash
    where
        [(); V::SIZE]: Sized,
    {
        debug_assert!(certified);

        let ptr = u64::from_fixed_size_bytes(&self.read_child_ptr(idx));
        let child = BTreeNode::<K, V>::from_ptr(ptr);

        match child {
            BTreeNode::Internal(n) => n.root_hash(),
            BTreeNode::Leaf(n) => n.root_hash(),
        }
    }
}

impl<K> IBTreeNode for InternalBTreeNode<K> {
    #[inline]
    unsafe fn from_ptr(ptr: u64) -> Self {
        Self {
            ptr,
            _marker_k: PhantomData::default(),
        }
    }

    #[inline]
    fn as_ptr(&self) -> u64 {
        self.ptr
    }

    #[inline]
    unsafe fn copy(&self) -> Self {
        Self::from_ptr(self.ptr)
    }

    #[inline]
    fn read_parent(&self) -> [u8; u64::SIZE] {
        SSlice::_read_const_u8_array_of_size::<u64>(self.ptr, PARENT_OFFSET)
    }

    #[inline]
    fn write_parent(&mut self, parent: &[u8; u64::SIZE]) {
        SSlice::_write_bytes(self.ptr, PARENT_OFFSET, parent)
    }
}

impl<K: StableAllocated + Ord + Debug> InternalBTreeNode<K>
where
    [(); K::SIZE]: Sized,
{
    pub fn to_string(&self) -> String {
        let mut result = format!(
            "InternalBTreeNode(&{}, {})[",
            self.as_ptr(),
            self.read_len()
        );
        for i in 0..self.read_len() {
            result += &format!(
                "*({}), ",
                u64::from_fixed_size_bytes(&self.read_child_ptr(i))
            );
            result += &format!("{:?}, ", K::from_fixed_size_bytes(&self.read_key(i)));
        }

        result += &format!(
            "*({})]",
            u64::from_fixed_size_bytes(&self.read_child_ptr(self.read_len()))
        );

        result
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::btree_map::internal_node::InternalBTreeNode;
    use crate::collections::btree_map::{
        B, CAPACITY, CHILDREN_MIN_LEN_AFTER_SPLIT, MIN_LEN_AFTER_SPLIT,
    };
    use crate::utils::encoding::AsFixedSizeBytes;
    use crate::{init_allocator, stable};

    #[test]
    fn works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut node = InternalBTreeNode::<u64>::create_empty(false);
        let mut buf = Vec::default();

        for i in 0..CAPACITY {
            node.push_key(&(i as u64).as_fixed_size_bytes(), i);
        }

        node.write_len(CAPACITY);
        println!("{}", node.to_string());
        println!();

        for i in 0..CAPACITY {
            let k = node.read_key(CAPACITY - i - 1);
            assert_eq!(k, ((CAPACITY - i - 1) as u64).as_fixed_size_bytes());
        }

        for i in 0..CAPACITY {
            node.insert_key(0, &(i as u64).as_fixed_size_bytes(), i, &mut buf);
        }

        for i in 0..CAPACITY {
            let k = node.read_key(i);
            node.remove_key(i, CAPACITY, &mut buf);
            assert_eq!(k, ((CAPACITY - i - 1) as u64).as_fixed_size_bytes());

            node.insert_key(i, &k, CAPACITY - 1, &mut buf);
            node.push_child_ptr(&1u64.as_fixed_size_bytes(), i);
        }

        node.push_child_ptr(&1u64.as_fixed_size_bytes(), CAPACITY);

        println!("before split: ");
        println!("{}", node.to_string());
        println!();

        let (mut right, mid) = node.split_max_len(&mut buf, false);

        node.write_len(MIN_LEN_AFTER_SPLIT);
        right.write_len(MIN_LEN_AFTER_SPLIT);

        println!("after split: ");
        println!("{}", node.to_string());
        println!("{}", right.to_string());

        assert_eq!(node.read_len(), MIN_LEN_AFTER_SPLIT);
        assert_eq!(right.read_len(), MIN_LEN_AFTER_SPLIT);

        for i in 0..node.read_len() {
            let k = node.read_key(i);
            assert_eq!(k, ((CAPACITY - i - 1) as u64).as_fixed_size_bytes());

            let c = node.read_child_ptr(i);
            assert_eq!(c, 1u64.as_fixed_size_bytes());
        }

        let c = node.read_child_ptr(MIN_LEN_AFTER_SPLIT);
        assert_eq!(c, 1u64.as_fixed_size_bytes());

        for i in 0..right.read_len() {
            let k = right.read_key(i);
            assert_eq!(k, ((CAPACITY - B - i - 1) as u64).as_fixed_size_bytes());

            let c = right.read_child_ptr(i);
            assert_eq!(c, 1u64.as_fixed_size_bytes());
        }

        let c = right.read_child_ptr(CHILDREN_MIN_LEN_AFTER_SPLIT - 1);
        assert_eq!(c, 1u64.as_fixed_size_bytes());

        node.merge_min_len(&mid, right, &mut buf);

        node.write_len(CAPACITY);
        assert_eq!(node.read_len(), CAPACITY);

        for i in 0..node.read_len() {
            let k = node.read_key(i);
            assert_eq!(k, ((CAPACITY - i - 1) as u64).as_fixed_size_bytes());

            let c = node.read_child_ptr(i);
            assert_eq!(c, 1u64.as_fixed_size_bytes());
        }

        let c = node.read_child_ptr(CAPACITY - 1);
        assert_eq!(c, 1u64.as_fixed_size_bytes());
    }
}
