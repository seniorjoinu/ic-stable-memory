use crate::collections::btree_map::leaf_node::LeafBTreeNode;
use crate::collections::btree_map::{BTreeNode, IBTreeNode, SBTreeMap};
use crate::encoding::AsFixedSizeBytes;
use crate::primitive::s_ref::SRef;
use crate::primitive::StableType;

pub struct SBTreeMapIter<'a, K, V> {
    root: &'a Option<BTreeNode<K, V>>,
    node: Option<LeafBTreeNode<K, V>>,
    node_idx: usize,
    node_len: usize,
}

impl<'a, K: StableType + AsFixedSizeBytes + Ord, V: StableType + AsFixedSizeBytes>
    SBTreeMapIter<'a, K, V>
{
    #[inline]
    pub(crate) fn new(map: &'a SBTreeMap<K, V>) -> Self {
        Self {
            root: &map.root,
            node: None,
            node_idx: 0,
            node_len: 0,
        }
    }
}

impl<'a, K: StableType + AsFixedSizeBytes + Ord, V: StableType + AsFixedSizeBytes> Iterator
    for SBTreeMapIter<'a, K, V>
{
    type Item = (SRef<'a, K>, SRef<'a, V>);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(node) = &self.node {
            if self.node_idx == self.node_len {
                let ptr = u64::from_fixed_size_bytes(&node.read_next_ptr_buf());

                if ptr == 0 {
                    return None;
                }

                let new_node = unsafe { LeafBTreeNode::<K, V>::from_ptr(ptr) };
                let len = new_node.read_len();

                self.node = Some(new_node);
                self.node_idx = 0;
                self.node_len = len;
            }

            let res = (&self.node)
                .as_ref()
                .map(|it| (it.get_key(self.node_idx), it.get_value(self.node_idx)));

            self.node_idx += 1;

            res
        } else {
            let mut node = unsafe { self.root.as_ref()?.copy() };
            let leaf = loop {
                match node {
                    BTreeNode::Internal(i) => {
                        let child_ptr = u64::from_fixed_size_bytes(&i.read_child_ptr_buf(0));
                        node = BTreeNode::<K, V>::from_ptr(child_ptr);
                    }
                    BTreeNode::Leaf(l) => {
                        break l;
                    }
                }
            };

            self.node_len = leaf.read_len();

            if self.node_len == 0 {
                return None;
            }

            self.node_idx = 0;
            self.node = Some(leaf);

            self.next()
        }
    }
}

impl<'a, K: StableType + AsFixedSizeBytes + Ord, V: StableType + AsFixedSizeBytes>
    DoubleEndedIterator for SBTreeMapIter<'a, K, V>
{
    fn next_back(&mut self) -> Option<Self::Item> {
        if let Some(node) = &self.node {
            if self.node_idx == 0 {
                return None;
            }

            self.node_idx -= 1;

            let k = node.get_key(self.node_idx);
            let v = node.get_value(self.node_idx);

            if self.node_idx == 0 {
                let ptr = u64::from_fixed_size_bytes(&node.read_prev_ptr_buf());

                if ptr != 0 {
                    let new_node = unsafe { LeafBTreeNode::<K, V>::from_ptr(ptr) };
                    let len = new_node.read_len();

                    self.node = Some(new_node);
                    self.node_idx = len;
                    self.node_len = len;
                }
            }

            Some((k, v))
        } else {
            let mut node = unsafe { self.root.as_ref()?.copy() };
            let leaf = loop {
                match node {
                    BTreeNode::Internal(i) => {
                        let len = i.read_len();
                        let child_ptr = u64::from_fixed_size_bytes(&i.read_child_ptr_buf(len));
                        node = BTreeNode::<K, V>::from_ptr(child_ptr);
                    }
                    BTreeNode::Leaf(l) => {
                        break l;
                    }
                }
            };

            self.node_len = leaf.read_len();

            if self.node_len == 0 {
                return None;
            }

            self.node_idx = self.node_len;
            self.node = Some(leaf);

            self.next_back()
        }
    }
}
