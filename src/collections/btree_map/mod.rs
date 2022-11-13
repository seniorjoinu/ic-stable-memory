use crate::collections::btree_map::iter::SBTreeMapIter;
use crate::collections::vec::SVec;
use crate::primitive::StableAllocated;
use crate::utils::phantom_data::SPhantomData;
use copy_as_bytes::traits::{AsBytes, SuperSized};
use speedy::{Context, LittleEndian, Readable, Reader, Writable, Writer};

pub mod iter;
pub mod map;
pub mod node;

const B: usize = 6;
const CAPACITY: usize = 2 * B - 1;
const MIN_LEN_AFTER_SPLIT: usize = B - 1;

pub struct SBTreeMap<K, V> {
    root: BTreeNode<K, V>,
    len: u64,
}

impl<K, V> SBTreeMap<K, V> {
    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl<K: Ord + StableAllocated, V: StableAllocated> SBTreeMap<K, V>
where
    [(); BTreeNode::<K, V>::SIZE]: Sized,
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
    [(); SVec::<BTreeNode<K, V>>::SIZE]: Sized,
    BTreeNode<K, V>: StableAllocated,
{
    pub fn new() -> Self {
        Self {
            root: BTreeNode::default(),
            len: 0,
        }
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        if self.root.keys.len() == CAPACITY {
            let mut temp = BTreeNode::new(false, false);

            self.root.is_root = false;
            temp.is_root = true;
            let old_root = std::mem::replace(&mut self.root, temp);

            self.root.children.insert(0, old_root);

            Self::split_child(&mut self.root, 0);
        }

        let res = Self::insert_non_full(&mut self.root, key, value);

        if res.is_none() {
            self.len += 1;
        }

        res
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        let res = Self::_delete(&mut self.root, key)?;
        self.len -= 1;

        Some(res)
    }

    unsafe fn _drop(mut node: BTreeNode<K, V>) {
        for i in 0..node.children.len() {
            Self::_drop(node.children.remove(0));
        }

        node.stable_drop();
    }

    pub fn get_copy(&self, key: &K) -> Option<V> {
        Self::_get(&self.root, key)
    }

    fn _get(node: &BTreeNode<K, V>, key: &K) -> Option<V> {
        match node.keys.binary_search_by(|k| k.cmp(key)) {
            Ok(idx) => node.values.get_copy(idx),
            Err(idx) => {
                let child = node.children.get_copy(idx)?;
                Self::_get(&child, key)
            }
        }
    }

    pub fn contains_key(&self, key: &K) -> bool {
        Self::_contains_key(&self.root, key)
    }

    fn _contains_key(node: &BTreeNode<K, V>, key: &K) -> bool {
        match node.keys.binary_search_by(|k| k.cmp(key)) {
            Ok(_) => true,
            Err(idx) => {
                if let Some(child) = node.children.get_copy(idx) {
                    Self::_contains_key(&child, key)
                } else {
                    false
                }
            }
        }
    }

    pub fn iter(&self) -> SBTreeMapIter<K, V> {
        SBTreeMapIter::new(self)
    }

    fn insert_non_full(node: &mut BTreeNode<K, V>, mut key: K, mut value: V) -> Option<V> {
        match node.keys.binary_search_by(|k| k.cmp(&key)) {
            Ok(idx) => {
                value.move_to_stable();

                let mut prev_value = node.values.replace(idx, value);

                prev_value.remove_from_stable();

                Some(prev_value)
            }
            Err(mut idx) => {
                if node.is_leaf {
                    key.move_to_stable();
                    value.move_to_stable();

                    node.keys.insert(idx, key);
                    node.values.insert(idx, value);

                    None
                } else {
                    if node.children.get_copy(idx).unwrap().keys.len() == CAPACITY {
                        Self::split_child(node, idx);

                        if key.gt(&node.keys.get_copy(idx).unwrap()) {
                            idx += 1;
                        }
                    }

                    let mut child = node.children.get_copy(idx).unwrap();
                    let result = Self::insert_non_full(&mut child, key, value);

                    node.children.replace(idx, child);

                    result
                }
            }
        }
    }

    fn split_child(node: &mut BTreeNode<K, V>, idx: usize) {
        let mut child = node.children.get_copy(idx).unwrap();
        let mut new_child = BTreeNode::<K, V>::new(child.is_leaf, false);

        for _ in 0..MIN_LEN_AFTER_SPLIT {
            new_child.keys.push(child.keys.remove(B));
            new_child.values.push(child.values.remove(B));
        }
        node.keys
            .insert(idx, child.keys.remove(MIN_LEN_AFTER_SPLIT));
        node.values
            .insert(idx, child.values.remove(MIN_LEN_AFTER_SPLIT));

        if !child.is_leaf {
            for _ in 0..B {
                new_child.children.push(child.children.remove(B));
            }
        }

        node.children.replace(idx, child);
        node.children.insert(idx + 1, new_child);
    }

    fn _delete(node: &mut BTreeNode<K, V>, key: &K) -> Option<V> {
        match node.keys.binary_search_by(|k| k.cmp(key)) {
            Ok(idx) => {
                if node.is_leaf {
                    let mut k = node.keys.remove(idx);
                    let mut v = node.values.remove(idx);

                    k.remove_from_stable();
                    v.remove_from_stable();

                    Some(v)
                } else {
                    Self::delete_internal_node(node, key, idx)
                }
            }
            Err(idx) => {
                let mut merged = false;

                if node.is_leaf {
                    return None;
                }

                let mut child = node.children.get_copy(idx).unwrap();

                if child.keys.len() >= B {
                    let res = Self::_delete(&mut child, key);
                    node.children.replace(idx, child);

                    res
                } else {
                    if idx != 0 && idx + 1 < node.children.len() {
                        let left_child_sibling = node.children.get_copy(idx - 1).unwrap();
                        let right_child_sibling = node.children.get_copy(idx + 1).unwrap();

                        if left_child_sibling.keys.len() >= B {
                            Self::delete_sibling(node, idx, idx - 1);
                        } else if right_child_sibling.keys.len() >= B {
                            Self::delete_sibling(node, idx, idx + 1);
                        } else {
                            Self::delete_merge(node, idx, idx + 1);
                            merged = true;
                        }
                    } else if idx == 0 {
                        let right_child_sibling = node.children.get_copy(idx + 1).unwrap();

                        if right_child_sibling.keys.len() >= B {
                            Self::delete_sibling(node, idx, idx + 1);
                        } else {
                            Self::delete_merge(node, idx, idx + 1);
                            merged = true;
                        }
                    } else if idx + 1 == node.children.len() {
                        let left_child_sibling = node.children.get_copy(idx - 1).unwrap();

                        if left_child_sibling.keys.len() >= B {
                            Self::delete_sibling(node, idx, idx - 1);
                        } else {
                            Self::delete_merge(node, idx, idx - 1);
                            merged = true;
                        }
                    }

                    if merged {
                        return Self::_delete(node, key);
                    }

                    let mut child = node.children.get_copy(idx).unwrap();
                    let res = Self::_delete(&mut child, key);
                    node.children.replace(idx, child);

                    res
                }
            }
        }
    }

    fn delete_internal_node(node: &mut BTreeNode<K, V>, key: &K, idx: usize) -> Option<V> {
        let mut left_child = node.children.get_copy(idx).unwrap();
        if left_child.keys.len() >= B {
            let (k, v) = Self::delete_predecessor(&mut left_child);
            let mut k = node.keys.replace(idx, k);
            let mut v = node.values.replace(idx, v);

            k.remove_from_stable();
            v.remove_from_stable();

            node.children.replace(idx, left_child);

            return Some(v);
        }

        let mut right_child = node.children.get_copy(idx + 1).unwrap();
        if right_child.keys.len() >= B {
            let (k, v) = Self::delete_successor(&mut right_child);
            let mut k = node.keys.replace(idx, k);
            let mut v = node.values.replace(idx, v);

            k.remove_from_stable();
            v.remove_from_stable();

            node.children.replace(idx + 1, right_child);

            return Some(v);
        }

        Self::delete_merge(node, idx, idx + 1);
        Self::_delete(node, key)
    }

    fn delete_predecessor(child: &mut BTreeNode<K, V>) -> (K, V) {
        if child.is_leaf {
            let k = child.keys.pop().unwrap();
            let v = child.values.pop().unwrap();

            return (k, v);
        }

        let n = child.keys.len() - 1;
        let grand_child = child.children.get_copy(n).unwrap();

        if grand_child.keys.len() >= B {
            Self::delete_sibling(child, n + 1, n);
        } else {
            Self::delete_merge(child, n + 1, n);
        }

        let mut grand_child = child.children.get_copy(n).unwrap();
        let res = Self::delete_predecessor(&mut grand_child);

        child.children.replace(n, grand_child);

        res
    }

    fn delete_successor(child: &mut BTreeNode<K, V>) -> (K, V) {
        if child.is_leaf {
            let k = child.keys.remove(0);
            let v = child.values.remove(0);

            return (k, v);
        }

        let grand_child = child.children.get_copy(0).unwrap();

        if grand_child.keys.len() >= B {
            Self::delete_sibling(child, 0, 1);
        } else {
            Self::delete_merge(child, 0, 1);
        }

        let mut grand_child = child.children.get_copy(0).unwrap();
        let res = Self::delete_successor(&mut grand_child);

        child.children.replace(0, grand_child);

        res
    }

    fn delete_merge(node: &mut BTreeNode<K, V>, i: usize, j: usize) {
        let mut child = node.children.get_copy(i).unwrap();

        if j > i {
            let child_right_sibling = node.children.remove(j);
            child.keys.push(node.keys.remove(i));
            child.values.push(node.values.remove(i));

            child.keys.extend_from(child_right_sibling.keys);
            child.values.extend_from(child_right_sibling.values);
            child.children.extend_from(child_right_sibling.children);

            if node.is_root && node.keys.is_empty() {
                child.is_root = true;
                *node = child;
            } else {
                node.children.replace(i, child);
            }
        } else {
            let mut child_left_sibling = node.children.get_copy(j).unwrap();
            child_left_sibling.keys.push(node.keys.remove(j));
            child_left_sibling.values.push(node.values.remove(j));

            child_left_sibling.keys.extend_from(child.keys);
            child_left_sibling.values.extend_from(child.values);
            child_left_sibling.children.extend_from(child.children);

            node.children.remove(i);

            if node.is_root && node.keys.is_empty() {
                child_left_sibling.is_root = true;
                *node = child_left_sibling;
            } else {
                node.children.replace(j, child_left_sibling);
            }
        };
    }

    fn delete_sibling(node: &mut BTreeNode<K, V>, i: usize, j: usize) {
        let mut child = node.children.get_copy(i).unwrap();

        if j > i {
            let mut child_right_sibling = node.children.get_copy(j).unwrap();

            child.keys.push(node.keys.remove(i));
            child.values.push(node.values.remove(i));

            node.keys.insert(i, child_right_sibling.keys.remove(0));
            node.values.insert(i, child_right_sibling.values.remove(0));

            if !child_right_sibling.children.is_empty() {
                child.children.push(child_right_sibling.children.remove(0));
            }

            node.children.replace(j, child_right_sibling);
        } else {
            let mut child_left_sibling = node.children.get_copy(j).unwrap();

            child.keys.insert(0, node.keys.remove(i - 1));
            child.values.insert(0, node.values.remove(i - 1));

            node.keys
                .insert(i - 1, child_left_sibling.keys.pop().unwrap());
            node.values
                .insert(i - 1, child_left_sibling.values.pop().unwrap());

            if !child_left_sibling.children.is_empty() {
                child
                    .children
                    .insert(0, child_left_sibling.children.pop().unwrap())
            }

            node.children.replace(j, child_left_sibling);
        }

        node.children.replace(i, child);
    }
}

impl<K: StableAllocated + Ord, V: StableAllocated> Default for SBTreeMap<K, V>
where
    [(); BTreeNode::<K, V>::SIZE]: Sized, // ???? why only putting K is enough
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
    [(); SVec::<BTreeNode<K, V>>::SIZE]: Sized,
    BTreeNode<K, V>: StableAllocated,
{
    fn default() -> Self {
        SBTreeMap::<K, V>::new()
    }
}

impl<'a, K, V> Readable<'a, LittleEndian> for SBTreeMap<K, V> {
    fn read_from<R: Reader<'a, LittleEndian>>(
        reader: &mut R,
    ) -> Result<Self, <speedy::LittleEndian as Context>::Error> {
        let root = BTreeNode::<K, V>::read_from(reader)?;
        let len = reader.read_u64()?;

        Ok(Self { root, len })
    }
}

impl<K, V> Writable<LittleEndian> for SBTreeMap<K, V> {
    fn write_to<W: ?Sized + Writer<LittleEndian>>(
        &self,
        writer: &mut W,
    ) -> Result<(), <speedy::LittleEndian as Context>::Error> {
        self.root.write_to(writer)?;
        writer.write_u64(self.len)
    }
}

impl<K, V> SuperSized for SBTreeMap<K, V> {
    const SIZE: usize = BTreeNode::<K, V>::SIZE + u64::SIZE;
}

impl<K: StableAllocated, V: StableAllocated> AsBytes for SBTreeMap<K, V>
where
    [(); BTreeNode::<K, V>::SIZE]: Sized,
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
    [(); SVec::<BTreeNode<K, V>>::SIZE]: Sized,
    BTreeNode<K, V>: StableAllocated,
{
    fn from_bytes(arr: [u8; Self::SIZE]) -> Self {
        let mut root_buf = [0u8; BTreeNode::<K, V>::SIZE];
        let mut len_buf = [0u8; u64::SIZE];

        root_buf.copy_from_slice(&arr[..BTreeNode::<K, V>::SIZE]);
        len_buf
            .copy_from_slice(&arr[BTreeNode::<K, V>::SIZE..(BTreeNode::<K, V>::SIZE + u64::SIZE)]);

        Self {
            root: BTreeNode::<K, V>::from_bytes(root_buf),
            len: u64::from_bytes(len_buf),
        }
    }

    fn to_bytes(self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[..BTreeNode::<K, V>::SIZE].copy_from_slice(&self.root.to_bytes());
        buf[BTreeNode::<K, V>::SIZE..(BTreeNode::<K, V>::SIZE + u64::SIZE)]
            .copy_from_slice(&self.len.to_bytes());

        buf
    }
}

impl<K: StableAllocated + Ord, V: StableAllocated> StableAllocated for SBTreeMap<K, V>
where
    [(); BTreeNode::<K, V>::SIZE]: Sized,
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
    [(); SVec::<BTreeNode<K, V>>::SIZE]: Sized,
    BTreeNode<K, V>: StableAllocated,
{
    #[inline]
    fn move_to_stable(&mut self) {}
    #[inline]
    fn remove_from_stable(&mut self) {}

    unsafe fn stable_drop(mut self) {
        while let Some(child_node) = self.root.children.pop() {
            Self::_drop(child_node);
        }
    }
}

pub struct BTreeNode<K, V> {
    is_leaf: bool,
    is_root: bool,
    keys: SVec<K>,
    values: SVec<V>,
    children: SVec<Self>,
}

impl<K: StableAllocated, V: StableAllocated> Default for BTreeNode<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
    [(); SVec::<Self>::SIZE]: Sized,
    [(); Self::SIZE]: Sized,
{
    fn default() -> Self {
        Self::new(true, true)
    }
}

impl<K: StableAllocated, V: StableAllocated> BTreeNode<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
    [(); SVec::<Self>::SIZE]: Sized,
    [(); Self::SIZE]: Sized,
{
    pub fn new(is_leaf: bool, is_root: bool) -> Self {
        Self {
            is_root,
            is_leaf,
            keys: SVec::new_with_capacity(CAPACITY),
            values: SVec::new_with_capacity(CAPACITY),
            children: SVec::new_with_capacity(CAPACITY),
        }
    }
}

impl<K, V> BTreeNode<K, V> {
    unsafe fn unsafe_clone(&self) -> Self {
        Self {
            is_root: self.is_root,
            is_leaf: self.is_leaf,
            keys: SVec {
                ptr: self.keys.ptr,
                len: self.keys.len,
                cap: self.keys.cap,
                _marker_t: SPhantomData::default(),
            },
            values: SVec {
                ptr: self.values.ptr,
                len: self.values.len,
                cap: self.values.cap,
                _marker_t: SPhantomData::default(),
            },
            children: SVec {
                ptr: self.children.ptr,
                len: self.children.len,
                cap: self.children.cap,
                _marker_t: SPhantomData::default(),
            },
        }
    }
}

impl<K, V> SuperSized for BTreeNode<K, V> {
    const SIZE: usize =
        bool::SIZE + bool::SIZE + SVec::<K>::SIZE + SVec::<V>::SIZE + SVec::<Self>::SIZE;
}

impl<K: StableAllocated, V: StableAllocated> AsBytes for BTreeNode<K, V>
where
    [(); K::SIZE]: Sized,
    [(); SVec::<K>::SIZE]: Sized,
    [(); V::SIZE]: Sized,
    [(); SVec::<V>::SIZE]: Sized,
    [(); Self::SIZE]: Sized,
    [(); SVec::<Self>::SIZE]: Sized,
{
    fn to_bytes(self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        if self.is_root {
            buf[0] = 1
        };
        if self.is_leaf {
            buf[1] = 1
        };

        let (keys_buf, rest) = buf[2..].split_at_mut(SVec::<K>::SIZE);
        let (vals_buf, children_buf) = rest.split_at_mut(SVec::<V>::SIZE);

        keys_buf.copy_from_slice(&self.keys.to_bytes());
        vals_buf.copy_from_slice(&self.values.to_bytes());
        children_buf.copy_from_slice(&self.children.to_bytes());

        buf
    }

    fn from_bytes(arr: [u8; Self::SIZE]) -> Self {
        debug_assert!(arr[0] < 2 && arr[1] < 2);

        let is_root = arr[0] == 1;
        let is_leaf = arr[1] == 1;

        let (keys_buf, rest) = arr[2..].split_at(SVec::<K>::SIZE);
        let (vals_buf, children_buf) = rest.split_at(SVec::<V>::SIZE);

        let mut keys_arr = [0u8; SVec::<K>::SIZE];
        let mut vals_arr = [0u8; SVec::<V>::SIZE];
        let mut children_arr = [0u8; SVec::<Self>::SIZE];

        keys_arr.copy_from_slice(keys_buf);
        vals_arr.copy_from_slice(vals_buf);
        children_arr.copy_from_slice(children_buf);

        Self {
            is_root,
            is_leaf,
            keys: SVec::<K>::from_bytes(keys_arr),
            values: SVec::<V>::from_bytes(vals_arr),
            children: SVec::<Self>::from_bytes(children_arr),
        }
    }
}

impl<K: StableAllocated, V: StableAllocated> StableAllocated for BTreeNode<K, V>
where
    [(); K::SIZE]: Sized,
    [(); SVec::<K>::SIZE]: Sized,
    [(); V::SIZE]: Sized,
    [(); SVec::<V>::SIZE]: Sized,
    [(); Self::SIZE]: Sized,
    [(); SVec::<Self>::SIZE]: Sized,
{
    #[inline]
    fn move_to_stable(&mut self) {}

    #[inline]
    fn remove_from_stable(&mut self) {}

    #[inline]
    unsafe fn stable_drop(self) {
        self.keys.stable_drop();
        self.values.stable_drop();
        self.children.stable_drop();
    }
}

impl<'a, K, V> Readable<'a, LittleEndian> for BTreeNode<K, V> {
    fn read_from<R: Reader<'a, LittleEndian>>(
        reader: &mut R,
    ) -> Result<Self, <speedy::LittleEndian as Context>::Error> {
        let is_leaf_byte = reader.read_u8()?;
        let is_leaf = match is_leaf_byte {
            0 => false,
            1 => true,
            _ => unreachable!(),
        };
        let is_root_byte = reader.read_u8()?;
        let is_root = match is_root_byte {
            0 => false,
            1 => true,
            _ => unreachable!(),
        };

        let keys = SVec::read_from(reader)?;
        let values = SVec::read_from(reader)?;
        let children = SVec::read_from(reader)?;

        Ok(Self {
            is_leaf,
            is_root,
            keys,
            values,
            children,
        })
    }
}

impl<K, V> Writable<LittleEndian> for BTreeNode<K, V> {
    fn write_to<T: ?Sized + Writer<LittleEndian>>(
        &self,
        writer: &mut T,
    ) -> Result<(), <speedy::LittleEndian as Context>::Error> {
        let is_leaf_byte: u8 = u8::from(self.is_leaf);
        writer.write_u8(is_leaf_byte)?;

        let is_root_byte: u8 = u8::from(self.is_root);
        writer.write_u8(is_root_byte)?;

        self.keys.write_to(writer)?;
        self.values.write_to(writer)?;
        self.children.write_to(writer)
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::btree_map::SBTreeMap;
    use crate::primitive::StableAllocated;
    use crate::{init_allocator, stable};
    use copy_as_bytes::traits::AsBytes;
    use rand::seq::SliceRandom;
    use rand::thread_rng;
    use speedy::{Readable, Writable};

    #[test]
    fn random_works_as_expected() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let example = vec![
            (10, 1),
            (20, 2),
            (30, 3),
            (40, 4),
            (50, 5),
            (60, 6),
            (70, 7),
            (80, 8),
            (90, 9),
        ];

        let mut map = SBTreeMap::new();

        println!("INSERTION");

        assert!(map.insert(30, 3).is_none());
        assert!(map.insert(90, 9).is_none());
        assert!(map.insert(10, 1).is_none());
        assert!(map.insert(70, 7).is_none());
        assert!(map.insert(80, 8).is_none());
        assert!(map.insert(50, 5).is_none());
        assert!(map.insert(20, 2).is_none());
        assert!(map.insert(60, 6).is_none());
        assert!(map.insert(40, 4).is_none());

        assert_eq!(map.len(), 9);

        println!("DELETION");

        assert_eq!(map.remove(&30).unwrap(), 3);
        assert_eq!(map.remove(&70).unwrap(), 7);
        assert_eq!(map.remove(&50).unwrap(), 5);
        assert_eq!(map.remove(&40).unwrap(), 4);
        assert_eq!(map.remove(&60).unwrap(), 6);
        assert_eq!(map.remove(&20).unwrap(), 2);
        assert_eq!(map.remove(&80).unwrap(), 8);
        assert_eq!(map.remove(&10).unwrap(), 1);
        assert_eq!(map.remove(&90).unwrap(), 9);
        assert!(map.is_empty());

        unsafe { map.stable_drop() };
    }

    #[test]
    fn sequential_works_as_expected() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SBTreeMap::new();

        println!("INSERTION");

        for i in 0..10 {
            map.insert(i, 0);
        }

        println!("DELETION");

        for i in 0..10 {
            map.remove(&i).unwrap();
        }

        unsafe { map.stable_drop() };
    }

    #[test]
    fn basic_flow_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SBTreeMap::new();

        let prev = map.insert(1, 10);
        assert!(prev.is_none());

        let val = map.get_copy(&1).unwrap();
        assert_eq!(val, 10);
        assert!(map.contains_key(&1));

        assert!(map.insert(2, 20).is_none());
        map.insert(3, 30);
        map.insert(4, 40);
        map.insert(5, 50);

        let val = map.insert(3, 130).unwrap();
        assert_eq!(val, 30);

        assert!(!map.contains_key(&99));
        assert!(map.remove(&99).is_none());

        unsafe { map.stable_drop() };

        let _map = SBTreeMap::<u64, u64>::default();
    }

    #[test]
    fn deletion_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SBTreeMap::new();

        for i in 0..50 {
            map.insert(i + 10, i);
        }

        let val = map.insert(13, 130).unwrap();
        assert_eq!(val, 3);

        let val1 = map.get_copy(&13).unwrap();
        assert_eq!(val1, 130);

        assert!(!map.contains_key(&99));
        assert!(map.remove(&99).is_none());

        map.insert(13, 3);
        assert_eq!(map.remove(&16).unwrap(), 6);

        map.insert(16, 6);
        map.insert(9, 90);

        assert_eq!(map.remove(&16).unwrap(), 6);

        map.insert(16, 6);
        assert_eq!(map.remove(&9).unwrap(), 90);
        assert_eq!(map.remove(&53).unwrap(), 43);

        map.insert(60, 70);
        map.insert(61, 71);
        assert_eq!(map.remove(&58).unwrap(), 48);

        unsafe { map.stable_drop() };

        let mut map = SBTreeMap::new();

        for i in 0..50 {
            map.insert(i * 2, i);
        }

        map.insert(85, 1);
        assert_eq!(map.remove(&88).unwrap(), 44);

        unsafe { map.stable_drop() };

        let mut map = SBTreeMap::new();

        for i in 0..50 {
            map.insert(i * 2, i);
        }

        map.remove(&94);
        map.remove(&96);
        map.remove(&98);

        assert_eq!(map.remove(&88).unwrap(), 44);

        map.insert(81, 1);
        map.insert(83, 1);
        map.insert(94, 1);
        map.insert(85, 1);

        assert_eq!(map.remove(&86).unwrap(), 43);

        map.insert(71, 1);
        map.insert(73, 1);
        map.insert(75, 1);
        map.insert(77, 1);
        map.insert(79, 1);

        map.insert(47, 1);
        map.insert(49, 1);
        map.insert(51, 1);
        map.insert(53, 1);
        map.insert(55, 1);
        map.insert(57, 1);
        map.insert(59, 1);
        map.insert(61, 1);
        map.insert(63, 1);
        map.insert(65, 1);
        map.insert(67, 1);
        map.insert(69, 1);

        unsafe { map.stable_drop() };

        let mut map = SBTreeMap::new();

        for i in 150..300 {
            map.insert(i, i);
        }
        for i in 0..150 {
            map.insert(i, i);
        }

        assert_eq!(map.remove(&203).unwrap(), 203);
        assert_eq!(map.remove(&80).unwrap(), 80);

        unsafe { map.stable_drop() };
    }

    #[test]
    fn complex_deletes_work_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SBTreeMap::new();

        for i in 0..75 {
            map.insert(i, i);
        }

        for i in 0..75 {
            map.insert(150 - i, i);
        }

        for i in 0..150 {
            let j = if i % 2 == 0 { i } else { 150 - i };

            if j % 3 == 0 {
                map.remove(&j);
            }
        }

        unsafe { map.stable_drop() };

        let mut map = SBTreeMap::new();

        for i in 0..150 {
            map.insert(150 - i, i);
        }

        for i in 0..150 {
            map.remove(&(150 - i));
        }

        unsafe { map.stable_drop() };

        let mut map = SBTreeMap::new();

        for i in 0..5000 {
            map.insert(4999 - i, i);
        }

        let mut vec = (2000..3000).collect::<Vec<_>>();
        vec.shuffle(&mut thread_rng());

        for i in vec {
            map.remove(&i);
        }

        for i in 2000..3000 {
            map.insert(i, i);
        }

        let mut vec = (0..5000).collect::<Vec<_>>();
        vec.shuffle(&mut thread_rng());

        for i in vec {
            map.remove(&i);
        }

        unsafe { map.stable_drop() };
    }

    #[test]
    fn set_like_map_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SBTreeMap::<i32, ()>::new();
        map.insert(1, ());
        unsafe { map.stable_drop() };
    }

    #[test]
    fn iter_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut map = SBTreeMap::new();
        for i in 0..100 {
            map.insert(i, i);
        }

        let mut c = 0;
        for (idx, (k, v)) in map.iter().enumerate() {
            assert!(k == idx && v == idx);
            c += 1;
        }

        assert_eq!(c, 100);
    }

    #[test]
    fn serialization_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let map = SBTreeMap::<u32, u32>::new();
        let buf = map.write_to_vec().unwrap();
        let map1 = SBTreeMap::<u32, u32>::read_from_buffer_copying_data(&buf).unwrap();

        assert_eq!(map.len, map1.len);
        assert_eq!(map.root.is_root, map1.root.is_root);
        assert_eq!(map.root.is_leaf, map1.root.is_leaf);

        assert_eq!(map.root.keys.ptr, map1.root.keys.ptr);
        assert_eq!(map.root.keys.len, map1.root.keys.len);
        assert_eq!(map.root.keys.cap, map1.root.keys.cap);

        assert_eq!(map.root.values.ptr, map1.root.values.ptr);
        assert_eq!(map.root.values.len, map1.root.values.len);
        assert_eq!(map.root.values.cap, map1.root.values.cap);

        assert_eq!(map.root.children.ptr, map1.root.children.ptr);
        assert_eq!(map.root.children.len, map1.root.children.len);
        assert_eq!(map.root.children.cap, map1.root.children.cap);

        let len = map.len;
        let is_root = map.root.is_root;
        let is_leaf = map.root.is_leaf;

        let keys_ptr = map.root.keys.ptr;
        let keys_len = map.root.keys.len;
        let keys_cap = map.root.keys.cap;

        let values_ptr = map.root.values.ptr;
        let values_len = map.root.values.len;
        let values_cap = map.root.values.cap;

        let children_ptr = map.root.children.ptr;
        let children_len = map.root.children.len;
        let children_cap = map.root.children.cap;

        let buf = map.to_bytes();
        let map1 = SBTreeMap::<u32, u32>::from_bytes(buf);

        assert_eq!(len, map1.len);
        assert_eq!(is_root, map1.root.is_root);
        assert_eq!(is_leaf, map1.root.is_leaf);

        assert_eq!(keys_ptr, map1.root.keys.ptr);
        assert_eq!(keys_len, map1.root.keys.len);
        assert_eq!(keys_cap, map1.root.keys.cap);

        assert_eq!(values_ptr, map1.root.values.ptr);
        assert_eq!(values_len, map1.root.values.len);
        assert_eq!(values_cap, map1.root.values.cap);

        assert_eq!(children_ptr, map1.root.children.ptr);
        assert_eq!(children_len, map1.root.children.len);
        assert_eq!(children_cap, map1.root.children.cap);
    }
}