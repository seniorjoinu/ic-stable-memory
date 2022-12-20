use crate::collections::b_plus_tree_map::leaf_node::LeafBTreeNode;
use crate::collections::b_plus_tree_map::{BTreeNode, SBTreeMap};
use crate::primitive::StableAllocated;
use crate::utils::encoding::AsFixedSizeBytes;

pub struct SBTreeMapIter<'a, K, V> {
    map: &'a SBTreeMap<K, V>,
    node: Option<LeafBTreeNode<K, V>>,
    idx: usize,
    len: usize,
}

impl<'a, K, V> SBTreeMapIter<'a, K, V> {
    #[inline]
    pub fn new(map: &'a SBTreeMap<K, V>) -> Self {
        Self {
            map,
            node: None,
            idx: 0,
            len: 0,
        }
    }
}
impl<'a, K: StableAllocated + Ord + Eq, V: StableAllocated> ExactSizeIterator
    for SBTreeMapIter<'a, K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    fn len(&self) -> usize {
        self.map.len as usize
    }
}

impl<'a, K: StableAllocated + Ord + Eq, V: StableAllocated> DoubleEndedIterator
    for SBTreeMapIter<'a, K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        if let Some(node) = &self.node {
            let k = K::from_fixed_size_bytes(&node.read_key(self.idx));
            let v = V::from_fixed_size_bytes(&node.read_value(self.idx));

            if self.idx == 0 {
                let prev_ptr = u64::from_fixed_size_bytes(&node.read_prev());

                if prev_ptr == 0 {
                    return None;
                }

                let prev = unsafe { LeafBTreeNode::<K, V>::from_ptr(prev_ptr) };
                let len = prev.read_len();

                self.idx = len - 1;
                self.len = len;
                self.node = Some(prev);
            } else {
                self.idx -= 1;
            }

            Some((k, v))
        } else {
            let mut node = unsafe { self.map.root.as_ref()?.copy() };
            let leaf = loop {
                match node {
                    BTreeNode::Internal(i) => {
                        let len = i.read_len();
                        let child_ptr = u64::from_fixed_size_bytes(&i.read_child_ptr(len));
                        node = BTreeNode::<K, V>::from_ptr(child_ptr);
                    }
                    BTreeNode::Leaf(l) => {
                        break l;
                    }
                }
            };

            let len = leaf.read_len();
            self.len = len;
            self.idx = len - 1;
            self.node = Some(leaf);

            self.next_back()
        }
    }
}

impl<'a, K: StableAllocated + Ord + Eq, V: StableAllocated> Iterator for SBTreeMapIter<'a, K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    type Item = (K, V);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(node) = &self.node {
            let k = K::from_fixed_size_bytes(&node.read_key(self.idx));
            let v = V::from_fixed_size_bytes(&node.read_value(self.idx));

            if self.idx == self.len - 1 {
                let next_ptr = u64::from_fixed_size_bytes(&node.read_next());

                if next_ptr == 0 {
                    return None;
                }

                let next = unsafe { LeafBTreeNode::<K, V>::from_ptr(next_ptr) };
                let len = next.read_len();

                self.idx = 0;
                self.len = len;
                self.node = Some(next);
            } else {
                self.idx += 1;
            }

            Some((k, v))
        } else {
            let mut node = unsafe { self.map.root.as_ref()?.copy() };
            let leaf = loop {
                match node {
                    BTreeNode::Internal(i) => {
                        let child_ptr = u64::from_fixed_size_bytes(&i.read_child_ptr(0));
                        node = BTreeNode::<K, V>::from_ptr(child_ptr);
                    }
                    BTreeNode::Leaf(l) => {
                        break l;
                    }
                }
            };

            self.len = leaf.read_len();
            self.idx = 0;
            self.node = Some(leaf);

            self.next()
        }
    }
}
