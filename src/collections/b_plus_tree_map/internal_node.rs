use crate::collections::b_plus_tree_map::{
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

    pub unsafe fn from_ptr(ptr: u64) -> Self {
        Self {
            ptr,
            _marker_k: PhantomData::default(),
        }
    }

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

        // TODO: batch
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

        // TODO: batch
        it.write_len(1);
        it.init_node_type();

        it.write_key(0, key);
        it.write_child_ptr(0, lcp);
        it.write_child_ptr(1, rcp);

        it
    }

    pub fn destroy(self) {
        let slice = SSlice::from_ptr(self.ptr, Side::Start).unwrap();
        deallocate(slice);
    }

    // TODO: also return found key
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

    // TODO: optimize
    pub fn split_max_len(&mut self) -> (InternalBTreeNode<K>, [u8; K::SIZE]) {
        let mut right = InternalBTreeNode::<K>::create_empty();

        let mut self_len = CAPACITY;
        let mut right_len = 0;

        for _ in B..CAPACITY {
            right.push_key(&self.remove_key(B, self_len), right_len);

            self_len -= 1;
            right_len += 1;
        }

        let mut self_c_len = CHILDREN_CAPACITY;
        let mut right_c_len = 0;

        for _ in B..CHILDREN_CAPACITY {
            right.push_child_ptr(&self.remove_child_ptr(B, self_c_len), right_c_len);

            self_c_len -= 1;
            right_c_len += 1;
        }

        (right, self.pop_key(self_len))
    }

    // TODO: optimize
    pub fn merge_min_len(&mut self, mid: &[u8; K::SIZE], right: InternalBTreeNode<K>) {
        self.push_key(mid, MIN_LEN_AFTER_SPLIT);

        for i in 0..MIN_LEN_AFTER_SPLIT {
            self.push_key(&right.read_key(i), B + i);
        }

        for i in 0..CHILDREN_MIN_LEN_AFTER_SPLIT {
            self.push_child_ptr(&right.read_child_ptr(i), B + 1);
        }

        right.destroy();
    }

    #[inline]
    pub fn push_key(&mut self, key: &[u8; K::SIZE], len: usize) {
        self.write_key(len, key);
    }

    pub fn insert_key(&mut self, idx: usize, key: &[u8; K::SIZE], len: usize) {
        if idx == len {
            self.push_key(key, len);
            return;
        }

        for i in (idx..len).rev() {
            let k = self.read_key(i);
            self.write_key(i + 1, &k);
        }

        self.write_key(idx, key);
    }

    #[inline]
    pub fn pop_key(&mut self, len: usize) -> [u8; K::SIZE] {
        self.read_key(len - 1)
    }

    pub fn remove_key(&mut self, idx: usize, len: usize) -> [u8; K::SIZE] {
        if idx == len - 1 {
            return self.pop_key(len);
        }

        let key = self.read_key(idx);

        for i in (idx + 1)..len {
            let k = self.read_key(i);
            self.write_key(i - 1, &k)
        }

        key
    }

    #[inline]
    pub fn push_child_ptr(&mut self, ptr: &[u8; u64::SIZE], children_len: usize) {
        self.write_child_ptr(children_len, ptr);
    }

    pub fn insert_child_ptr(&mut self, idx: usize, ptr: &[u8; u64::SIZE], children_len: usize) {
        if idx == children_len {
            self.push_child_ptr(ptr, children_len);
            return;
        }

        for i in (idx..children_len).rev() {
            let p = self.read_child_ptr(i);
            self.write_child_ptr(i + 1, &p);
        }

        self.write_child_ptr(idx, ptr);
    }

    #[inline]
    pub fn pop_child_ptr(&mut self, children_len: usize) -> [u8; u64::SIZE] {
        self.read_child_ptr(children_len - 1)
    }

    pub fn remove_child_ptr(&mut self, idx: usize, children_len: usize) -> [u8; u64::SIZE] {
        if idx == children_len - 1 {
            return self.pop_child_ptr(children_len);
        }

        let ptr = self.read_child_ptr(idx);

        for i in (idx + 1)..children_len {
            let p = self.read_child_ptr(i);
            self.write_child_ptr(i - 1, &p)
        }

        ptr
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
    pub fn read_child_ptr(&self, idx: usize) -> [u8; u64::SIZE] {
        SSlice::_read_const_u8_array_of_size::<u64>(self.ptr, CHILDREN_OFFSET + idx * u64::SIZE)
    }

    #[inline]
    pub fn write_key(&mut self, idx: usize, key: &[u8; K::SIZE]) {
        SSlice::_write_bytes(self.ptr, KEYS_OFFSET + idx * K::SIZE, key);
    }

    #[inline]
    pub fn write_child_ptr(&mut self, idx: usize, ptr: &[u8; u64::SIZE]) {
        SSlice::_write_bytes(self.ptr, CHILDREN_OFFSET + idx * u64::SIZE, ptr);
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
        print!("InternalBTreeNode({})[", self.read_len());
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
    use crate::collections::b_plus_tree_map::internal_node::InternalBTreeNode;
    use crate::collections::b_plus_tree_map::{
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

        for i in 0..CAPACITY {
            node.push_key(&(i as u64).as_fixed_size_bytes(), i);
        }

        node.write_len(CAPACITY);
        node.debug_print();
        println!();

        for i in 0..CAPACITY {
            let k = node.pop_key(CAPACITY - i);
            assert_eq!(k, ((CAPACITY - i - 1) as u64).as_fixed_size_bytes());
        }

        for i in 0..CAPACITY {
            node.insert_key(0, &(i as u64).as_fixed_size_bytes(), i);
        }

        for i in 0..CAPACITY {
            let k = node.remove_key(i, CAPACITY);
            assert_eq!(k, ((CAPACITY - i - 1) as u64).as_fixed_size_bytes());

            node.insert_key(i, &k, CAPACITY - 1);
            node.push_child_ptr(&1u64.as_fixed_size_bytes(), i);
        }

        node.push_child_ptr(&1u64.as_fixed_size_bytes(), CAPACITY);

        println!("before split: ");
        node.debug_print();
        println!();

        let (mut right, mid) = node.split_max_len();

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

        node.merge_min_len(&mid, right);

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
