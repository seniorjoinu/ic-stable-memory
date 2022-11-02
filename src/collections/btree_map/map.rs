use crate::collections::btree_map::node::{BTreeNode, MIN_LEN_AFTER_SPLIT};
use crate::primitive::StableAllocated;
use copy_as_bytes::traits::{AsBytes, SuperSized};

pub struct SBTreeMap<K, V> {
    root: Option<BTreeNode<K, V>>,
    len: u64,
}

impl<K: StableAllocated + Ord, V: StableAllocated> SBTreeMap<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
    [(); MIN_LEN_AFTER_SPLIT * K::SIZE]: Sized,
    [(); MIN_LEN_AFTER_SPLIT * V::SIZE]: Sized,
{
    pub fn insert(&mut self, mut k: K, mut v: V) -> Option<V> {
        let mut node = self.get_root_mut();

        let mut new_node = loop {
            match node.insert_down(k, v) {
                Ok(res) => match res {
                    Ok(it) => {
                        if it.is_none() {
                            self.len += 1;
                        }

                        return it;
                    }
                    Err((_k, _v, _new_node)) => {
                        if self.get_root_mut().as_ptr() == node.as_ptr() {
                            node.set_is_root(false);

                            let mut new_root = BTreeNode::<K, V>::new(false, true);
                            new_root.set_key(0, _k);
                            new_root.set_value(0, _v);
                            new_root.set_len(1);
                            new_root.set_child_ptr(0, node.as_ptr());
                            new_root.set_child_ptr(1, _new_node.as_ptr());

                            self.root = Some(new_root);
                            self.len += 1;

                            return None;
                        }

                        k = _k;
                        v = _v;
                        node = unsafe { BTreeNode::<K, V>::from_ptr(node.get_parent()) };

                        break _new_node;
                    }
                },
                Err((_k, _v, _node)) => {
                    k = _k;
                    v = _v;
                    node = _node;
                }
            }
        };

        loop {
            match node.insert_up(k, v, new_node) {
                Ok(_) => {
                    self.len += 1;
                    return None;
                }
                Err((_k, _v, _new_node)) => {
                    if self.get_root_mut().as_ptr() == node.as_ptr() {
                        node.set_is_root(false);

                        let mut new_root = BTreeNode::<K, V>::new(false, true);
                        new_root.set_key(0, _k);
                        new_root.set_value(0, _v);
                        new_root.set_len(1);
                        new_root.set_child_ptr(0, node.as_ptr());
                        new_root.set_child_ptr(1, _new_node.as_ptr());

                        self.root = Some(new_root);
                        self.len += 1;

                        return None;
                    }

                    k = _k;
                    v = _v;
                    new_node = _new_node;
                    node = unsafe { BTreeNode::<K, V>::from_ptr(node.get_parent()) };
                }
            }
        }
    }

    pub fn delete(&mut self, k: &K) -> Option<V> {
        let mut parent: Option<BTreeNode<K, V>> = None;
        let mut parent_idx: Option<usize> = None;
        let mut parent_len: Option<usize> = None;

        let mut node = self.get_root_mut();

        loop {
            let len = node.len();
            let is_leaf = node.is_leaf();

            match node.find_idx(k, len) {
                Ok(idx) => {
                    if is_leaf {
                        // if it is possible to remove without violation - do it
                        if len > MIN_LEN_AFTER_SPLIT || parent.is_none() {
                            let mut k = node.get_key(idx);
                            let mut v = node.get_value(idx);

                            node.remove_key(idx, len);
                            node.remove_value(idx, len);

                            k.remove_from_stable();
                            v.remove_from_stable();

                            node.set_len(len - 1);

                            return Some(v);
                        }

                        // if it is impossible to simply remove the element without violating the min-len constraint

                        let p = unsafe { parent.unwrap_unchecked() };
                        let p_idx = unsafe { parent_idx.unwrap_unchecked() };
                        let p_len = unsafe { parent_len.unwrap_unchecked() };

                        let (mut k, mut v) =
                            BTreeNode::delete_in_violating_leaf(node, p, p_idx, p_len, idx, 0);

                        k.remove_from_stable();
                        v.remove_from_stable();

                        return Some(v);
                    }

                    // go to left subtree's max child or to right subtree's min child
                    let mut child = unsafe {
                        BTreeNode::<K, V>::from_ptr(node.get_child_ptr(
                            if idx > MIN_LEN_AFTER_SPLIT {
                                idx
                            } else {
                                idx + 1
                            },
                        ))
                    };

                    let mut child_parent = unsafe { node.copy() };
                    let mut child_p_idx = idx;
                    let mut child_p_len = len;

                    let mut child_is_leaf = child.is_leaf();
                    let mut child_len = child.len();

                    loop {
                        if !child_is_leaf {
                            child_is_leaf = child.is_leaf();
                            child_len = child.len();
                            child_parent = child;
                            child_p_idx = child_len;
                            child_p_len = child_len;

                            child = unsafe {
                                BTreeNode::<K, V>::from_ptr(child_parent.get_child_ptr(
                                    if idx > MIN_LEN_AFTER_SPLIT {
                                        child_len
                                    } else {
                                        0
                                    },
                                ))
                            };

                            continue;
                        }

                        break;
                    }

                    let mut k = node.get_key(idx);
                    let mut v = node.get_value(idx);

                    k.remove_from_stable();
                    v.remove_from_stable();

                    let child_idx = if idx > MIN_LEN_AFTER_SPLIT {
                        child_len - 1
                    } else {
                        0
                    };

                    let (replace_k, replace_v) = if child_len > MIN_LEN_AFTER_SPLIT {
                        let replace_k = child.get_key(child_idx);
                        let replace_v = child.get_value(child_idx);

                        child.remove_key(child_idx, child_len);
                        child.remove_value(child_idx, child_len);
                        child.set_len(child_len - 1);

                        (replace_k, replace_v)
                    } else {
                        BTreeNode::delete_in_violating_leaf(
                            child,
                            child_parent,
                            child_p_idx,
                            child_p_len,
                            child_idx,
                            node.as_ptr(),
                        )
                    };

                    node.set_key(idx, replace_k);
                    node.set_value(idx, replace_v);

                    return Some(v);
                }
                Err(idx) => {
                    if is_leaf {
                        return None;
                    }

                    parent = unsafe { Some(node.copy()) };
                    parent_len = Some(len);
                    parent_idx = Some(idx);

                    node = unsafe { BTreeNode::<K, V>::from_ptr(node.get_child_ptr(idx)) };
                }
            }
        }
    }

    pub fn get_copy(&self, k: &K) -> Option<V> {
        let mut node = self.root.as_ref().map(|it| unsafe { it.copy() })?;
        let mut len = node.len();

        loop {
            match node.find_idx(k, len) {
                Ok(idx) => return Some(node.get_value(idx)),
                Err(idx) => {
                    if node.is_leaf() {
                        return None;
                    } else {
                        node = unsafe { BTreeNode::<K, V>::from_ptr(node.get_child_ptr(idx)) };
                        len = node.len();
                    }
                }
            };
        }
    }

    pub fn contains_key(&self, k: &K) -> bool {
        if let Some(mut node) = self.root.as_ref().map(|it| unsafe { it.copy() }) {
            let mut len = node.len();

            loop {
                match node.find_idx(k, len) {
                    Ok(_) => return true,
                    Err(idx) => {
                        if node.is_leaf() {
                            return false;
                        } else {
                            node = unsafe { BTreeNode::<K, V>::from_ptr(node.get_child_ptr(idx)) };
                            len = node.len();
                        }
                    }
                };
            }
        } else {
            false
        }
    }

    fn get_root_mut(&mut self) -> BTreeNode<K, V> {
        if let Some(r) = self.root.as_ref().map(|it| unsafe { it.copy() }) {
            r
        } else {
            self.root = Some(BTreeNode::<K, V>::new(true, true));
            self.get_root_mut()
        }
    }
}
