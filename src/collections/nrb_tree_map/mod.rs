use crate::collections::nrb_tree_map::node::NRBTreeNode;
use crate::mem::allocator::EMPTY_PTR;
use crate::primitive::StableAllocated;
use crate::utils::phantom_data::SPhantomData;

pub mod node;

pub struct NRBTreeMap<K, V> {
    root: Option<NRBTreeNode<K, V>>,
    len: u64,
    _marker_k: SPhantomData<K>,
    _marker_v: SPhantomData<V>,
}

impl<K: StableAllocated + Ord, V: StableAllocated> NRBTreeMap<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    pub fn new() -> Self {
        Self {
            root: None,
            len: 0,
            _marker_k: SPhantomData::default(),
            _marker_v: SPhantomData::default(),
        }
    }

    pub fn contains_key(&self, k: &K) -> bool {
        if let Some(root) = &self.root {
            let mut node = unsafe { NRBTreeNode::<K, V>::from_ptr(root.as_ptr()) };

            loop {
                match node.contains_key(k) {
                    Ok(opt) => return opt,
                    Err(to_left) => {
                        let ptr = if to_left {
                            node.get_left()
                        } else {
                            node.get_right()
                        };

                        if ptr == 0 {
                            return false;
                        }

                        node = unsafe { NRBTreeNode::<K, V>::from_ptr(ptr) };
                    }
                }
            }
        } else {
            false
        }
    }

    pub fn get_copy(&self, k: &K) -> Option<V> {
        if let Some(root) = &self.root {
            let mut node = unsafe { NRBTreeNode::<K, V>::from_ptr(root.as_ptr()) };

            loop {
                match node.get(k) {
                    Ok(opt) => return opt,
                    Err(to_left) => {
                        let ptr = if to_left {
                            node.get_left()
                        } else {
                            node.get_right()
                        };

                        if ptr == 0 {
                            return None;
                        }

                        node = unsafe { NRBTreeNode::<K, V>::from_ptr(ptr) };
                    }
                }
            }
        } else {
            None
        }
    }

    pub fn remove(&mut self, k: &K) -> Option<V> {
        if let Some(root) = &self.root {
            let mut node = unsafe { NRBTreeNode::<K, V>::from_ptr(root.as_ptr()) };

            loop {
                match node.remove(k) {
                    Ok(opt) => return opt,
                    Err(to_left) => {
                        let ptr = if to_left {
                            node.get_left()
                        } else {
                            node.get_right()
                        };

                        if ptr == 0 {
                            return None;
                        }

                        node = unsafe { NRBTreeNode::<K, V>::from_ptr(ptr) };
                    }
                }
            }
        } else {
            None
        }
    }

    pub fn insert(&mut self, mut k: K, mut v: V) -> Option<V> {
        let mut node = if let Some(root) = &self.root {
            unsafe { NRBTreeNode::from_ptr(root.as_ptr()) }
        } else {
            let node = NRBTreeNode::new();
            let ptr = node.as_ptr();

            self.root = Some(node);

            unsafe { NRBTreeNode::from_ptr(ptr) }
        };

        loop {
            match node.insert(k, v) {
                Ok(opt) => return opt,
                Err((_k, _v, to_left)) => {
                    k = _k;
                    v = _v;

                    let ptr = if to_left {
                        node.get_left()
                    } else {
                        node.get_right()
                    };

                    node = if ptr == 0 {
                        let mut new_node = NRBTreeNode::new();
                        let new_ptr = new_node.as_ptr();

                        if to_left {
                            node.set_left(new_ptr);
                        } else {
                            node.set_right(new_ptr);
                        }

                        new_node.set_parent(node.as_ptr());

                        let left = new_node.get_left();
                        let right = new_node.get_right();
                        let parent = new_node.get_parent();

                        new_node
                    } else {
                        unsafe { NRBTreeNode::from_ptr(ptr) }
                    }
                }
            }
        }
    }
}

impl<K: StableAllocated + Ord, V: StableAllocated> Default for NRBTreeMap<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    fn default() -> Self {
        Self::new()
    }
}
