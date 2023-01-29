use crate::collections::btree_map::leaf_node::LeafBTreeNode;
use crate::collections::btree_map::{BTreeNode, IBTreeNode};
use crate::primitive::s_ref::SRef;
use crate::primitive::StableAllocated;
use crate::utils::encoding::AsFixedSizeBytes;

pub struct SBTreeMapIter<'a, K, V> {
    root: &'a Option<BTreeNode<K, V>>,
    len: u64,
    node: Option<LeafBTreeNode<K, V>>,
    node_idx: usize,
    node_len: usize,
}

impl<'a, K, V> SBTreeMapIter<'a, K, V> {
    #[inline]
    pub(crate) fn new(root: &'a Option<BTreeNode<K, V>>, len: u64) -> Self {
        Self {
            root,
            len,
            node: None,
            node_idx: 0,
            node_len: 0,
        }
    }
}
impl<'a, K: StableAllocated + Ord, V: StableAllocated> ExactSizeIterator for SBTreeMapIter<'a, K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    fn len(&self) -> usize {
        self.len as usize
    }
}

impl<'a, K: StableAllocated + Ord, V: StableAllocated> DoubleEndedIterator
    for SBTreeMapIter<'a, K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        if let Some(node) = &self.node {
            let k = SRef::new(node.get_key_ptr(self.node_idx));
            let v = SRef::new(node.get_value_ptr(self.node_idx));

            if self.node_idx == 0 {
                let prev_ptr = u64::from_fixed_size_bytes(&node.read_prev());

                if prev_ptr == 0 {
                    return None;
                }

                let prev = unsafe { LeafBTreeNode::<K, V>::from_ptr(prev_ptr) };
                let len = prev.read_len();

                self.node_idx = len - 1;
                self.node_len = len;
                self.node = Some(prev);
            } else {
                self.node_idx -= 1;
            }

            Some((k, v))
        } else {
            let mut node = unsafe { self.root.as_ref()?.copy() };
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
            self.node_len = len;
            self.node_idx = len - 1;
            self.node = Some(leaf);

            self.next_back()
        }
    }
}

impl<'a, K: StableAllocated + Ord, V: StableAllocated> Iterator for SBTreeMapIter<'a, K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    type Item = (SRef<'a, K>, SRef<'a, V>);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(node) = &self.node {
            let k = SRef::new(node.get_key_ptr(self.node_idx));
            let v = SRef::new(node.get_value_ptr(self.node_idx));

            if self.node_idx == self.node_len - 1 {
                let next_ptr = u64::from_fixed_size_bytes(&node.read_next());

                if next_ptr == 0 {
                    return None;
                }

                let next = unsafe { LeafBTreeNode::<K, V>::from_ptr(next_ptr) };
                let len = next.read_len();

                self.node_idx = 0;
                self.node_len = len;
                self.node = Some(next);
            } else {
                self.node_idx += 1;
            }

            Some((k, v))
        } else {
            let mut node = unsafe { self.root.as_ref()?.copy() };
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

            self.node_len = leaf.read_len();
            self.node_idx = 0;
            self.node = Some(leaf);

            self.next()
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::btree_map::iter::SBTreeMapIter;

    #[test]
    fn test() {
        let it = None;
        let iter = SBTreeMapIter::<u64, u64>::new(&it, 10);
        assert_eq!(iter.len(), 10);
    }
}
