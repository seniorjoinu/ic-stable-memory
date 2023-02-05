use crate::collections::btree_map::{BTreeNode, IBTreeNode};
use crate::collections::btree_map::{
    B, CAPACITY, CHILDREN_CAPACITY, CHILDREN_MIN_LEN_AFTER_SPLIT, MIN_LEN_AFTER_SPLIT,
    NODE_TYPE_INTERNAL, NODE_TYPE_OFFSET,
};
use crate::encoding::{AsFixedSizeBytes, Buffer};
use crate::mem::s_slice::Side;
use crate::mem::{stable_ptr_buf, StablePtr, StablePtrBuf};
use crate::primitive::StableType;
use crate::utils::certification::{AsHashTree, AsHashableBytes, Hash, EMPTY_HASH};
use crate::{allocate, deallocate, SSlice};
use std::borrow::Borrow;
use std::cmp::Ordering;
use std::fmt::Debug;
use std::marker::PhantomData;

// LAYOUT:
// node_type: u8
// len: usize
// children: [u64; CHILDREN_CAPACITY]
// keys: [K; CAPACITY]
// root_hash: Hash -- ONLY IF certified == true

const LEN_OFFSET: usize = NODE_TYPE_OFFSET + u8::SIZE;
const CHILDREN_OFFSET: usize = LEN_OFFSET + usize::SIZE;
const KEYS_OFFSET: usize = CHILDREN_OFFSET + u64::SIZE * CHILDREN_CAPACITY;

const fn root_hash_offset<K: AsFixedSizeBytes>() -> usize {
    KEYS_OFFSET + K::SIZE * CAPACITY
}

pub struct InternalBTreeNode<K> {
    ptr: u64,
    _marker_k: PhantomData<K>,
}

impl<K: StableType + AsFixedSizeBytes + Ord> InternalBTreeNode<K> {
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
            ptr: slice.as_ptr(),
            _marker_k: PhantomData::default(),
        };

        it.write_len(0);
        it.init_node_type();

        it
    }

    pub fn create(key: &K::Buf, lcp: &StablePtrBuf, rcp: &StablePtrBuf, certified: bool) -> Self {
        let slice = allocate(Self::calc_byte_size(certified));
        let mut it = Self {
            ptr: slice.as_ptr(),
            _marker_k: PhantomData::default(),
        };

        it.write_len(1);
        it.init_node_type();

        it.write_key_buf(0, key);

        it.write_child_ptr_buf(0, lcp);
        it.write_child_ptr_buf(1, rcp);

        it
    }

    #[inline]
    pub fn destroy(self) {
        let slice = SSlice::from_ptr(self.ptr, Side::Start).unwrap();
        deallocate(slice);
    }

    pub fn binary_search<Q>(&self, k: &Q, len: usize) -> Result<usize, usize>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        let mut min = 0;
        let mut max = len;
        let mut mid = (max - min) / 2;

        loop {
            let ptr = SSlice::_make_ptr_by_offset(self.ptr, KEYS_OFFSET + mid * K::SIZE);
            let key: K = unsafe { crate::mem::read_fixed_for_reference(ptr) };

            match key.borrow().cmp(k) {
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
        left_insert_last_element: Option<(&K::Buf, &StablePtrBuf)>,
        buf: &mut Vec<u8>,
    ) {
        let pk = parent.read_key_buf(parent_idx);

        if let Some((k, c)) = left_insert_last_element {
            parent.write_key_buf(parent_idx, k);
            self.insert_child_ptr_buf(0, c, self_len + 1, buf);
        } else {
            let lsk = left_sibling.read_key_buf(left_sibling_len - 1);
            parent.write_key_buf(parent_idx, &lsk);

            let lsc = left_sibling.read_child_ptr_buf(left_sibling_len);
            self.insert_child_ptr_buf(0, &lsc, self_len + 1, buf);
        };

        self.insert_key_buf(0, &pk, self_len, buf);
    }

    pub fn steal_from_right(
        &mut self,
        self_len: usize,
        right_sibling: &mut Self,
        right_sibling_len: usize,
        parent: &mut Self,
        parent_idx: usize,
        right_insert_first_element: Option<(&K::Buf, &StablePtrBuf)>,
        buf: &mut Vec<u8>,
    ) {
        let pk = parent.read_key_buf(parent_idx);

        let rsc = if let Some((k, c)) = right_insert_first_element {
            let rsc = right_sibling.read_child_ptr_buf(0);
            right_sibling.write_child_ptr_buf(0, c);

            parent.write_key_buf(parent_idx, k);

            rsc
        } else {
            let rsk = right_sibling.read_key_buf(0);
            right_sibling.remove_key_buf(0, right_sibling_len, buf);

            let rsc = right_sibling.read_child_ptr_buf(0);
            right_sibling.remove_child_ptr_buf(0, right_sibling_len + 1, buf);

            parent.write_key_buf(parent_idx, &rsk);

            rsc
        };

        self.push_key_buf(&pk, self_len);
        self.push_child_ptr_buf(&rsc, self_len + 1);
    }

    pub fn split_max_len(
        &mut self,
        buf: &mut Vec<u8>,
        certified: bool,
    ) -> (InternalBTreeNode<K>, K::Buf) {
        let mut right = InternalBTreeNode::<K>::create_empty(certified);

        self.read_many_keys_to_buf(B, MIN_LEN_AFTER_SPLIT, buf);
        right.write_many_keys_from_buf(0, buf);

        self.read_many_child_ptrs_to_buf(B, CHILDREN_MIN_LEN_AFTER_SPLIT, buf);
        right.write_many_child_ptrs_from_buf(0, buf);

        (right, self.read_key_buf(MIN_LEN_AFTER_SPLIT))
    }

    pub fn merge_min_len(&mut self, mid: &K::Buf, right: InternalBTreeNode<K>, buf: &mut Vec<u8>) {
        self.push_key_buf(mid, MIN_LEN_AFTER_SPLIT);

        right.read_many_keys_to_buf(0, MIN_LEN_AFTER_SPLIT, buf);
        self.write_many_keys_from_buf(B, buf);

        right.read_many_child_ptrs_to_buf(0, CHILDREN_MIN_LEN_AFTER_SPLIT, buf);
        self.write_many_child_ptrs_from_buf(B, buf);

        right.destroy();
    }

    #[inline]
    pub fn push_key_buf(&mut self, key: &K::Buf, len: usize) {
        self.write_key_buf(len, key);
    }

    pub fn insert_key_buf(&mut self, idx: usize, key: &K::Buf, len: usize, buf: &mut Vec<u8>) {
        if idx == len {
            self.push_key_buf(key, len);
            return;
        }

        self.read_many_keys_to_buf(idx, len - idx, buf);
        self.write_many_keys_from_buf(idx + 1, buf);

        self.write_key_buf(idx, key);
    }

    pub fn remove_key_buf(&mut self, idx: usize, len: usize, buf: &mut Vec<u8>) {
        if idx == len - 1 {
            return;
        }

        self.read_many_keys_to_buf(idx + 1, len - idx - 1, buf);
        self.write_many_keys_from_buf(idx, buf);
    }

    #[inline]
    pub fn push_child_ptr_buf(&mut self, ptr: &StablePtrBuf, children_len: usize) {
        self.write_child_ptr_buf(children_len, ptr);
    }

    pub fn insert_child_ptr_buf(
        &mut self,
        idx: usize,
        ptr_buf: &StablePtrBuf,
        children_len: usize,
        buf: &mut Vec<u8>,
    ) {
        if idx == children_len {
            self.push_child_ptr_buf(ptr_buf, children_len);
            return;
        }

        self.read_many_child_ptrs_to_buf(idx, children_len - idx, buf);
        self.write_many_child_ptrs_from_buf(idx + 1, buf);

        self.write_child_ptr_buf(idx, ptr_buf);
    }

    pub fn remove_child_ptr_buf(&mut self, idx: usize, children_len: usize, buf: &mut Vec<u8>) {
        if idx == children_len - 1 {
            return;
        }

        self.read_many_child_ptrs_to_buf(idx + 1, children_len - idx - 1, buf);
        self.write_many_child_ptrs_from_buf(idx, buf);
    }

    pub fn read_left_sibling<T: IBTreeNode>(&self, idx: usize) -> Option<T> {
        if idx == 0 {
            return None;
        }

        let left_sibling_ptr = StablePtr::from_fixed_size_bytes(&self.read_child_ptr_buf(idx - 1));

        unsafe { Some(T::from_ptr(left_sibling_ptr)) }
    }

    pub fn read_right_sibling<T: IBTreeNode>(&self, idx: usize, len: usize) -> Option<T> {
        if idx == len {
            return None;
        }

        let right_sibling_ptr = StablePtr::from_fixed_size_bytes(&self.read_child_ptr_buf(idx + 1));

        unsafe { Some(T::from_ptr(right_sibling_ptr)) }
    }

    #[inline]
    pub fn read_key_buf(&self, idx: usize) -> K::Buf {
        let mut b = K::Buf::new(K::SIZE);
        let ptr = SSlice::_make_ptr_by_offset(self.ptr, KEYS_OFFSET + idx * K::SIZE);

        unsafe { crate::mem::read_bytes(ptr, b._deref_mut()) }

        b
    }

    #[inline]
    fn read_many_keys_to_buf(&self, from_idx: usize, len: usize, buf: &mut Vec<u8>) {
        buf.resize(len * K::SIZE, 0);
        let ptr = SSlice::_make_ptr_by_offset(self.ptr, KEYS_OFFSET + from_idx * K::SIZE);

        unsafe { crate::mem::read_bytes(ptr, buf) }
    }

    #[inline]
    pub fn read_child_ptr_buf(&self, idx: usize) -> StablePtrBuf {
        let mut b = stable_ptr_buf();
        let ptr = SSlice::_make_ptr_by_offset(self.ptr, CHILDREN_OFFSET + idx * u64::SIZE);

        unsafe { crate::mem::read_bytes(ptr, b._deref_mut()) };

        b
    }

    #[inline]
    fn read_many_child_ptrs_to_buf(&self, from_idx: usize, len: usize, buf: &mut Vec<u8>) {
        buf.resize(len * u64::SIZE, 0);
        let ptr = SSlice::_make_ptr_by_offset(self.ptr, CHILDREN_OFFSET + from_idx * u64::SIZE);

        unsafe { crate::mem::read_bytes(ptr, buf) };
    }

    #[inline]
    pub fn write_key_buf(&mut self, idx: usize, key: &K::Buf) {
        let ptr = SSlice::_make_ptr_by_offset(self.ptr, KEYS_OFFSET + idx * K::SIZE);
        unsafe { crate::mem::write_bytes(ptr, key._deref()) };
    }

    #[inline]
    fn write_many_keys_from_buf(&mut self, from_idx: usize, buf: &Vec<u8>) {
        let ptr = SSlice::_make_ptr_by_offset(self.ptr, KEYS_OFFSET + from_idx * K::SIZE);

        unsafe { crate::mem::write_bytes(ptr, buf) };
    }

    #[inline]
    pub fn write_child_ptr_buf(&mut self, idx: usize, child_ptr: &StablePtrBuf) {
        let ptr = SSlice::_make_ptr_by_offset(self.ptr, CHILDREN_OFFSET + idx * u64::SIZE);

        unsafe { crate::mem::write_bytes(ptr, child_ptr) };
    }

    #[inline]
    fn write_many_child_ptrs_from_buf(&mut self, from_idx: usize, buf: &Vec<u8>) {
        let ptr = SSlice::_make_ptr_by_offset(self.ptr, CHILDREN_OFFSET + from_idx * u64::SIZE);

        unsafe { crate::mem::write_bytes(ptr, buf) };
    }

    #[inline]
    pub fn write_root_hash(&mut self, root_hash: &Hash, certified: bool) {
        debug_assert!(certified);

        let ptr = SSlice::_make_ptr_by_offset(self.ptr, root_hash_offset::<K>());
        unsafe { crate::mem::write_bytes(ptr, root_hash) };
    }

    #[inline]
    pub fn read_root_hash(&self, certified: bool) -> Hash {
        debug_assert!(certified);

        let mut buf = EMPTY_HASH;
        let ptr = SSlice::_make_ptr_by_offset(self.ptr, root_hash_offset::<K>());
        unsafe { crate::mem::read_bytes(ptr, &mut buf) };

        buf
    }

    #[inline]
    pub fn write_len(&mut self, mut len: usize) {
        let ptr = SSlice::_make_ptr_by_offset(self.ptr, LEN_OFFSET);

        unsafe { crate::mem::write_and_own_fixed(ptr, &mut len) };
    }

    #[inline]
    pub fn read_len(&self) -> usize {
        let ptr = SSlice::_make_ptr_by_offset(self.ptr, LEN_OFFSET);

        unsafe { crate::mem::read_fixed_for_reference(ptr) }
    }

    #[inline]
    fn init_node_type(&mut self) {
        let ptr = SSlice::_make_ptr_by_offset(self.ptr, NODE_TYPE_OFFSET);

        unsafe { crate::mem::write_and_own_fixed(ptr, &mut NODE_TYPE_INTERNAL) };
    }
}

impl<K: StableType + AsFixedSizeBytes + AsHashableBytes + Ord> InternalBTreeNode<K> {
    #[inline]
    pub fn read_child_root_hash<V: StableType + AsFixedSizeBytes + AsHashTree>(
        &self,
        idx: usize,
        certified: bool,
    ) -> Hash {
        debug_assert!(certified);

        let ptr = StablePtr::from_fixed_size_bytes(&self.read_child_ptr_buf(idx));
        let child = BTreeNode::<K, V>::from_ptr(ptr);

        match child {
            BTreeNode::Internal(n) => n.root_hash(),
            BTreeNode::Leaf(n) => n.root_hash(),
        }
    }
}

impl<K> IBTreeNode for InternalBTreeNode<K> {
    #[inline]
    unsafe fn from_ptr(ptr: StablePtr) -> Self {
        Self {
            ptr,
            _marker_k: PhantomData::default(),
        }
    }

    #[inline]
    fn as_ptr(&self) -> StablePtr {
        self.ptr
    }

    #[inline]
    unsafe fn copy(&self) -> Self {
        Self::from_ptr(self.ptr)
    }
}

impl<K: StableType + AsFixedSizeBytes + Ord + Debug> InternalBTreeNode<K> {
    pub fn to_string(&self) -> String {
        let mut result = format!(
            "InternalBTreeNode(&{}, {})[",
            self.as_ptr(),
            self.read_len()
        );
        for i in 0..self.read_len() {
            result += &format!(
                "*({}), ",
                StablePtr::from_fixed_size_bytes(&self.read_child_ptr_buf(i))
            );
            result += &format!(
                "{:?}, ",
                K::from_fixed_size_bytes(self.read_key_buf(i)._deref())
            );
        }

        result += &format!(
            "*({})]",
            StablePtr::from_fixed_size_bytes(&self.read_child_ptr_buf(self.read_len()))
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
    use crate::encoding::AsFixedSizeBytes;
    use crate::{init_allocator, stable, stable_memory_init};

    #[test]
    fn works_fine() {
        stable::clear();
        stable_memory_init();

        let mut node = InternalBTreeNode::<u64>::create_empty(false);
        let mut buf = Vec::default();

        for i in 0..CAPACITY {
            node.push_key_buf(&(i as u64).as_new_fixed_size_bytes(), i);
        }

        node.write_len(CAPACITY);
        println!("{}", node.to_string());
        println!();

        for i in 0..CAPACITY {
            let k = node.read_key_buf(CAPACITY - i - 1);
            assert_eq!(k, ((CAPACITY - i - 1) as u64).as_new_fixed_size_bytes());
        }

        for i in 0..CAPACITY {
            node.insert_key_buf(0, &(i as u64).as_new_fixed_size_bytes(), i, &mut buf);
        }

        for i in 0..CAPACITY {
            let k = node.read_key_buf(i);
            node.remove_key_buf(i, CAPACITY, &mut buf);
            assert_eq!(k, ((CAPACITY - i - 1) as u64).as_new_fixed_size_bytes());

            node.insert_key_buf(i, &k, CAPACITY - 1, &mut buf);
            node.push_child_ptr_buf(&1u64.as_new_fixed_size_bytes(), i);
        }

        node.push_child_ptr_buf(&1u64.as_new_fixed_size_bytes(), CAPACITY);

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
            let k = node.read_key_buf(i);
            assert_eq!(k, ((CAPACITY - i - 1) as u64).as_new_fixed_size_bytes());

            let c = node.read_child_ptr_buf(i);
            assert_eq!(c, 1u64.as_new_fixed_size_bytes());
        }

        let c = node.read_child_ptr_buf(MIN_LEN_AFTER_SPLIT);
        assert_eq!(c, 1u64.as_new_fixed_size_bytes());

        for i in 0..right.read_len() {
            let k = right.read_key_buf(i);
            assert_eq!(k, ((CAPACITY - B - i - 1) as u64).as_new_fixed_size_bytes());

            let c = right.read_child_ptr_buf(i);
            assert_eq!(c, 1u64.as_new_fixed_size_bytes());
        }

        let c = right.read_child_ptr_buf(CHILDREN_MIN_LEN_AFTER_SPLIT - 1);
        assert_eq!(c, 1u64.as_new_fixed_size_bytes());

        node.merge_min_len(&mid, right, &mut buf);

        node.write_len(CAPACITY);
        assert_eq!(node.read_len(), CAPACITY);

        for i in 0..node.read_len() {
            let k = node.read_key_buf(i);
            assert_eq!(k, ((CAPACITY - i - 1) as u64).as_new_fixed_size_bytes());

            let c = node.read_child_ptr_buf(i);
            assert_eq!(c, 1u64.as_new_fixed_size_bytes());
        }

        let c = node.read_child_ptr_buf(CAPACITY - 1);
        assert_eq!(c, 1u64.as_new_fixed_size_bytes());
    }
}
