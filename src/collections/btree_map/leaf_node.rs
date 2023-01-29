use crate::collections::btree_map::internal_node::InternalBTreeNode;
use crate::collections::btree_map::{
    IBTreeNode, B, CAPACITY, MIN_LEN_AFTER_SPLIT, NODE_TYPE_LEAF, NODE_TYPE_OFFSET,
};
use crate::mem::s_slice::Side;
use crate::primitive::StableAllocated;
use crate::utils::certification::Hash;
use crate::utils::encoding::{AsFixedSizeBytes, FixedSize};
use crate::{allocate, deallocate, isoprint, SSlice};
use std::cmp::Ordering;
use std::fmt::Debug;
use std::marker::PhantomData;

// LAYOUT:
// node_type: u8
// prev, next: u64
// len: usize,
// keys: [K; CAPACITY]
// values: [V; CAPACITY]
// root_hash: Hash -- only when certified == true

const PREV_OFFSET: usize = NODE_TYPE_OFFSET + u8::SIZE;
const NEXT_OFFSET: usize = PREV_OFFSET + u64::SIZE;
const LEN_OFFSET: usize = NEXT_OFFSET + u64::SIZE;
const KEYS_OFFSET: usize = LEN_OFFSET + usize::SIZE;

const fn values_offset<K: FixedSize>() -> usize {
    KEYS_OFFSET + K::SIZE * CAPACITY
}
const fn root_hash_offset<K: FixedSize, V: FixedSize>() -> usize {
    values_offset::<K>() + V::SIZE * CAPACITY
}

pub struct LeafBTreeNode<K, V> {
    ptr: u64,
    _marker_k: PhantomData<K>,
    _marker_v: PhantomData<V>,
}

impl<K: StableAllocated + Ord, V: StableAllocated> LeafBTreeNode<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    #[inline]
    const fn calc_size(certified: bool) -> usize {
        let mut size = root_hash_offset::<K, V>();

        if certified {
            size += Hash::SIZE;
        }

        size
    }

    pub fn create(certified: bool) -> Self {
        let slice = allocate(Self::calc_size(certified));
        let mut it = unsafe { Self::from_ptr(slice.get_ptr()) };

        it.init_node_type();
        it.write_len(0);

        let b = u64::_u8_arr_of_size();
        it.write_prev(&b);
        it.write_next(&b);

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
        parent: &mut InternalBTreeNode<K>,
        parent_idx: usize,
        left_insert_last_element: Option<(&[u8; K::SIZE], &[u8; V::SIZE])>,
        buf: &mut Vec<u8>,
    ) {
        if let Some((k, v)) = left_insert_last_element {
            parent.write_key(parent_idx, &k);

            self.insert_key(0, k, self_len, buf);
            self.insert_value(0, v, self_len, buf);
        } else {
            let replace_key = left_sibling.read_key(left_sibling_len - 1);
            let replace_value = left_sibling.read_value(left_sibling_len - 1);

            parent.write_key(parent_idx, &replace_key);

            self.insert_key(0, &replace_key, self_len, buf);
            self.insert_value(0, &replace_value, self_len, buf);
        }
    }

    pub fn steal_from_right(
        &mut self,
        self_len: usize,
        right_sibling: &mut Self,
        right_sibling_len: usize,
        parent: &mut InternalBTreeNode<K>,
        parent_idx: usize,
        right_insert_first_element: Option<(&[u8; K::SIZE], &[u8; V::SIZE])>,
        buf: &mut Vec<u8>,
    ) {
        let replace_key = right_sibling.read_key(0);
        let replace_value = right_sibling.read_value(0);

        if let Some((k, v)) = right_insert_first_element {
            right_sibling.write_key(0, k);
            right_sibling.write_value(0, v);

            parent.write_key(parent_idx, k);
        } else {
            right_sibling.remove_key(0, right_sibling_len, buf);
            right_sibling.remove_value(0, right_sibling_len, buf);

            parent.write_key(parent_idx, &right_sibling.read_key(0));
        };

        self.push_key(&replace_key, self_len);
        self.push_value(&replace_value, self_len);
    }

    #[allow(clippy::explicit_counter_loop)]
    pub fn split_max_len(
        &mut self,
        right_biased: bool,
        buf: &mut Vec<u8>,
        certified: bool,
    ) -> Self {
        let mut right = Self::create(certified);

        let min_idx = if right_biased { MIN_LEN_AFTER_SPLIT } else { B };

        self.read_keys_to_buf(min_idx, CAPACITY - min_idx, buf);
        right.write_keys_from_buf(0, buf);

        self.read_values_to_buf(min_idx, CAPACITY - min_idx, buf);
        right.write_values_from_buf(0, buf);

        let self_next = self.read_next();
        self.write_next(&right.ptr.as_fixed_size_bytes());

        right.write_prev(&self.ptr.as_fixed_size_bytes());
        right.write_next(&self_next);

        right
    }

    pub fn merge_min_len(&mut self, right: Self, buf: &mut Vec<u8>) {
        right.read_keys_to_buf(0, MIN_LEN_AFTER_SPLIT, buf);
        self.write_keys_from_buf(MIN_LEN_AFTER_SPLIT, buf);

        right.read_values_to_buf(0, MIN_LEN_AFTER_SPLIT, buf);
        self.write_values_from_buf(MIN_LEN_AFTER_SPLIT, buf);

        let right_next_buf = right.read_next();
        self.write_next(&right_next_buf);

        if right_next_buf != [0u8; u64::SIZE] {
            let right_next_ptr = u64::from_fixed_size_bytes(&right_next_buf);
            let mut right_next = unsafe { Self::from_ptr(right_next_ptr) };

            right_next.write_prev(&self.ptr.as_fixed_size_bytes());
        }

        right.destroy();
    }

    #[inline]
    pub fn remove_by_idx(&mut self, idx: usize, len: usize, buf: &mut Vec<u8>) -> V {
        let mut k = K::from_fixed_size_bytes(&self.read_key(idx));
        let mut v = V::from_fixed_size_bytes(&self.read_value(idx));

        self.remove_key(idx, len, buf);
        self.remove_value(idx, len, buf);

        k.remove_from_stable();
        v.remove_from_stable();

        v
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
    pub fn push_value(&mut self, value: &[u8; V::SIZE], len: usize) {
        self.write_value(len, value);
    }

    pub fn insert_value(
        &mut self,
        idx: usize,
        value: &[u8; V::SIZE],
        len: usize,
        buf: &mut Vec<u8>,
    ) {
        if idx == len {
            self.push_value(value, len);
            return;
        }

        self.read_values_to_buf(idx, len - idx, buf);
        self.write_values_from_buf(idx + 1, buf);

        self.write_value(idx, value);
    }

    pub fn remove_value(&mut self, idx: usize, len: usize, buf: &mut Vec<u8>) {
        if idx == len - 1 {
            return;
        }

        self.read_values_to_buf(idx + 1, len - idx - 1, buf);
        self.write_values_from_buf(idx, buf);
    }

    #[inline]
    pub fn read_entry(&self, idx: usize) -> (K, V) {
        (
            K::from_fixed_size_bytes(&self.read_key(idx)),
            V::from_fixed_size_bytes(&self.read_value(idx)),
        )
    }

    #[inline]
    pub fn write_key(&mut self, idx: usize, key: &[u8; K::SIZE]) {
        SSlice::_write_bytes(self.ptr, KEYS_OFFSET + idx * K::SIZE, key);
    }

    #[inline]
    fn write_keys_from_buf(&self, from_idx: usize, buf: &Vec<u8>) {
        SSlice::_write_bytes(self.ptr, KEYS_OFFSET + from_idx * K::SIZE, buf);
    }

    #[inline]
    pub fn get_key_ptr(&self, idx: usize) -> u64 {
        self.ptr + (KEYS_OFFSET + idx * K::SIZE) as u64
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
    pub fn write_value(&mut self, idx: usize, value: &[u8; V::SIZE]) {
        SSlice::_write_bytes(self.ptr, values_offset::<K>() + idx * V::SIZE, value);
    }

    #[inline]
    fn write_values_from_buf(&self, from_idx: usize, buf: &Vec<u8>) {
        SSlice::_write_bytes(self.ptr, values_offset::<K>() + from_idx * V::SIZE, buf);
    }

    #[inline]
    pub fn get_value_ptr(&self, idx: usize) -> u64 {
        self.ptr + (values_offset::<K>() + idx * V::SIZE) as u64
    }

    #[inline]
    pub fn read_value(&self, idx: usize) -> [u8; V::SIZE] {
        SSlice::_read_const_u8_array_of_size::<V>(self.ptr, values_offset::<K>() + idx * V::SIZE)
    }

    #[inline]
    fn read_values_to_buf(&self, from_idx: usize, len: usize, buf: &mut Vec<u8>) {
        buf.resize(len * V::SIZE, 0);
        SSlice::_read_bytes(self.ptr, values_offset::<K>() + from_idx * V::SIZE, buf);
    }

    #[inline]
    pub fn write_prev(&mut self, prev: &[u8; u64::SIZE]) {
        SSlice::_write_bytes(self.ptr, PREV_OFFSET, prev);
    }

    #[inline]
    pub fn read_prev(&self) -> [u8; u64::SIZE] {
        SSlice::_read_const_u8_array_of_size::<u64>(self.ptr, PREV_OFFSET)
    }

    #[inline]
    pub fn write_next(&mut self, next: &[u8; u64::SIZE]) {
        SSlice::_write_bytes(self.ptr, NEXT_OFFSET, next);
    }

    #[inline]
    pub fn read_next(&self) -> [u8; u64::SIZE] {
        SSlice::_read_const_u8_array_of_size::<u64>(self.ptr, NEXT_OFFSET)
    }

    #[inline]
    pub fn write_root_hash(&mut self, root_hash: &Hash, certified: bool) {
        debug_assert!(certified);
        SSlice::_write_bytes(self.ptr, root_hash_offset::<K, V>(), root_hash);
    }

    #[inline]
    pub fn read_root_hash(&self, certified: bool) -> Hash {
        debug_assert!(certified);

        SSlice::_read_const_u8_array_of_size::<Hash>(self.ptr, root_hash_offset::<K, V>())
    }

    #[inline]
    pub fn write_len(&mut self, len: usize) {
        SSlice::_as_fixed_size_bytes_write::<usize>(self.ptr, LEN_OFFSET, len);
    }

    #[inline]
    pub fn read_len(&self) -> usize {
        SSlice::_as_fixed_size_bytes_read::<usize>(self.ptr, LEN_OFFSET)
    }

    #[inline]
    fn init_node_type(&mut self) {
        SSlice::_as_fixed_size_bytes_write::<u8>(self.ptr, NODE_TYPE_OFFSET, NODE_TYPE_LEAF);
    }
}

impl<K, V> IBTreeNode for LeafBTreeNode<K, V> {
    #[inline]
    unsafe fn from_ptr(ptr: u64) -> Self {
        Self {
            ptr,
            _marker_k: PhantomData::default(),
            _marker_v: PhantomData::default(),
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
}

impl<K: StableAllocated + Ord + Debug, V: StableAllocated + Debug> LeafBTreeNode<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    pub fn to_string(&self) -> String {
        let mut result = format!("LeafBTreeNode(&{}, {})[", self.as_ptr(), self.read_len());
        for i in 0..self.read_len() {
            result += &format!("({:?}, ", K::from_fixed_size_bytes(&self.read_key(i)));
            result += &format!("{:?})", V::from_fixed_size_bytes(&self.read_value(i)));

            if i < self.read_len() - 1 {
                result += ", ";
            }
        }

        result += "]";

        result
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::btree_map::leaf_node::LeafBTreeNode;
    use crate::collections::btree_map::{B, CAPACITY, MIN_LEN_AFTER_SPLIT};
    use crate::utils::encoding::AsFixedSizeBytes;
    use crate::{init_allocator, stable};

    #[test]
    fn works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut node = LeafBTreeNode::<u64, u64>::create(false);
        let mut buf = Vec::default();

        for i in 0..CAPACITY {
            node.push_key(&(i as u64).as_fixed_size_bytes(), i);
            node.push_value(&(i as u64).as_fixed_size_bytes(), i);
        }

        for i in 0..CAPACITY {
            let k = node.read_key(CAPACITY - i - 1);
            let v = node.read_value(CAPACITY - i - 1);

            assert_eq!(k, ((CAPACITY - i - 1) as u64).as_fixed_size_bytes());
            assert_eq!(v, ((CAPACITY - i - 1) as u64).as_fixed_size_bytes());
        }

        for i in (0..CAPACITY).rev() {
            node.insert_key(
                0,
                &(i as u64).as_fixed_size_bytes(),
                CAPACITY - i - 1,
                &mut buf,
            );
            node.insert_value(
                0,
                &(i as u64).as_fixed_size_bytes(),
                CAPACITY - i - 1,
                &mut buf,
            );
        }

        node.write_len(CAPACITY);
        println!("{}", node.to_string());

        for i in 0..CAPACITY {
            let k = node.read_key(i);
            let v = node.read_value(i);

            node.remove_key(i, CAPACITY, &mut buf);
            node.remove_value(i, CAPACITY, &mut buf);

            assert_eq!(k, (i as u64).as_fixed_size_bytes());
            assert_eq!(v, (i as u64).as_fixed_size_bytes());

            node.insert_key(i, &k, CAPACITY - 1, &mut buf);
            node.insert_value(i, &v, CAPACITY - 1, &mut buf);
        }

        let right = node.split_max_len(true, &mut buf, false);

        for i in 0..MIN_LEN_AFTER_SPLIT {
            let k = node.read_key(i);
            let v = node.read_value(i);

            assert_eq!(k, (i as u64).as_fixed_size_bytes());
            assert_eq!(v, (i as u64).as_fixed_size_bytes());
        }

        for i in 0..B {
            let k = right.read_key(i);
            let v = right.read_value(i);

            assert_eq!(k, ((i + MIN_LEN_AFTER_SPLIT) as u64).as_fixed_size_bytes());
            assert_eq!(v, ((i + MIN_LEN_AFTER_SPLIT) as u64).as_fixed_size_bytes());
        }

        node.merge_min_len(right, &mut buf);

        for i in 0..CAPACITY {
            let k = node.read_key(i);
            let v = node.read_value(i);

            assert_eq!(k, (i as u64).as_fixed_size_bytes());
            assert_eq!(v, (i as u64).as_fixed_size_bytes());
        }
    }
}
