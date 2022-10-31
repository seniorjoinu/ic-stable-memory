use crate::allocate;
use crate::mem::s_slice::SSlice;
use crate::utils::phantom_data::SPhantomData;
use copy_as_bytes::traits::{AsBytes, SuperSized};
use std::cmp::Ordering;
use std::fmt::{Debug, Formatter};

const B: usize = 6;
const CAPACITY: usize = 2 * B - 1;
const MIN_LEN_AFTER_SPLIT: usize = B - 1;

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
    pub fn new() -> Self {
        let slice =
            allocate(node_meta_size() + (K::SIZE + V::SIZE + u64::SIZE) * CAPACITY + u64::SIZE);
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
    fn set_len(&mut self, it: usize) {
        SSlice::_as_bytes_write(self.ptr, LEN_OFFSET, it);
    }

    #[inline]
    fn len(&self) -> usize {
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
    fn set_child_ptr(&mut self, idx: usize, c: u64) {
        SSlice::_as_bytes_write(self.ptr, CHILDREN_OFFSET::<K, V>() + idx * u64::SIZE, c);
    }

    #[inline]
    fn get_child_ptr(&self, idx: usize) -> u64 {
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
{
    pub fn insert_key(&mut self, k: K, idx: usize, len: usize) {
        debug_assert!(len < CAPACITY && idx <= len);

        if idx != len {
            self.keys_shr(idx, len);
        }

        self.set_key(idx, k);
    }

    pub fn remove_key(&mut self, idx: usize, len: usize) -> K {
        debug_assert!(len < CAPACITY && idx < len);

        let k = self.get_key(idx);

        if idx != len {
            self.keys_shl(idx + 1, len);
        }

        k
    }

    pub fn insert_value(&mut self, v: V, idx: usize, len: usize) {
        debug_assert!(len < CAPACITY && idx <= len);

        if idx != len {
            self.values_shr(idx, len);
        }

        self.set_value(idx, v);
    }

    pub fn remove_value(&mut self, idx: usize, len: usize) -> V {
        debug_assert!(len < CAPACITY && idx < len);

        let v = self.get_value(idx);

        if idx != len {
            self.values_shl(idx + 1, len);
        }

        v
    }

    pub fn insert_child_ptr(&mut self, c: u64, idx: usize, len: usize) {
        debug_assert!(len < CAPACITY + 1 && idx <= len);

        if idx != len {
            self.children_shr(idx, len);
        }

        self.set_child_ptr(idx, c);
    }

    pub fn remove_child_ptr(&mut self, idx: usize, len: usize) -> u64 {
        debug_assert!(len < CAPACITY && idx < len);

        let c = self.get_child_ptr(idx);

        if idx != len {
            self.children_shl(idx + 1, len);
        }

        c
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

impl<K: AsBytes, V: AsBytes> Default for BTreeNode<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    fn default() -> Self {
        Self::new()
    }
}

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
