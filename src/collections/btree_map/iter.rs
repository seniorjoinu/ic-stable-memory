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
                let next_ptr = u64::from_fixed_size_bytes(&node.read_next_ptr_buf());

                if next_ptr == 0 {
                    return None;
                }

                let next = unsafe { LeafBTreeNode::<K, V>::from_ptr(next_ptr) };
                let len = next.read_len();

                self.node_idx = 0;
                self.node_len = len;
                self.node = Some(next);

                self.next()
            } else {
                let k = node.get_key(self.node_idx);
                let v = node.get_value(self.node_idx);

                self.node_idx += 1;

                Some((k, v))
            }
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
