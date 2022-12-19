use crate::collections::b_plus_tree_map::leaf_node::LeafBTreeNode;
use crate::collections::b_plus_tree_map::SBTreeMap;
use crate::primitive::StableAllocated;
use crate::utils::encoding::AsFixedSizeBytes;

pub struct SBTreeMapIter<'a, K, V> {
    map: &'a SBTreeMap<K, V>,
    node: Option<LeafBTreeNode<K, V>>,
    idx: usize,
    len: usize,
}

impl<'a, K, V> SBTreeMapIter<'a, K, V> {
    pub fn new(map: &'a SBTreeMap<K, V>) -> Self {
        Self {
            map,
            node: None,
            idx: 0,
            len: 0,
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
        if let Some(node) = self.node {
            let k = K::from_fixed_size_bytes(&node.read_key(self.idx));
            let v = V::from_fixed_size_bytes(&node.read_value(self.idx));

            if self.idx == self.len - 1 {
                let next_ptr = u64::from_fixed_size_bytes(&node.read_next());
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
            if self.map.root
        }
    }
}
