use crate::collections::btree_map::iter::SBTreeMapIter;
use crate::collections::btree_map::{BTreeNode, LeveledList, SBTreeMap};
use crate::primitive::StableAllocated;
use crate::utils::certification::{leaf, AsHashTree, AsHashableBytes, Hash, HashTree, EMPTY_HASH};
use crate::utils::encoding::{AsFixedSizeBytes, FixedSize};
use std::fmt::Debug;

pub struct SCertifiedBTreeMap<K, V> {
    inner: SBTreeMap<K, V>,
    modified: LeveledList,
    frozen: bool,
}

impl<K: StableAllocated + Ord + AsHashableBytes, V: StableAllocated + AsHashableBytes>
    SCertifiedBTreeMap<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    #[inline]
    pub fn new() -> Self {
        Self {
            inner: SBTreeMap::new_certified(),
            modified: LeveledList::new(),
            frozen: false,
        }
    }

    #[inline]
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        if !self.frozen {
            self.frozen = true;
        }

        self.inner._insert(key, value, &mut self.modified)
    }

    #[inline]
    pub fn insert_and_commit(&mut self, key: K, value: V) -> Option<V> {
        let it = self.insert(key, value);
        self.commit();

        it
    }

    #[inline]
    pub fn remove(&mut self, key: &K) -> Option<V> {
        if !self.frozen {
            self.frozen = true;
        }

        self.inner._remove(key, &mut self.modified)
    }

    #[inline]
    pub fn remove_and_commit(&mut self, key: &K) -> Option<V> {
        let it = self.remove(key);
        self.commit();

        it
    }

    #[inline]
    pub fn witness(&self, key: &K) -> HashTree {
        self.witness_with(key, |it| leaf(it.as_hashable_bytes()))
    }

    #[inline]
    pub fn get_copy(&self, key: &K) -> Option<V> {
        self.inner.get_copy(key)
    }

    #[inline]
    pub fn contains_key(&self, key: &K) -> bool {
        self.inner.contains_key(key)
    }

    #[inline]
    pub fn len(&self) -> u64 {
        self.inner.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    #[inline]
    pub fn iter(&self) -> SBTreeMapIter<'_, K, V> {
        self.inner.iter()
    }
}

impl<K: StableAllocated + Ord + AsHashableBytes, V: StableAllocated + AsHashableBytes>
    AsHashTree<V, &K> for SCertifiedBTreeMap<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    #[inline]
    fn root_hash(&self) -> Hash {
        self.inner
            .get_root()
            .map(|it| match it {
                BTreeNode::Internal(n) => n.root_hash(),
                BTreeNode::Leaf(n) => n.root_hash(),
            })
            .unwrap_or(EMPTY_HASH)
    }

    fn commit(&mut self) {
        if !self.frozen {
            return;
        }
        self.frozen = false;

        while let Some(ptr) = self.modified.pop() {
            let mut node = BTreeNode::<K, V>::from_ptr(ptr);
            match &mut node {
                BTreeNode::Internal(n) => n.commit::<V>(),
                BTreeNode::Leaf(n) => n.commit(),
            };
        }
    }

    fn witness_with<Fn: FnMut(&V) -> HashTree>(&self, index: &K, f: Fn) -> HashTree {
        assert!(!self.frozen);

        let root_opt = self.inner.get_root();
        if root_opt.is_none() {
            return HashTree::Empty;
        }

        let node = unsafe { root_opt.unwrap_unchecked() };
        witness_node(&node, index, f)
    }
}

fn witness_node<
    K: StableAllocated + Ord + AsHashableBytes,
    V: StableAllocated + AsHashableBytes,
    Fn: FnMut(&V) -> HashTree,
>(
    node: &BTreeNode<K, V>,
    k: &K,
    f: Fn,
) -> HashTree
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    match node {
        BTreeNode::Internal(n) => {
            let len = n.read_len();
            let idx = match n.binary_search(k, len) {
                Ok(idx) => idx + 1,
                Err(idx) => idx,
            };

            let child =
                BTreeNode::<K, V>::from_ptr(u64::from_fixed_size_bytes(&n.read_child_ptr(idx)));
            
            n.witness_with_replacement::<V>(idx, witness_node(&child, k, f), len)
        }
        BTreeNode::Leaf(n) => n.witness_with(k, f),
    }
}

impl<
        K: StableAllocated + Ord + AsHashableBytes + Debug,
        V: StableAllocated + AsHashableBytes + Debug,
    > SCertifiedBTreeMap<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    pub fn debug_print(&self) {
        self.inner.debug_print();
        self.modified.debug_print();
    }
}

impl<K: StableAllocated + Ord + AsHashableBytes, V: StableAllocated + AsHashableBytes> Default
    for SCertifiedBTreeMap<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> FixedSize for SCertifiedBTreeMap<K, V> {
    const SIZE: usize = SBTreeMap::<K, V>::SIZE;
}

impl<K, V> AsFixedSizeBytes for SCertifiedBTreeMap<K, V> {
    fn as_fixed_size_bytes(&self) -> [u8; Self::SIZE] {
        assert!(!self.frozen);

        self.inner.as_fixed_size_bytes()
    }

    fn from_fixed_size_bytes(buf: &[u8; Self::SIZE]) -> Self {
        let mut inner = SBTreeMap::<K, V>::from_fixed_size_bytes(buf);
        inner.certified = true;

        Self {
            inner,
            modified: LeveledList::new(),
            frozen: false,
        }
    }
}

impl<K: StableAllocated + Ord, V: StableAllocated> StableAllocated for SCertifiedBTreeMap<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    #[inline]
    fn move_to_stable(&mut self) {}

    #[inline]
    fn remove_from_stable(&mut self) {}

    unsafe fn stable_drop(self) {
        self.inner.stable_drop()
    }
}

impl<K: StableAllocated + Ord + AsHashableBytes, V: StableAllocated + AsHashableBytes>
    AsHashableBytes for SCertifiedBTreeMap<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    fn as_hashable_bytes(&self) -> Vec<u8> {
        self.root_hash().as_hashable_bytes()
    }
}

/// Fork((
///     Fork((
///         Fork((
///             Fork((
///                 Fork((
///                     Fork((
///                         Fork((
///                             Fork((
///                                 Pruned([179, 153, 113, 149, 6, 68, 157, 185, 96, 229, 107, 14, 64, 77, 84, 134, 167, 253, 118, 215, 235, 117, 162, 150, 22, 213, 109, 143, 161, 249, 123, 131]),
///                                 Pruned([6, 176, 218, 104, 173, 204, 111, 91, 223, 238, 229, 110, 221, 104, 138, 231, 220, 115, 158, 117, 0, 162, 29, 211, 91, 140, 205, 30, 200, 250, 113, 52])
///                             )),
///                             Pruned([252, 71, 197, 180, 249, 128, 172, 93, 249, 113, 181, 211, 16, 96, 134, 204, 233, 172, 133, 22, 248, 16, 103, 70, 136, 224, 51, 35, 42, 161, 100, 67])
///                         )),
///                         Pruned([54, 249, 66, 10, 250, 105, 63, 79, 163, 228, 47, 187, 189, 227, 150, 178, 111, 153, 151, 173, 220, 65, 156, 188, 130, 148, 216, 100, 10, 100, 30, 139])
///                     )),
///                     Pruned([69, 132, 192, 122, 232, 90, 228, 49, 11, 119, 133, 93, 26, 92, 46, 59, 85, 18, 107, 61, 198, 229, 207, 93, 32, 101, 75, 26, 206, 206, 40, 158])
///                 )),
///                 Pruned([206, 105, 91, 24, 64, 112, 126, 25, 89, 168, 162, 228, 153, 105, 185, 82, 144, 2, 47, 70, 205, 171, 225, 10, 1, 239, 248, 231, 119, 200, 111, 18])
///             )),
///             Labeled([248, 1, 0, 0, 0, 0, 0, 0], Leaf([248, 1, 0, 0, 0, 0, 0, 0]))
///         )),
///         Pruned([223, 196, 169, 88, 33, 92, 93, 206, 157, 1, 218, 214, 123, 245, 238, 124, 142, 41, 74, 129, 163, 28, 28, 103, 8, 36, 62, 220, 22, 54, 116, 208])
///     )),
///     Pruned([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
/// ))

#[cfg(test)]
mod tests {
    use crate::collections::certified_btree_map::SCertifiedBTreeMap;
    use crate::primitive::StableAllocated;
    use crate::utils::certification::{AsHashTree, AsHashableBytes, HashTree};
    use crate::utils::encoding::AsFixedSizeBytes;
    use crate::{get_allocated_size, init_allocator, stable};
    use rand::seq::SliceRandom;
    use rand::thread_rng;

    impl AsHashableBytes for u64 {
        fn as_hashable_bytes(&self) -> Vec<u8> {
            self.to_le_bytes().to_vec()
        }
    }

    #[test]
    fn random_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let iterations = 1000;
        let mut map = SCertifiedBTreeMap::<u64, u64>::default();

        let mut example = Vec::new();
        for i in 0..iterations {
            example.push(i as u64);
        }
        example.shuffle(&mut thread_rng());

        for i in 0..iterations {
            assert!(map.insert(example[i], example[i]).is_none());
            map.inner.debug_print();

            map.modified.debug_print();
            map.commit();
            map.modified.debug_print();
            println!();

            for j in 0..i {
                let wit = map.witness(&example[j]);
                assert_eq!(
                    wit.reconstruct(),
                    map.root_hash(),
                    "invalid witness {:?}",
                    wit
                );
                assert!(
                    map.contains_key(&example[j]),
                    "don't contain {}",
                    example[j]
                );
                assert_eq!(
                    map.get_copy(&example[j]),
                    Some(example[j]),
                    "unable to get {}",
                    example[j]
                );
            }
        }

        assert_eq!(map.len(), iterations as u64);
        assert_eq!(map.is_empty(), false);

        map.debug_print();
        println!();
        println!();

        assert_eq!(map.insert(0, 1).unwrap(), 0);
        assert_eq!(map.insert(0, 0).unwrap(), 1);

        example.shuffle(&mut thread_rng());
        for i in 0..iterations {
            assert_eq!(map.remove_and_commit(&example[i]), Some(example[i]));

            for j in (i + 1)..iterations {
                let wit = map.witness(&example[j]);
                assert_eq!(
                    wit.reconstruct(),
                    map.root_hash(),
                    "invalid witness of {}: {:?}",
                    example[j],
                    wit
                );
                assert!(
                    map.contains_key(&example[j]),
                    "don't contain {}",
                    example[j]
                );
                assert_eq!(
                    map.get_copy(&example[j]),
                    Some(example[j]),
                    "unable to get {}",
                    example[j]
                );
            }
        }

        map.debug_print();
    }
}
