use crate::collections::btree_map::internal_node::InternalBTreeNode;
use crate::collections::btree_map::{
    IBTreeNode, B, CAPACITY, MIN_LEN_AFTER_SPLIT, NODE_TYPE_LEAF, NODE_TYPE_OFFSET,
};
use crate::encoding::{AsFixedSizeBytes, Buffer};
use crate::mem::{stable_ptr_buf, StablePtrBuf};
use crate::primitive::s_ref::SRef;
use crate::primitive::s_ref_mut::SRefMut;
use crate::primitive::StableType;
use crate::utils::certification::{Hash, EMPTY_HASH};
use crate::{allocate, deallocate, OutOfMemory, SSlice};
use std::borrow::Borrow;
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

const PREV_OFFSET: u64 = NODE_TYPE_OFFSET + u8::SIZE as u64;
const NEXT_OFFSET: u64 = PREV_OFFSET + u64::SIZE as u64;
const LEN_OFFSET: u64 = NEXT_OFFSET + u64::SIZE as u64;
const KEYS_OFFSET: u64 = LEN_OFFSET + usize::SIZE as u64;

const fn values_offset<K: AsFixedSizeBytes>() -> u64 {
    KEYS_OFFSET + (K::SIZE * CAPACITY) as u64
}
const fn root_hash_offset<K: AsFixedSizeBytes, V: AsFixedSizeBytes>() -> u64 {
    values_offset::<K>() + (V::SIZE * CAPACITY) as u64
}

pub struct LeafBTreeNode<K, V> {
    ptr: u64,
    _marker_k: PhantomData<K>,
    _marker_v: PhantomData<V>,
}

impl<K: StableType + AsFixedSizeBytes + Ord, V: StableType + AsFixedSizeBytes> LeafBTreeNode<K, V> {
    #[inline]
    pub const fn calc_size_bytes(certified: bool) -> u64 {
        let mut size = root_hash_offset::<K, V>();

        if certified {
            size += Hash::SIZE as u64;
        }

        size
    }

    pub fn create(certified: bool) -> Result<Self, OutOfMemory> {
        let slice = unsafe { allocate(Self::calc_size_bytes(certified))? };
        let mut it = unsafe { Self::from_ptr(slice.as_ptr()) };

        it.init_node_type();
        it.write_len(0);

        let b = <u64 as AsFixedSizeBytes>::Buf::new(u64::SIZE);
        it.write_prev_ptr_buf(&b);
        it.write_next_ptr_buf(&b);

        Ok(it)
    }

    #[inline]
    pub fn destroy(self) {
        let slice = unsafe { SSlice::from_ptr(self.ptr).unwrap() };
        deallocate(slice);
    }

    pub fn binary_search<Q>(&self, k: &Q, len: usize) -> Result<usize, usize>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        if len == 0 {
            return Err(0);
        }

        let mut min = 0;
        let mut max = len;
        let mut mid = (max - min) / 2;

        loop {
            let ptr = SSlice::_offset(self.ptr, KEYS_OFFSET + (mid * K::SIZE) as u64);
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
        parent: &mut InternalBTreeNode<K>,
        parent_idx: usize,
        left_insert_last_element: Option<(&K::Buf, &V::Buf)>,
        buf: &mut Vec<u8>,
    ) {
        if let Some((k, v)) = left_insert_last_element {
            parent.write_key_buf(parent_idx, k);

            self.insert_key_buf(0, k, self_len, buf);
            self.insert_value_buf(0, v, self_len, buf);
        } else {
            let replace_key = left_sibling.read_key_buf(left_sibling_len - 1);
            let replace_value = left_sibling.read_value_buf(left_sibling_len - 1);

            parent.write_key_buf(parent_idx, &replace_key);

            self.insert_key_buf(0, &replace_key, self_len, buf);
            self.insert_value_buf(0, &replace_value, self_len, buf);
        }
    }

    pub fn steal_from_right(
        &mut self,
        self_len: usize,
        right_sibling: &mut Self,
        right_sibling_len: usize,
        parent: &mut InternalBTreeNode<K>,
        parent_idx: usize,
        right_insert_first_element: Option<(&K::Buf, &V::Buf)>,
        buf: &mut Vec<u8>,
    ) {
        let replace_key = right_sibling.read_key_buf(0);
        let replace_value = right_sibling.read_value_buf(0);

        if let Some((k, v)) = right_insert_first_element {
            right_sibling.write_key_buf(0, k);
            right_sibling.write_value_buf(0, v);

            parent.write_key_buf(parent_idx, k);
        } else {
            right_sibling.remove_key_buf(0, right_sibling_len, buf);
            right_sibling.remove_value_buf(0, right_sibling_len, buf);

            parent.write_key_buf(parent_idx, &right_sibling.read_key_buf(0));
        };

        self.push_key_buf(&replace_key, self_len);
        self.push_value_buf(&replace_value, self_len);
    }

    #[allow(clippy::explicit_counter_loop)]
    pub fn split_max_len(
        &mut self,
        right_biased: bool,
        buf: &mut Vec<u8>,
        certified: bool,
    ) -> Result<Self, OutOfMemory> {
        let mut right = Self::create(certified)?;

        let min_idx = if right_biased { MIN_LEN_AFTER_SPLIT } else { B };

        self.read_many_keys_to_buf(min_idx, CAPACITY - min_idx, buf);
        right.write_many_keys_from_buf(0, buf);

        self.read_many_values_to_buf(min_idx, CAPACITY - min_idx, buf);
        right.write_many_values_from_buf(0, buf);

        let self_next = self.read_next_ptr_buf();
        let mut buf = <u64 as AsFixedSizeBytes>::Buf::new(<u64 as AsFixedSizeBytes>::SIZE);

        right.ptr.as_fixed_size_bytes(buf._deref_mut());
        self.write_next_ptr_buf(&buf);

        self.ptr.as_fixed_size_bytes(buf._deref_mut());
        right.write_prev_ptr_buf(&buf);
        right.write_next_ptr_buf(&self_next);

        Ok(right)
    }

    pub fn merge_min_len(&mut self, right: Self, buf: &mut Vec<u8>) {
        right.read_many_keys_to_buf(0, MIN_LEN_AFTER_SPLIT, buf);
        self.write_many_keys_from_buf(MIN_LEN_AFTER_SPLIT, buf);

        right.read_many_values_to_buf(0, MIN_LEN_AFTER_SPLIT, buf);
        self.write_many_values_from_buf(MIN_LEN_AFTER_SPLIT, buf);

        let right_next_buf = right.read_next_ptr_buf();
        self.write_next_ptr_buf(&right_next_buf);

        if right_next_buf != [0u8; u64::SIZE] {
            let right_next_ptr = u64::from_fixed_size_bytes(&right_next_buf);
            let mut right_next = unsafe { Self::from_ptr(right_next_ptr) };

            right_next.write_prev_ptr_buf(&self.ptr.as_new_fixed_size_bytes());
        }

        right.destroy();
    }

    #[inline]
    pub fn remove_and_disown_by_idx(&mut self, idx: usize, len: usize, buf: &mut Vec<u8>) -> V {
        self.read_and_disown_key(idx);
        let v = self.read_and_disown_value(idx);

        self.remove_key_buf(idx, len, buf);
        self.remove_value_buf(idx, len, buf);

        v
    }

    #[inline]
    fn push_key_buf(&mut self, key: &K::Buf, len: usize) {
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

    fn remove_key_buf(&mut self, idx: usize, len: usize, buf: &mut Vec<u8>) {
        if idx == len - 1 {
            return;
        }

        self.read_many_keys_to_buf(idx + 1, len - idx - 1, buf);
        self.write_many_keys_from_buf(idx, buf);
    }

    #[inline]
    fn push_value_buf(&mut self, value: &V::Buf, len: usize) {
        self.write_value_buf(len, value);
    }

    pub fn insert_value_buf(&mut self, idx: usize, value: &V::Buf, len: usize, buf: &mut Vec<u8>) {
        if idx == len {
            self.push_value_buf(value, len);
            return;
        }

        self.read_many_values_to_buf(idx, len - idx, buf);
        self.write_many_values_from_buf(idx + 1, buf);

        self.write_value_buf(idx, value);
    }

    fn remove_value_buf(&mut self, idx: usize, len: usize, buf: &mut Vec<u8>) {
        if idx == len - 1 {
            return;
        }

        self.read_many_values_to_buf(idx + 1, len - idx - 1, buf);
        self.write_many_values_from_buf(idx, buf);
    }

    #[inline]
    pub fn get_key<'a>(&self, idx: usize) -> SRef<'a, K> {
        unsafe { SRef::new(self.get_key_ptr(idx)) }
    }

    #[inline]
    pub fn write_and_own_key(&mut self, idx: usize, mut key: K) {
        unsafe { crate::mem::write_fixed(self.get_key_ptr(idx), &mut key) };
    }

    #[inline]
    pub fn read_and_disown_key(&mut self, idx: usize) -> K {
        unsafe { crate::mem::read_fixed_for_move(self.get_key_ptr(idx)) }
    }

    #[inline]
    pub fn get_value<'a>(&self, idx: usize) -> SRef<'a, V> {
        unsafe { SRef::new(self.get_value_ptr(idx)) }
    }

    #[inline]
    pub fn get_value_mut<'a>(&mut self, idx: usize) -> SRefMut<'a, V> {
        unsafe { SRefMut::new(self.get_value_ptr(idx)) }
    }

    #[inline]
    pub fn write_and_own_value(&mut self, idx: usize, mut value: V) {
        unsafe { crate::mem::write_fixed(self.get_value_ptr(idx), &mut value) };
    }

    #[inline]
    pub fn read_and_disown_value(&mut self, idx: usize) -> V {
        unsafe { crate::mem::read_fixed_for_move(self.get_value_ptr(idx)) }
    }

    #[inline]
    pub fn write_key_buf(&mut self, idx: usize, key: &K::Buf) {
        unsafe { crate::mem::write_bytes(self.get_key_ptr(idx), key._deref()) };
    }

    #[inline]
    fn write_many_keys_from_buf(&self, from_idx: usize, buf: &Vec<u8>) {
        unsafe { crate::mem::write_bytes(self.get_key_ptr(from_idx), buf) };
    }

    #[inline]
    fn get_key_ptr(&self, idx: usize) -> u64 {
        SSlice::_offset(self.ptr, KEYS_OFFSET + (idx * K::SIZE) as u64)
    }

    #[inline]
    pub fn read_key_buf(&self, idx: usize) -> K::Buf {
        let mut buf = K::Buf::new(K::SIZE);

        unsafe { crate::mem::read_bytes(self.get_key_ptr(idx), buf._deref_mut()) };

        buf
    }

    pub fn read_key_as_reference(&self, idx: usize) -> K {
        let k_buf = self.read_key_buf(idx);
        let mut k = K::from_fixed_size_bytes(k_buf._deref());

        unsafe {
            k.stable_drop_flag_off();
        }

        k
    }

    #[inline]
    fn read_many_keys_to_buf(&self, from_idx: usize, len: usize, buf: &mut Vec<u8>) {
        buf.resize(len * K::SIZE, 0);

        unsafe { crate::mem::read_bytes(self.get_key_ptr(from_idx), buf) };
    }

    #[inline]
    pub fn write_value_buf(&mut self, idx: usize, value: &V::Buf) {
        unsafe { crate::mem::write_bytes(self.get_value_ptr(idx), value._deref()) };
    }

    #[inline]
    fn write_many_values_from_buf(&self, from_idx: usize, buf: &Vec<u8>) {
        unsafe { crate::mem::write_bytes(self.get_value_ptr(from_idx), buf) };
    }

    #[inline]
    fn get_value_ptr(&self, idx: usize) -> u64 {
        SSlice::_offset(self.ptr, values_offset::<K>() + (idx * V::SIZE) as u64)
    }

    #[inline]
    pub fn read_value_buf(&self, idx: usize) -> V::Buf {
        let mut b = V::Buf::new(V::SIZE);
        unsafe { crate::mem::read_bytes(self.get_value_ptr(idx), b._deref_mut()) };

        b
    }

    pub fn read_value_as_reference(&self, idx: usize) -> V {
        let v_buf = self.read_value_buf(idx);
        let mut v = V::from_fixed_size_bytes(v_buf._deref());

        unsafe {
            v.stable_drop_flag_off();
        }

        v
    }

    #[inline]
    fn read_many_values_to_buf(&self, from_idx: usize, len: usize, buf: &mut Vec<u8>) {
        buf.resize(len * V::SIZE, 0);

        unsafe { crate::mem::read_bytes(self.get_value_ptr(from_idx), buf) };
    }

    #[inline]
    pub fn write_prev_ptr_buf(&mut self, prev: &StablePtrBuf) {
        let ptr = SSlice::_offset(self.ptr, PREV_OFFSET);

        unsafe { crate::mem::write_bytes(ptr, prev) };
    }

    #[inline]
    pub fn read_prev_ptr_buf(&self) -> StablePtrBuf {
        let ptr = SSlice::_offset(self.ptr, PREV_OFFSET);
        let mut b = stable_ptr_buf();

        unsafe { crate::mem::read_bytes(ptr, &mut b) };

        b
    }

    #[inline]
    pub fn write_next_ptr_buf(&mut self, next: &StablePtrBuf) {
        let ptr = SSlice::_offset(self.ptr, NEXT_OFFSET);

        unsafe { crate::mem::write_bytes(ptr, next) };
    }

    #[inline]
    pub fn read_next_ptr_buf(&self) -> StablePtrBuf {
        let ptr = SSlice::_offset(self.ptr, NEXT_OFFSET);
        let mut b = stable_ptr_buf();

        unsafe { crate::mem::read_bytes(ptr, &mut b) };

        b
    }

    #[inline]
    pub fn write_root_hash(&mut self, root_hash: &Hash, certified: bool) {
        debug_assert!(certified);

        let ptr = SSlice::_offset(self.ptr, root_hash_offset::<K, V>());
        unsafe { crate::mem::write_bytes(ptr, root_hash) };
    }

    #[inline]
    pub fn read_root_hash(&self, certified: bool) -> Hash {
        debug_assert!(certified);

        let ptr = SSlice::_offset(self.ptr, root_hash_offset::<K, V>());
        let mut buf = EMPTY_HASH;

        unsafe { crate::mem::read_bytes(ptr, &mut buf) };

        buf
    }

    #[inline]
    pub fn write_len(&mut self, mut len: usize) {
        let ptr = SSlice::_offset(self.ptr, LEN_OFFSET);

        unsafe { crate::mem::write_fixed(ptr, &mut len) };
    }

    #[inline]
    pub fn read_len(&self) -> usize {
        let ptr = SSlice::_offset(self.ptr, LEN_OFFSET);

        unsafe { crate::mem::read_fixed_for_reference(ptr) }
    }

    #[inline]
    fn init_node_type(&mut self) {
        let ptr = SSlice::_offset(self.ptr, NODE_TYPE_OFFSET);

        unsafe { crate::mem::write_fixed(ptr, &mut NODE_TYPE_LEAF) };
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

impl<K: StableType + AsFixedSizeBytes + Ord + Debug, V: StableType + AsFixedSizeBytes + Debug>
    LeafBTreeNode<K, V>
{
    pub fn to_string(&self) -> String {
        let mut result = format!("LeafBTreeNode(&{}, {})[", self.as_ptr(), self.read_len());
        for i in 0..self.read_len() {
            result += &format!("({:?}, ", self.read_key_as_reference(i));
            result += &format!("{:?})", self.read_value_as_reference(i));

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
    use crate::encoding::AsFixedSizeBytes;
    use crate::{_debug_validate_allocator, get_allocated_size, stable, stable_memory_init};

    #[test]
    fn works_fine() {
        stable::clear();
        stable_memory_init();

        {
            let mut node = LeafBTreeNode::<u64, u64>::create(false).unwrap();
            let mut buf = Vec::default();

            for i in 0..CAPACITY {
                node.push_key_buf(&(i as u64).as_new_fixed_size_bytes(), i);
                node.push_value_buf(&(i as u64).as_new_fixed_size_bytes(), i);
            }

            for i in 0..CAPACITY {
                let k = node.read_key_buf(CAPACITY - i - 1);
                let v = node.read_value_buf(CAPACITY - i - 1);

                assert_eq!(k, ((CAPACITY - i - 1) as u64).as_new_fixed_size_bytes());
                assert_eq!(v, ((CAPACITY - i - 1) as u64).as_new_fixed_size_bytes());
            }

            for i in (0..CAPACITY).rev() {
                node.insert_key_buf(
                    0,
                    &(i as u64).as_new_fixed_size_bytes(),
                    CAPACITY - i - 1,
                    &mut buf,
                );
                node.insert_value_buf(
                    0,
                    &(i as u64).as_new_fixed_size_bytes(),
                    CAPACITY - i - 1,
                    &mut buf,
                );
            }

            node.write_len(CAPACITY);
            println!("{}", node.to_string());

            for i in 0..CAPACITY {
                let k = node.read_key_buf(i);
                let v = node.read_value_buf(i);

                node.remove_key_buf(i, CAPACITY, &mut buf);
                node.remove_value_buf(i, CAPACITY, &mut buf);

                assert_eq!(k, (i as u64).as_new_fixed_size_bytes());
                assert_eq!(v, (i as u64).as_new_fixed_size_bytes());

                node.insert_key_buf(i, &k, CAPACITY - 1, &mut buf);
                node.insert_value_buf(i, &v, CAPACITY - 1, &mut buf);
            }

            let right = node.split_max_len(true, &mut buf, false).unwrap();

            for i in 0..MIN_LEN_AFTER_SPLIT {
                let k = node.read_key_buf(i);
                let v = node.read_value_buf(i);

                assert_eq!(k, (i as u64).as_new_fixed_size_bytes());
                assert_eq!(v, (i as u64).as_new_fixed_size_bytes());
            }

            for i in 0..B {
                let k = right.read_key_buf(i);
                let v = right.read_value_buf(i);

                assert_eq!(
                    k,
                    ((i + MIN_LEN_AFTER_SPLIT) as u64).as_new_fixed_size_bytes()
                );
                assert_eq!(
                    v,
                    ((i + MIN_LEN_AFTER_SPLIT) as u64).as_new_fixed_size_bytes()
                );
            }

            node.merge_min_len(right, &mut buf);

            for i in 0..CAPACITY {
                let k = node.read_key_buf(i);
                let v = node.read_value_buf(i);

                assert_eq!(k, (i as u64).as_new_fixed_size_bytes());
                assert_eq!(v, (i as u64).as_new_fixed_size_bytes());
            }

            node.destroy();
        }

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
    }
}
