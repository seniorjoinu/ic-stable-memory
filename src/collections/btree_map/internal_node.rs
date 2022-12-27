use crate::collections::btree_map::{
    B, CAPACITY, CHILDREN_CAPACITY, CHILDREN_MIN_LEN_AFTER_SPLIT, MIN_LEN_AFTER_SPLIT,
    NODE_TYPE_INTERNAL, NODE_TYPE_OFFSET,
};
use crate::mem::s_slice::Side;
use crate::primitive::StableAllocated;
use crate::utils::encoding::{AsFixedSizeBytes, FixedSize};
use crate::{allocate, deallocate, SSlice};
use std::cmp::Ordering;
use std::fmt::Debug;
use std::marker::PhantomData;

// LAYOUT:
// node_type: u8
// len: usize
// children: [u64; CHILDREN_CAPACITY]
// keys: [K; CAPACITY]

const LEN_OFFSET: usize = NODE_TYPE_OFFSET + u8::SIZE;
const CHILDREN_OFFSET: usize = LEN_OFFSET + usize::SIZE;
const KEYS_OFFSET: usize = CHILDREN_OFFSET + u64::SIZE * CHILDREN_CAPACITY;

pub struct InternalBTreeNode<K> {
    ptr: u64,
    _marker_k: PhantomData<K>,
}

impl<K: StableAllocated + Ord + Eq> InternalBTreeNode<K>
where
    [(); K::SIZE]: Sized,
{
    #[inline]
    const fn calc_byte_size() -> usize {
        KEYS_OFFSET + K::SIZE * CAPACITY
    }

    #[inline]
    pub unsafe fn from_ptr(ptr: u64) -> Self {
        Self {
            ptr,
            _marker_k: PhantomData::default(),
        }
    }

    #[inline]
    pub unsafe fn copy(&self) -> Self {
        Self {
            ptr: self.ptr,
            _marker_k: PhantomData::default(),
        }
    }

    pub fn create_empty() -> Self {
        let slice = allocate(Self::calc_byte_size());
        let mut it = Self {
            ptr: slice.get_ptr(),
            _marker_k: PhantomData::default(),
        };

        it.write_len(0);
        it.init_node_type();

        it
    }

    pub fn create(key: &[u8; K::SIZE], lcp: &[u8; u64::SIZE], rcp: &[u8; u64::SIZE]) -> Self {
        let slice = allocate(Self::calc_byte_size());
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
        let slice = SSlice::from_ptr(self.ptr, Side::Start).unwrap();
        deallocate(slice);
    }

    pub fn binary_search(&self, k: &K, len: usize) -> Result<usize, usize> {
        if len == 0 {
            return Err(0);
        }

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
        left_insert_last_element: Option<(&[u8; K::SIZE], &[u8; u64::SIZE])>,
        buf: &mut Vec<u8>,
    ) {
        let pk = parent.read_key(parent_idx);

        if let Some((k, c)) = left_insert_last_element {
            parent.write_key(parent_idx, k);
            self.insert_child_ptr(0, c, self_len + 1, buf);
        } else {
            let lsk = left_sibling.read_key(left_sibling_len - 1);
            let lsc = left_sibling.read_child_ptr(left_sibling_len);

            parent.write_key(parent_idx, &lsk);
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
        right_insert_first_element: Option<(&[u8; K::SIZE], &[u8; u64::SIZE])>,
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
            let rsc = right_sibling.read_child_ptr(0);

            right_sibling.remove_key(0, right_sibling_len, buf);
            right_sibling.remove_child_ptr(0, right_sibling_len + 1, buf);

            parent.write_key(parent_idx, &rsk);

            rsc
        };

        self.push_key(&pk, self_len);
        self.push_child_ptr(&rsc, self_len + 1);
    }

    pub fn split_max_len(&mut self, buf: &mut Vec<u8>) -> (InternalBTreeNode<K>, [u8; K::SIZE]) {
        let mut right = InternalBTreeNode::<K>::create_empty();

        self.read_keys_to_buf(B, MIN_LEN_AFTER_SPLIT, buf);
        right.write_keys_from_buf(0, buf);

        self.read_child_ptrs_to_buf(B, CHILDREN_MIN_LEN_AFTER_SPLIT, buf);
        right.write_child_ptrs_from_buf(0, buf);

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
    pub fn push_child_ptr(&mut self, ptr: &[u8; u64::SIZE], children_len: usize) {
        self.write_child_ptr(children_len, ptr);
    }

    pub fn insert_child_ptr(
        &mut self,
        idx: usize,
        ptr: &[u8; u64::SIZE],
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

    #[inline]
    pub fn as_ptr(&self) -> u64 {
        self.ptr
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
    pub fn read_child_ptr(&self, idx: usize) -> [u8; u64::SIZE] {
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
    pub fn write_child_ptr(&mut self, idx: usize, ptr: &[u8; u64::SIZE]) {
        SSlice::_write_bytes(self.ptr, CHILDREN_OFFSET + idx * u64::SIZE, ptr);
    }

    #[inline]
    fn write_child_ptrs_from_buf(&mut self, from_idx: usize, buf: &Vec<u8>) {
        SSlice::_write_bytes(self.ptr, CHILDREN_OFFSET + from_idx * u64::SIZE, buf);
    }

    #[inline]
    pub fn write_len(&mut self, len: usize) {
        SSlice::_as_fixed_size_bytes_write(self.ptr, LEN_OFFSET, len)
    }

    #[inline]
    pub fn read_len(&self) -> usize {
        SSlice::_as_fixed_size_bytes_read(self.ptr, LEN_OFFSET)
    }

    #[inline]
    fn init_node_type(&mut self) {
        SSlice::_as_fixed_size_bytes_write(self.ptr, NODE_TYPE_OFFSET, NODE_TYPE_INTERNAL)
    }
}

impl<K: StableAllocated + Ord + Eq + Debug> InternalBTreeNode<K>
where
    [(); K::SIZE]: Sized,
{
    pub fn debug_print(&self) {
        print!(
            "InternalBTreeNode(&{}, {})[",
            self.as_ptr(),
            self.read_len()
        );
        for i in 0..self.read_len() {
            print!(
                "*({}), ",
                u64::from_fixed_size_bytes(&self.read_child_ptr(i))
            );
            print!("{:?}, ", K::from_fixed_size_bytes(&self.read_key(i)));
        }

        print!(
            "*({})]",
            u64::from_fixed_size_bytes(&self.read_child_ptr(self.read_len()))
        );
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

        let mut node = InternalBTreeNode::<u64>::create_empty();
        let mut buf = Vec::default();

        for i in 0..CAPACITY {
            node.push_key(&(i as u64).as_fixed_size_bytes(), i);
        }

        node.write_len(CAPACITY);
        node.debug_print();
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
        node.debug_print();
        println!();

        let (mut right, mid) = node.split_max_len(&mut buf);

        node.write_len(MIN_LEN_AFTER_SPLIT);
        right.write_len(MIN_LEN_AFTER_SPLIT);

        println!("after split: ");
        node.debug_print();
        right.debug_print();

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
