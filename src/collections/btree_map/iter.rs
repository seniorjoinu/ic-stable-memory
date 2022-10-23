use crate::collections::btree_map::{BTreeNode, SBTreeMap};
use crate::primitive::StableAllocated;
use copy_as_bytes::traits::{AsBytes, SuperSized};

pub struct SBTreeMapIter<'a, K, V> {
    map: &'a SBTreeMap<K, V>,
    stack: Option<Vec<(BTreeNode<K, V>, usize)>>,
}

impl<'a, K, V> SBTreeMapIter<'a, K, V> {
    pub fn new(map: &'a SBTreeMap<K, V>) -> Self {
        Self { map, stack: None }
    }
}

impl<'a, K: StableAllocated, V: StableAllocated> SBTreeMapIter<'a, K, V>
where
    [(); BTreeNode::<K, V>::SIZE]: Sized,
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
    BTreeNode<K, V>: StableAllocated,
{
    fn find_smallest_child(
        node: BTreeNode<K, V>,
    ) -> Result<BTreeNode<K, V>, (BTreeNode<K, V>, BTreeNode<K, V>)> {
        if node.children.len() == 0 {
            return Ok(node);
        }

        let child = node.children.get_copy(0);

        Err((node, unsafe { child.unwrap_unchecked() }))
    }
}

impl<'a, K: StableAllocated, V: StableAllocated> Iterator for SBTreeMapIter<'a, K, V>
where
    [(); BTreeNode::<K, V>::SIZE]: Sized,
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
    BTreeNode<K, V>: StableAllocated,
{
    type Item = (K, V);

    // iterating using a stack of btree nodes (a tree branch basically)
    fn next(&mut self) -> Option<Self::Item> {
        let stack = if let Some(stack) = &mut self.stack {
            stack
        } else {
            let mut stack = Vec::new();
            let mut node = unsafe { self.map.root.unsafe_clone() };

            loop {
                match Self::find_smallest_child(node) {
                    Ok(smallest_node) => {
                        stack.push((smallest_node, 0));
                        break;
                    }
                    Err((prev_node, next_node)) => {
                        stack.push((prev_node, 0));
                        node = next_node;
                    }
                };
            }

            self.stack = Some(stack);

            unsafe { self.stack.as_mut().unwrap_unchecked() }
        };

        if let Some((last_node, idx)) = stack.last_mut() {
            if last_node.keys.len().eq(idx) {
                stack.pop();

                return self.next();
            }

            let k = unsafe { last_node.keys.get_copy(*idx).unwrap_unchecked() };
            let v = unsafe { last_node.values.get_copy(*idx).unwrap_unchecked() };

            *idx += 1;

            if let Some(child) = last_node.children.get_copy(*idx) {
                stack.push((child, 0));
            }

            Some((k, v))
        } else {
            None
        }
    }
}
