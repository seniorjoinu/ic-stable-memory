use crate::collections::b_plus_tree_map::{
    B, CAPACITY, MIN_LEN_AFTER_SPLIT, NODE_TYPE_LEAF, NODE_TYPE_OFFSET,
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
// prev, next: u64
// len: usize,
// keys: [K; CAPACITY]
// values: [V; CAPACITY]

const PREV_OFFSET: usize = NODE_TYPE_OFFSET + u8::SIZE;
const NEXT_OFFSET: usize = PREV_OFFSET + u64::SIZE;
const LEN_OFFSET: usize = NEXT_OFFSET + u64::SIZE;
const KEYS_OFFSET: usize = LEN_OFFSET + usize::SIZE;

const fn values_offset<K: FixedSize>() -> usize {
    KEYS_OFFSET + K::SIZE * CAPACITY
}

pub struct LeafBTreeNode<K, V> {
    ptr: u64,
    _marker_k: PhantomData<K>,
    _marker_v: PhantomData<V>,
}

impl<K: StableAllocated + Ord + Eq, V: StableAllocated> LeafBTreeNode<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    #[inline]
    const fn calc_size() -> usize {
        values_offset::<K>() + V::SIZE * CAPACITY
    }

    pub unsafe fn from_ptr(ptr: u64) -> Self {
        Self {
            ptr,
            _marker_k: PhantomData::default(),
            _marker_v: PhantomData::default(),
        }
    }

    pub unsafe fn copy(&self) -> Self {
        Self {
            ptr: self.ptr,
            _marker_k: PhantomData::default(),
            _marker_v: PhantomData::default(),
        }
    }

    pub fn create() -> Self {
        let slice = allocate(Self::calc_size());
        let mut it = Self {
            ptr: slice.get_ptr(),
            _marker_k: PhantomData::default(),
            _marker_v: PhantomData::default(),
        };

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

    // TODO: optimize
    #[allow(clippy::explicit_counter_loop)]
    pub fn split_max_len(&mut self, right_biased: bool) -> Self {
        let mut right = Self::create();

        let mut len_self = CAPACITY;
        let mut len_right = 0usize;

        let min_idx = if right_biased { MIN_LEN_AFTER_SPLIT } else { B };

        // TODO: optimize - just copy and set len
        for _ in min_idx..CAPACITY {
            let k = self.pop_key(len_self);
            let v = self.pop_value(len_self);

            len_self -= 1;

            right.insert_key(0, &k, len_right);
            right.insert_value(0, &v, len_right);

            len_right += 1;
        }

        let self_next = self.read_next();
        self.write_next(&right.ptr.as_fixed_size_bytes());

        right.write_prev(&self.ptr.as_fixed_size_bytes());
        right.write_next(&self_next);

        right
    }

    // TODO: optimize
    pub fn merge_min_len(&mut self, right: Self) {
        for i in 0..MIN_LEN_AFTER_SPLIT {
            let k = right.read_key(i);
            let v = right.read_value(i);

            self.push_key(&k, MIN_LEN_AFTER_SPLIT + i);
            self.push_value(&v, MIN_LEN_AFTER_SPLIT + i);
        }

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

        for i in idx..(len - 1) {
            let k = self.read_key(i + 1);
            self.write_key(i, &k);
        }

        key
    }

    #[inline]
    pub fn push_value(&mut self, value: &[u8; V::SIZE], len: usize) {
        self.write_value(len, value);
    }

    pub fn insert_value(&mut self, idx: usize, value: &[u8; V::SIZE], len: usize) {
        if idx == len {
            self.push_value(value, len);
            return;
        }

        for i in (idx..len).rev() {
            let v = self.read_value(i);
            self.write_value(i + 1, &v);
        }

        self.write_value(idx, value);
    }

    #[inline]
    pub fn pop_value(&mut self, len: usize) -> [u8; V::SIZE] {
        self.read_value(len - 1)
    }

    pub fn remove_value(&mut self, idx: usize, len: usize) -> [u8; V::SIZE] {
        if idx == len - 1 {
            return self.pop_value(len);
        }

        let value = self.read_value(idx);

        for i in idx..(len - 1) {
            let v = self.read_value(i + 1);
            self.write_value(i, &v);
        }

        value
    }

    #[inline]
    pub fn as_ptr(&self) -> u64 {
        self.ptr
    }

    #[inline]
    pub fn write_key(&mut self, idx: usize, key: &[u8; K::SIZE]) {
        SSlice::_write_bytes(self.ptr, KEYS_OFFSET + idx * K::SIZE, key);
    }

    #[inline]
    pub fn read_key(&self, idx: usize) -> [u8; K::SIZE] {
        SSlice::_read_const_u8_array_of_size::<K>(self.ptr, KEYS_OFFSET + idx * K::SIZE)
    }

    #[inline]
    pub fn write_value(&mut self, idx: usize, value: &[u8; V::SIZE]) {
        SSlice::_write_bytes(self.ptr, values_offset::<K>() + idx * V::SIZE, value);
    }

    #[inline]
    pub fn read_value(&self, idx: usize) -> [u8; V::SIZE] {
        SSlice::_read_const_u8_array_of_size::<V>(self.ptr, values_offset::<K>() + idx * V::SIZE)
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
    pub fn write_len(&mut self, len: usize) {
        SSlice::_as_fixed_size_bytes_write(self.ptr, LEN_OFFSET, len);
    }

    #[inline]
    pub fn read_len(&self) -> usize {
        SSlice::_as_fixed_size_bytes_read(self.ptr, LEN_OFFSET)
    }

    #[inline]
    fn init_node_type(&mut self) {
        SSlice::_as_fixed_size_bytes_write(self.ptr, NODE_TYPE_OFFSET, NODE_TYPE_LEAF);
    }
}

impl<K: StableAllocated + Ord + Eq + Debug, V: StableAllocated + Debug> LeafBTreeNode<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    pub fn debug_print(&self) {
        print!("LeafBTreeNode({})[", self.read_len());
        for i in 0..self.read_len() {
            print!("({:?}, ", K::from_fixed_size_bytes(&self.read_key(i)));
            print!("{:?})", V::from_fixed_size_bytes(&self.read_value(i)));

            if i < self.read_len() - 1 {
                print!(", ");
            }
        }

        print!("]");
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::b_plus_tree_map::leaf_node::LeafBTreeNode;
    use crate::collections::b_plus_tree_map::{B, CAPACITY, MIN_LEN_AFTER_SPLIT};
    use crate::utils::encoding::AsFixedSizeBytes;
    use crate::{init_allocator, stable};

    #[test]
    fn works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut node = LeafBTreeNode::<u64, u64>::create();

        for i in 0..CAPACITY {
            node.push_key(&(i as u64).as_fixed_size_bytes(), i);
            node.push_value(&(i as u64).as_fixed_size_bytes(), i);
        }

        for i in 0..CAPACITY {
            let k = node.pop_key(CAPACITY - i);
            let v = node.pop_value(CAPACITY - i);

            assert_eq!(k, ((CAPACITY - i - 1) as u64).as_fixed_size_bytes());
            assert_eq!(v, ((CAPACITY - i - 1) as u64).as_fixed_size_bytes());
        }

        for i in (0..CAPACITY).rev() {
            node.insert_key(0, &(i as u64).as_fixed_size_bytes(), (CAPACITY - i - 1));
            node.insert_value(0, &(i as u64).as_fixed_size_bytes(), (CAPACITY - i - 1));
        }

        node.write_len(CAPACITY);
        node.debug_print();

        for i in 0..CAPACITY {
            let k = node.remove_key(i, CAPACITY);
            let v = node.remove_value(i, CAPACITY);

            assert_eq!(k, (i as u64).as_fixed_size_bytes());
            assert_eq!(v, (i as u64).as_fixed_size_bytes());

            node.insert_key(i, &k, CAPACITY - 1);
            node.insert_value(i, &v, CAPACITY - 1);
        }

        let right = node.split_max_len(true);

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

        node.merge_min_len(right);

        for i in 0..CAPACITY {
            let k = node.read_key(i);
            let v = node.read_value(i);

            assert_eq!(k, (i as u64).as_fixed_size_bytes());
            assert_eq!(v, (i as u64).as_fixed_size_bytes());
        }
    }
}
