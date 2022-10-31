use crate::utils::phantom_data::SPhantomData;
use crate::{allocate, SSlice};
use copy_as_bytes::traits::{AsBytes, SuperSized};
use std::cmp::Ordering;
use std::fmt::{Debug, Formatter};

const CAPACITY: usize = 11;
const MID_ELEM_IDX: usize = CAPACITY / 2;

// DEFAULTS ARE
//
// parent, left, right: u64 = 0
// len: usize = 0
// is_black, is_left_child: bool = false
//
// keys: [K; CAPACITY] = [uninit; CAPACITY]
// values: [V; CAPACITY] = [uninit; CAPACITY]

const PARENT_OFFSET: usize = 0;
const LEFT_OFFSET: usize = PARENT_OFFSET + u64::SIZE;
const RIGHT_OFFSET: usize = LEFT_OFFSET + u64::SIZE;
const LEN_OFFSET: usize = RIGHT_OFFSET + u64::SIZE;
const IS_BLACK_OFFSET: usize = LEN_OFFSET + usize::SIZE;
const IS_LEFT_CHILD_OFFSET: usize = IS_BLACK_OFFSET + bool::SIZE;
const KEYS_OFFSET: usize = IS_LEFT_CHILD_OFFSET + bool::SIZE;

#[inline]
pub(crate) const fn VALUES_OFFSET<K: SuperSized>() -> usize {
    KEYS_OFFSET + CAPACITY * K::SIZE
}

#[inline]
pub(crate) const fn node_meta_size() -> usize {
    u64::SIZE * 3 + usize::SIZE + bool::SIZE * 2
}

pub(crate) struct NRBTreeNode<K, V> {
    ptr: u64,
    _marker_k: SPhantomData<K>,
    _marker_v: SPhantomData<V>,
}

impl<K: AsBytes, V: AsBytes> NRBTreeNode<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    pub fn new() -> Self {
        let slice = allocate(node_meta_size() + K::SIZE * CAPACITY + V::SIZE * CAPACITY);
        let buf = [0u8; node_meta_size()];

        slice.write_bytes(0, &buf);

        Self {
            ptr: slice.get_ptr(),
            _marker_k: SPhantomData::new(),
            _marker_v: SPhantomData::new(),
        }
    }

    pub fn as_ptr(&self) -> u64 {
        self.ptr
    }

    pub unsafe fn from_ptr(ptr: u64) -> Self {
        Self {
            ptr,
            _marker_k: SPhantomData::new(),
            _marker_v: SPhantomData::new(),
        }
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
    pub fn set_left(&mut self, it: u64) {
        SSlice::_as_bytes_write(self.ptr, LEFT_OFFSET, it)
    }

    #[inline]
    pub fn get_left(&self) -> u64 {
        SSlice::_as_bytes_read(self.ptr, LEFT_OFFSET)
    }

    #[inline]
    pub fn set_right(&mut self, it: u64) {
        SSlice::_as_bytes_write(self.ptr, RIGHT_OFFSET, it)
    }

    #[inline]
    pub fn get_right(&self) -> u64 {
        SSlice::_as_bytes_read(self.ptr, RIGHT_OFFSET)
    }

    #[inline]
    fn set_len(&mut self, it: usize) {
        SSlice::_as_bytes_write(self.ptr, LEN_OFFSET, it);
    }

    #[inline]
    fn len(&self) -> usize {
        SSlice::_as_bytes_read(self.ptr, LEN_OFFSET)
    }

    #[inline]
    pub fn is_black(&self) -> bool {
        SSlice::_as_bytes_read(self.ptr, IS_BLACK_OFFSET)
    }

    #[inline]
    pub fn set_is_black(&mut self, it: bool) {
        SSlice::_as_bytes_write(self.ptr, IS_BLACK_OFFSET, it);
    }

    #[inline]
    pub fn is_left_child(&self) -> bool {
        SSlice::_as_bytes_read(self.ptr, IS_LEFT_CHILD_OFFSET)
    }

    #[inline]
    pub fn set_is_left_child(&mut self, it: bool) {
        SSlice::_as_bytes_write(self.ptr, IS_LEFT_CHILD_OFFSET, it)
    }

    #[inline]
    fn set_key(&mut self, idx: usize, k: K) {
        SSlice::_as_bytes_write(self.ptr, KEYS_OFFSET + idx * K::SIZE, k);
    }

    #[inline]
    fn get_key(&self, idx: usize) -> K {
        SSlice::_as_bytes_read(self.ptr, KEYS_OFFSET + idx * K::SIZE)
    }

    #[inline]
    fn set_value(&mut self, idx: usize, v: V) {
        SSlice::_as_bytes_write(self.ptr, VALUES_OFFSET::<K>() + idx * V::SIZE, v);
    }

    #[inline]
    fn get_value(&self, idx: usize) -> V {
        SSlice::_as_bytes_read(self.ptr, VALUES_OFFSET::<K>() + idx * V::SIZE)
    }

    #[inline]
    fn keys_shr(&mut self, idx1: usize, idx2: usize) {
        let mut buf = vec![0u8; (idx2 - idx1 + 1) * K::SIZE];

        SSlice::_read_bytes(self.ptr, KEYS_OFFSET + idx1 * K::SIZE, &mut buf);
        SSlice::_write_bytes(self.ptr, KEYS_OFFSET + (idx1 + 1) * K::SIZE, &buf);
    }

    #[inline]
    fn values_shr(&mut self, idx1: usize, idx2: usize) {
        let mut buf = vec![0u8; (idx2 - idx1 + 1) * V::SIZE];

        SSlice::_read_bytes(self.ptr, VALUES_OFFSET::<K>() + idx1 * V::SIZE, &mut buf);
        SSlice::_write_bytes(self.ptr, VALUES_OFFSET::<K>() + (idx1 + 1) * V::SIZE, &buf);
    }

    #[inline]
    fn keys_shl(&mut self, idx1: usize, idx2: usize) {
        let mut buf = vec![0u8; (idx2 - idx1 + 1) * K::SIZE];

        SSlice::_read_bytes(self.ptr, KEYS_OFFSET + idx1 * K::SIZE, &mut buf);
        SSlice::_write_bytes(self.ptr, KEYS_OFFSET + (idx1 - 1) * K::SIZE, &buf);
    }

    #[inline]
    fn values_shl(&mut self, idx1: usize, idx2: usize) {
        let mut buf = vec![0u8; (idx2 - idx1 + 1) * V::SIZE];

        SSlice::_read_bytes(self.ptr, VALUES_OFFSET::<K>() + idx1 * V::SIZE, &mut buf);
        SSlice::_write_bytes(self.ptr, VALUES_OFFSET::<K>() + (idx1 - 1) * V::SIZE, &buf);
    }
}

impl<K: AsBytes + Ord, V: AsBytes> NRBTreeNode<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    pub fn insert(&mut self, k: K, v: V) -> Result<Option<V>, (K, V, bool)> {
        let len = self.len();

        match self.find_idx(&k, len) {
            Ok(idx) => {
                let old_v = self.get_value(idx);
                self.set_value(idx, v);

                Ok(Some(old_v))
            }
            Err(mut idx) => {
                if len < CAPACITY {
                    if idx < len {
                        self.keys_shr(idx, len - 1);
                        self.values_shr(idx, len - 1);
                    }

                    self.set_key(idx, k);
                    self.set_value(idx, v);
                    self.set_len(len + 1);

                    return Ok(None);
                }

                if idx == 0 {
                    return Err((k, v, true));
                }

                if idx == len {
                    return Err((k, v, false));
                }

                if idx < MID_ELEM_IDX {
                    let old_k = self.get_key(0);
                    let old_v = self.get_value(0);

                    if idx == 1 {
                        self.set_key(0, k);
                        self.set_value(0, v);

                        return Err((old_k, old_v, true));
                    }

                    idx -= 1;

                    self.keys_shl(1, idx);
                    self.values_shl(1, idx);

                    self.set_key(idx, k);
                    self.set_value(idx, v);

                    return Err((old_k, old_v, true));
                }

                let old_k = self.get_key(len - 1);
                let old_v = self.get_value(len - 1);

                if idx == len - 1 {
                    self.set_key(idx, k);
                    self.set_value(idx, v);

                    return Err((old_k, old_v, false));
                }

                self.keys_shr(idx, len - 2);
                self.values_shr(idx, len - 2);

                self.set_key(idx, k);
                self.set_value(idx, v);

                Err((old_k, old_v, false))
            }
        }
    }

    pub fn remove(&mut self, k: &K) -> Result<Option<V>, bool> {
        let len = self.len();

        match self.find_idx(k, len) {
            Ok(idx) => {
                let new_len = len - 1;

                let v = self.get_value(idx);

                if idx < new_len {
                    self.keys_shl(idx + 1, new_len);
                    self.values_shl(idx + 1, new_len);
                }

                self.set_len(new_len);

                Ok(Some(v))
            }
            Err(idx) => {
                if idx == 0 {
                    return Err(true);
                }

                if idx == len {
                    return Err(false);
                }

                Ok(None)
            }
        }
    }

    pub fn contains_key(&self, k: &K) -> Result<bool, bool> {
        let len = self.len();

        match self.find_idx(k, len) {
            Ok(idx) => Ok(true),
            Err(idx) => {
                if idx == 0 {
                    return Err(true);
                }

                if idx == len {
                    return Err(false);
                }

                Ok(false)
            }
        }
    }

    pub fn get(&self, k: &K) -> Result<Option<V>, bool> {
        let len = self.len();

        match self.find_idx(k, len) {
            Ok(idx) => Ok(Some(self.get_value(idx))),
            Err(idx) => {
                if idx == 0 {
                    return Err(true);
                }

                if idx == len {
                    return Err(false);
                }

                Ok(None)
            }
        }
    }

    fn find_idx(&self, k: &K, len: usize) -> Result<usize, usize> {
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

impl<K: AsBytes, V: AsBytes> Default for NRBTreeNode<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K: AsBytes + Debug, V: AsBytes + Debug> Debug for NRBTreeNode<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("NRBTreeNode[")?;

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

#[cfg(test)]
mod tests {
    use crate::collections::nrb_tree_map::node::{NRBTreeNode, CAPACITY};
    use crate::{init_allocator, stable};

    #[test]
    fn basic_flow_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut node = NRBTreeNode::<u64, u64>::new();

        assert_eq!(node.get_left(), 0);
        assert_eq!(node.get_right(), 0);
        assert_eq!(node.get_parent(), 0);

        node.set_left(1000);
        node.set_right(2000);
        node.set_parent(3000);

        assert_eq!(node.get_left(), 1000);
        assert_eq!(node.get_right(), 2000);
        assert_eq!(node.get_parent(), 3000);

        for i in 10..(10 + CAPACITY) as u64 {
            assert!(node.insert(i * 2, i * 2).unwrap().is_none());
        }

        for i in 100..105 {
            let (k, v, to_left) = node.insert(i, i).unwrap_err();
            assert_eq!(k, i);
            assert_eq!(v, i);
            assert!(!to_left);
        }

        for i in 0..5 {
            let (k, v, to_left) = node.insert(i, i).unwrap_err();
            assert_eq!(k, i);
            assert_eq!(v, i);
            assert!(to_left);
        }

        println!("{:?}", node);

        for i in 10..(10 + CAPACITY) as u64 {
            let (k, v, to_left) = node.insert(i * 2 + 1, i * 2 + 1).unwrap_err();

            assert_eq!(node.len(), CAPACITY);

            println!("{}", i * 2 + 1);
            println!("{:?}", node);
        }

        for i in 0..5 {
            let to_left = node.remove(&i).unwrap_err();
            assert!(to_left)
        }

        for i in 100..105 {
            let to_left = node.remove(&i).unwrap_err();
            assert!(!to_left);
        }

        for i in 24..30 {
            assert_eq!(node.remove(&i).unwrap().unwrap(), i);
        }

        assert_eq!(node.remove(&32).unwrap().unwrap(), 32);
        assert!(node.remove(&32).unwrap().is_none());
        assert_eq!(node.remove(&34).unwrap().unwrap(), 34);

        assert_eq!(node.get_left(), 1000);
        assert_eq!(node.get_right(), 2000);
        assert_eq!(node.get_parent(), 3000);
    }
}
