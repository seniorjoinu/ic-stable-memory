use crate::collections::btree_map::iter::SBTreeMapIter;
use crate::collections::btree_map::{BTreeNode, LeveledList, SBTreeMap};
use crate::encoding::AsFixedSizeBytes;
use crate::primitive::s_ref::SRef;
use crate::primitive::StableType;
use crate::utils::certification::{empty_hash, AsHashTree, AsHashableBytes, Hash, HashTree};
use std::borrow::Borrow;
use std::fmt::Debug;
use std::ops::Deref;

pub struct SCertifiedBTreeMap<
    K: StableType + AsFixedSizeBytes + Ord + AsHashableBytes,
    V: StableType + AsFixedSizeBytes + AsHashTree,
> {
    inner: SBTreeMap<K, V>,
    modified: LeveledList,
    frozen: bool,
}

impl<
        K: StableType + AsFixedSizeBytes + Ord + AsHashableBytes,
        V: StableType + AsFixedSizeBytes + AsHashTree,
    > SCertifiedBTreeMap<K, V>
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
    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        if !self.frozen {
            self.frozen = true;
        }

        self.inner._remove(key, &mut self.modified)
    }

    #[inline]
    pub fn remove_and_commit<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        let it = self.remove(key);
        self.commit();

        it
    }

    #[inline]
    pub fn get<Q>(&self, key: &Q) -> Option<SRef<'_, V>>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        self.inner.get(key)
    }

    #[inline]
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Ord,
    {
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

    #[inline]
    pub fn first(&self) -> Option<(SRef<K>, SRef<V>)> {
        self.inner.first()
    }

    #[inline]
    pub fn last(&self) -> Option<(SRef<K>, SRef<V>)> {
        self.inner.last()
    }

    pub fn commit(&mut self) {
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

    pub fn prove_absence<Q>(&self, index: &Q) -> HashTree
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        assert!(!self.frozen);

        let root_opt = self.inner.get_root();
        if root_opt.is_none() {
            return HashTree::Empty;
        }

        let node = unsafe { root_opt.unwrap_unchecked() };
        match node {
            BTreeNode::Internal(n) => match n.prove_absence::<V, Q>(index) {
                Ok(w) => w,
                Err(w) => w,
            },
            BTreeNode::Leaf(n) => {
                let len = n.read_len();
                let idx = match n.binary_search(index, len) {
                    Ok(_) => panic!("The key is present!"),
                    Err(idx) => idx,
                };

                match n.prove_absence(idx, len) {
                    Ok(w) => w,
                    Err(w) => w,
                }
            }
        }
    }

    pub fn prove_range<Q>(&self, from: &Q, to: &Q) -> HashTree
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        assert!(!self.frozen);
        assert!(from.le(to));

        let root_opt = self.inner.get_root();
        if root_opt.is_none() {
            return HashTree::Empty;
        }

        let node = unsafe { root_opt.unwrap_unchecked() };
        match node {
            BTreeNode::Internal(n) => n.prove_range::<V, Q>(from, to),
            BTreeNode::Leaf(n) => n.prove_range(from, to),
        }
    }

    pub fn as_hash_tree(&self) -> HashTree {
        let e1 = self.inner.first();
        let e2 = self.inner.last();

        match (e1, e2) {
            (None, None) => HashTree::Empty,
            (Some((k1, _)), Some((k2, _))) => self.prove_range(k1.deref(), k2.deref()),
            _ => unreachable!(),
        }
    }

    pub fn witness_with<Q, Fn: FnMut(&V) -> HashTree>(&self, index: &Q, f: Fn) -> HashTree
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        assert!(!self.frozen);

        let root_opt = self.inner.get_root();
        if root_opt.is_none() {
            return HashTree::Empty;
        }

        let node = unsafe { root_opt.unwrap_unchecked() };
        witness_node(&node, index, f)
    }
}

impl<
        K: StableType + AsFixedSizeBytes + Ord + AsHashableBytes,
        V: StableType + AsFixedSizeBytes + AsHashTree,
    > SCertifiedBTreeMap<K, V>
{
    #[inline]
    pub fn clear(&mut self) {
        self.frozen = false;
        self.modified = LeveledList::new();

        self.inner.clear();
    }
}

impl<
        K: StableType + AsFixedSizeBytes + Ord + AsHashableBytes,
        V: StableType + AsFixedSizeBytes + AsHashTree,
    > AsHashTree for SCertifiedBTreeMap<K, V>
{
    #[inline]
    fn root_hash(&self) -> Hash {
        self.inner
            .get_root()
            .map(|it| match it {
                BTreeNode::Internal(n) => n.root_hash(),
                BTreeNode::Leaf(n) => n.root_hash(),
            })
            .unwrap_or_else(empty_hash)
    }
}

fn witness_node<
    Q,
    K: StableType + AsFixedSizeBytes + Ord + AsHashableBytes,
    V: StableType + AsFixedSizeBytes + AsHashTree,
    Fn: FnMut(&V) -> HashTree,
>(
    node: &BTreeNode<K, V>,
    k: &Q,
    f: Fn,
) -> HashTree
where
    K: Borrow<Q>,
    Q: Ord,
{
    match node {
        BTreeNode::Internal(n) => {
            let len = n.read_len();
            let idx = match n.binary_search(k, len) {
                Ok(idx) => idx + 1,
                Err(idx) => idx,
            };

            let child =
                BTreeNode::<K, V>::from_ptr(u64::from_fixed_size_bytes(&n.read_child_ptr_buf(idx)));

            n.witness_with_replacement::<V>(idx, witness_node(&child, k, f), len)
        }
        BTreeNode::Leaf(n) => n.witness_with(k, f),
    }
}

impl<
        K: StableType + AsFixedSizeBytes + Ord + AsHashableBytes + Debug,
        V: StableType + AsFixedSizeBytes + AsHashTree + Debug,
    > SCertifiedBTreeMap<K, V>
{
    #[inline]
    pub fn debug_print(&self) {
        self.inner.debug_print();
        self.modified.debug_print();
    }
}

impl<
        K: StableType + AsFixedSizeBytes + Ord + AsHashableBytes,
        V: StableType + AsFixedSizeBytes + AsHashTree,
    > Default for SCertifiedBTreeMap<K, V>
{
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<
        K: StableType + AsFixedSizeBytes + Ord + AsHashableBytes,
        V: StableType + AsFixedSizeBytes + AsHashTree,
    > AsFixedSizeBytes for SCertifiedBTreeMap<K, V>
{
    const SIZE: usize = SBTreeMap::<K, V>::SIZE;
    type Buf = <SBTreeMap<K, V> as AsFixedSizeBytes>::Buf;

    fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
        assert!(!self.frozen);

        self.inner.as_fixed_size_bytes(buf)
    }

    fn from_fixed_size_bytes(buf: &[u8]) -> Self {
        let mut inner = SBTreeMap::<K, V>::from_fixed_size_bytes(buf);
        inner.set_certified(true);

        Self {
            inner,
            modified: LeveledList::new(),
            frozen: false,
        }
    }
}

impl<
        K: StableType + AsFixedSizeBytes + Ord + AsHashableBytes,
        V: StableType + AsFixedSizeBytes + AsHashTree,
    > StableType for SCertifiedBTreeMap<K, V>
{
    #[inline]
    unsafe fn assume_not_owned_by_stable_memory(&mut self) {
        self.inner.assume_not_owned_by_stable_memory();
    }

    #[inline]
    unsafe fn assume_owned_by_stable_memory(&mut self) {
        self.inner.assume_owned_by_stable_memory();
    }

    #[inline]
    fn is_owned_by_stable_memory(&self) -> bool {
        self.inner.is_owned_by_stable_memory()
    }

    #[inline]
    unsafe fn stable_drop(&mut self) {}
}

#[cfg(test)]
mod tests {
    use crate::collections::certified_btree_map::SCertifiedBTreeMap;
    use crate::utils::certification::{
        leaf, leaf_hash, traverse_hashtree, AsHashTree, AsHashableBytes, Hash, HashTree,
    };
    use crate::{stable, stable_memory_init};
    use rand::seq::SliceRandom;
    use rand::thread_rng;

    impl AsHashTree for u64 {
        fn root_hash(&self) -> Hash {
            leaf_hash(&self.to_le_bytes())
        }
    }

    impl AsHashableBytes for u64 {
        fn as_hashable_bytes(&self) -> Vec<u8> {
            self.to_le_bytes().to_vec()
        }
    }

    #[test]
    fn random_works_fine() {
        stable::clear();
        stable_memory_init();

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
                let wit = map.witness_with(&example[j], |it| leaf(it.as_hashable_bytes()));
                assert_eq!(
                    wit.reconstruct(),
                    map.root_hash(),
                    "invalid witness {:?}",
                    wit
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
                let wit = map.witness_with(&example[j], |it| leaf(it.as_hashable_bytes()));
                assert_eq!(
                    wit.reconstruct(),
                    map.root_hash(),
                    "invalid witness of {}: {:?}",
                    example[j],
                    wit
                );
            }
        }

        map.debug_print();
    }

    #[test]
    fn random_in_batches_works_fine() {
        stable::clear();
        stable_memory_init();

        let iterations = 10;
        let mut map = SCertifiedBTreeMap::<u64, u64>::default();

        let mut example = Vec::new();
        for i in 0..(iterations * 100) {
            example.push(i as u64);
        }
        example.shuffle(&mut thread_rng());

        for i in 0..iterations {
            for j in (i * 100)..((i + 1) * 100) {
                map.insert(example[j], example[j]);
            }

            map.commit();

            for j in 0..((i + 1) * 100) {
                let wit = map.witness_with(&example[j], |it| leaf(it.as_hashable_bytes()));
                assert_eq!(
                    wit.reconstruct(),
                    map.root_hash(),
                    "invalid witness {:?}",
                    wit
                );
            }
        }

        for i in 0..iterations {
            for j in (i * 100)..((i + 1) * 100) {
                map.remove(&example[j]);
            }

            map.commit();

            for j in ((i + 1) * 100)..(iterations * 100) {
                let wit = map.witness_with(&example[j], |it| leaf(it.as_hashable_bytes()));
                assert_eq!(
                    wit.reconstruct(),
                    map.root_hash(),
                    "invalid witness {:?}",
                    wit
                );
            }
        }
    }

    fn hash_tree_to_labeled_leaves(t: HashTree) -> Vec<HashTree> {
        let mut r = Vec::new();

        let mut l = |it: &HashTree| {
            if let HashTree::Labeled(_, _) = it {
                r.push(it.clone())
            }
        };

        traverse_hashtree(&t, &mut l);

        r
    }

    #[test]
    fn absence_proofs_work_fine() {
        stable::clear();
        stable_memory_init();

        let iterations = 100;
        let mut map = SCertifiedBTreeMap::<u64, u64>::default();

        let proof = map.prove_absence(&0);

        assert_eq!(
            proof.reconstruct(),
            map.root_hash(),
            "invalid proof {:?}",
            proof
        );

        let leaves = hash_tree_to_labeled_leaves(proof);
        assert_eq!(leaves.len(), 0);

        for i in 1..iterations {
            map.insert(i * 2, i * 2);
        }

        map.commit();
        map.debug_print();

        let proof = map.prove_absence(&0);

        assert_eq!(
            proof.reconstruct(),
            map.root_hash(),
            "invalid proof {:?}",
            proof
        );

        let leaves = hash_tree_to_labeled_leaves(proof);
        assert_eq!(leaves.len(), 1);

        for i in 1..(iterations - 1) {
            let proof = map.prove_absence(&(i * 2 + 1));

            assert_eq!(
                proof.reconstruct(),
                map.root_hash(),
                "invalid proof {:?}",
                proof
            );

            let leaves = hash_tree_to_labeled_leaves(proof);
            assert_eq!(leaves.len(), 2);
        }

        let proof = map.prove_absence(&300);

        assert_eq!(
            proof.reconstruct(),
            map.root_hash(),
            "invalid proof {:?}",
            proof
        );

        let leaves = hash_tree_to_labeled_leaves(proof);
        assert_eq!(leaves.len(), 1);

        for i in 1..iterations {
            map.remove(&(i * 2));
        }

        map.commit();

        let proof = map.prove_absence(&0);

        assert_eq!(
            proof.reconstruct(),
            map.root_hash(),
            "invalid proof {:?}",
            proof
        );

        let leaves = hash_tree_to_labeled_leaves(proof);
        assert!(leaves.is_empty());
    }

    #[test]
    fn range_proofs_work_fine() {
        stable::clear();
        stable_memory_init();

        let iterations = 100;
        let mut map = SCertifiedBTreeMap::<u64, u64>::default();

        for i in 0..iterations {
            map.insert(i, i);
        }

        map.commit();
        map.debug_print();

        for i in 0..iterations {
            for j in i..iterations {
                let proof = map.prove_range(&i, &j);

                assert_eq!(
                    proof.reconstruct(),
                    map.root_hash(),
                    "invalid proof {:?}",
                    proof
                );

                let leaves = hash_tree_to_labeled_leaves(proof);
                assert_eq!(leaves.len() as u64, j - i + 1, "{} {}", i, j);
            }
        }
    }

    #[test]
    fn nested_maps_work_fine() {
        stable::clear();
        stable_memory_init();

        let mut map = SCertifiedBTreeMap::<u64, SCertifiedBTreeMap<u64, u64>>::default();

        let mut nested_map_1 = SCertifiedBTreeMap::default();
        let mut nested_map_2 = SCertifiedBTreeMap::default();

        nested_map_1.insert(1, 1);
        nested_map_1.commit();

        nested_map_2.insert(2, 2);
        nested_map_2.commit();

        map.insert(1, nested_map_1);
        map.insert(2, nested_map_2);

        map.commit();

        let composite_witness = map.witness_with(&1, |it| {
            it.witness_with(&1, |it1| leaf(it1.as_hashable_bytes()))
        });

        assert_eq!(
            composite_witness.reconstruct(),
            map.root_hash(),
            "invalid witness {:?}",
            composite_witness
        );

        let mut label_1_met = false;
        let mut label_2_met = false;
        let mut leave_met = false;

        traverse_hashtree(&composite_witness, &mut |it| match it {
            HashTree::Labeled(l, _) => {
                assert!(1u64.as_hashable_bytes().eq(l));

                if !label_1_met {
                    label_1_met = true;
                } else if !label_2_met {
                    label_2_met = true;
                } else {
                    panic!("Extra label met");
                }
            }
            HashTree::Leaf(l) => {
                if !leave_met {
                    leave_met = true;
                } else {
                    panic!("Extra leave met");
                }

                assert!(1u64.as_hashable_bytes().eq(l));
            }
            _ => {}
        });

        assert!(label_1_met);
        assert!(label_2_met);
        assert!(leave_met);
    }
}
