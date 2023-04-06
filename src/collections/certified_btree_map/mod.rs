use crate::collections::btree_map::internal_node::InternalBTreeNode;
use crate::collections::btree_map::iter::SBTreeMapIter;
use crate::collections::btree_map::leaf_node::LeafBTreeNode;
use crate::collections::btree_map::{BTreeNode, LeveledList, SBTreeMap};
use crate::encoding::AsFixedSizeBytes;
use crate::primitive::s_ref::SRef;
use crate::primitive::s_ref_mut::SRefMut;
use crate::primitive::StableType;
use crate::utils::certification::{
    empty_hash, labeled, labeled_hash, pruned, AsHashTree, AsHashableBytes, Hash, HashForker,
    HashTree, WitnessForker,
};
use std::borrow::Borrow;
use std::fmt::{Debug, Formatter};
use std::ops::Deref;

/// Merkle tree certified map on top of [SBTreeMap]
///
/// All logic, not related to the undelying Merkle tree is simply proxied from the underlying [SBTreeMap],
/// read its documentation for more details.
///
/// This Merkle tree provides various proofs in a form of [HashTree] data structure that is completely
/// compatible with [Dfinity's ic-certified-map](https://github.com/dfinity/cdk-rs/tree/main/library/ic-certified-map),
/// which means that you can verify data, certified with [SCertifiedBTreeMap] using [agent-js library](https://github.com/dfinity/agent-js).
///
/// Both `K` and `V` have to implement [StableType] and [AsFixedSizeBytes] traits. [SCertifiedBTreeMap]
/// also implements both these traits, so you can nest it into other stable structures. `K` also has
/// to implement [AsHashableBytes] trait. `V` also has to implement [AsHashTree] trait. [SCertifiedBTreeMap]
/// also implements [AsHashTree], so you can nest it into itself.
///
/// For a real-world example of how to use this data stucture, visit [this repository](https://github.com/seniorjoinu/ic-stable-certified-assets).
///
/// Features:
/// 1. You can nest multiple [SCertifiedBTreeMap]s into each other to create more complex Merkle trees.
/// 2. O(logN) perfromance and proof size.
/// 3. Batch API - modify the map multiple times, but recalculate the underlying Merkle tree only once.
/// 4. Witnesses of a single key-value pair, range proofs and proofs of absence of key are supported.
///
/// # Examples
/// ```rust
/// # use std::borrow::Borrow;
/// # use ic_stable_memory::collections::SCertifiedBTreeMap;
/// # use ic_stable_memory::{leaf, stable_memory_init};
/// # use ic_stable_memory::utils::certification::{AsHashableBytes, AsHashTree, leaf_hash, Hash, HashTree};
/// # use ic_stable_memory::derive::{StableType, AsFixedSizeBytes};
/// # unsafe { ic_stable_memory::mem::clear(); }
/// # stable_memory_init();
///
/// // create a wrapping structure, to be able to implement custom traits
/// #[derive(StableType, AsFixedSizeBytes, Ord, PartialOrd, Eq, PartialEq, Debug)]
/// struct WrappedNumber(u64);
///
/// // implement borrow, to be able to use &u64 for search
/// impl Borrow<u64> for WrappedNumber {
///     fn borrow(&self) -> &u64 {
///         &self.0
///     }
/// }
///
/// // implement AsHashableBytes to be able to use the type as a key
/// impl AsHashableBytes for WrappedNumber {
///     fn as_hashable_bytes(&self) -> Vec<u8> {
///         self.0.to_le_bytes().to_vec()
///     }
/// }
///
/// // implement AsHashTree to be able to use the type as a value
/// impl AsHashTree for WrappedNumber {
///     fn root_hash(&self) -> Hash {
///         leaf_hash(&self.0.to_le_bytes())
///     }
///
///     fn hash_tree(&self) -> HashTree {
///         leaf(self.0.to_le_bytes().to_vec())
///     }
/// }
///
/// // create the map
/// let mut map = SCertifiedBTreeMap::<WrappedNumber, WrappedNumber>::new();
///
/// // insert some values in one batch
/// map.insert(
///     WrappedNumber(1),
///     WrappedNumber(1)
/// ).expect("Out of memory");
///
/// map.insert(
///     WrappedNumber(2),
///     WrappedNumber(2)
/// ).expect("Out of memory");
///
/// map.insert(
///     WrappedNumber(3),
///     WrappedNumber(3)
/// ).expect("Out of memory");
///
/// // recalculate the Merkle tree
/// map.commit();
///
/// // prove that there is a value by "2" key
/// let witness = map.witness(&2);
/// assert_eq!(witness.reconstruct(), map.root_hash());
///
/// // prove that there is no key "5"
/// let absence_proof = map.prove_absence(&5);
/// assert_eq!(absence_proof.reconstruct(), map.root_hash());
///
/// // prove that all three keys are there
/// let range_proof = map.prove_range(&1, &3);
/// assert_eq!(range_proof.reconstruct(), map.root_hash());
/// ```
///
/// Another example with nested maps
/// ```rust
/// # use std::borrow::Borrow;
/// # use ic_stable_memory::collections::SCertifiedBTreeMap;
/// # use ic_stable_memory::{leaf, stable_memory_init};
/// # use ic_stable_memory::utils::certification::{AsHashableBytes, AsHashTree, leaf_hash, Hash, HashTree};
/// # use ic_stable_memory::derive::{StableType, AsFixedSizeBytes};
/// # unsafe { ic_stable_memory::mem::clear(); }
/// # stable_memory_init();
/// # #[derive(StableType, AsFixedSizeBytes, Ord, PartialOrd, Eq, PartialEq, Debug)]
/// # struct WrappedNumber(u64);
/// # impl Borrow<u64> for WrappedNumber {
/// #     fn borrow(&self) -> &u64 {
/// #         &self.0
/// #     }
/// # }
/// # impl AsHashableBytes for WrappedNumber {
/// #     fn as_hashable_bytes(&self) -> Vec<u8> {
/// #         self.0.to_le_bytes().to_vec()
/// #     }
/// # }
/// # impl AsHashTree for WrappedNumber {
/// #     fn root_hash(&self) -> Hash {
/// #         leaf_hash(&self.0.to_le_bytes())
/// #     }
/// #     fn hash_tree(&self) -> HashTree {
/// #         leaf(self.0.to_le_bytes().to_vec())
/// #     }
/// # }
/// // same setup as in previous example
/// // create the outer map
/// let mut outer_map = SCertifiedBTreeMap::new();
///
/// // create a couple of nested maps
/// let mut map_1 = SCertifiedBTreeMap::<WrappedNumber, WrappedNumber>::new();
/// let mut map_2 = SCertifiedBTreeMap::<WrappedNumber, WrappedNumber>::new();
///
/// // nest maps
/// outer_map.insert(WrappedNumber(1), map_1)
///     .expect("Out of memory");
/// outer_map.insert(WrappedNumber(2), map_2)
///     .expect("Out of memory");
///
/// // insert some values into nested maps
/// // with_key() commits changes automatically
/// outer_map
///     .with_key(&1, |val| {
///         val
///             .unwrap()
///             .insert_and_commit(
///                 WrappedNumber(11),
///                 WrappedNumber(11),
///             )
///             .expect("Out of memory");
///     });
///
/// outer_map
///     .with_key(&2, |val| {
///         val.unwrap()
///         .insert_and_commit(
///             WrappedNumber(22),
///             WrappedNumber(22),
///         )
///         .expect("Out of memory");
///     });
///
/// // create a witness for some key in a nested map using `witness_with()`
/// let witness = outer_map.witness_with(&2, |map_2| {
///     map_2.witness(&22)
/// });
///
/// assert_eq!(witness.reconstruct(), outer_map.root_hash());
/// ```
pub struct SCertifiedBTreeMap<
    K: StableType + AsFixedSizeBytes + Ord + AsHashableBytes,
    V: StableType + AsFixedSizeBytes + AsHashTree,
> {
    pub(crate) inner: SBTreeMap<K, V>,
    modified: LeveledList,
    uncommited: bool,
}

impl<
        K: StableType + AsFixedSizeBytes + Ord + AsHashableBytes,
        V: StableType + AsFixedSizeBytes + AsHashTree,
    > SCertifiedBTreeMap<K, V>
{
    /// Creates a new [SCertifiedBTreeMap]
    ///
    /// Allocates a small amount of heap memory.
    #[inline]
    pub fn new() -> Self {
        Self {
            inner: SBTreeMap::new_certified(),
            modified: LeveledList::new(),
            uncommited: false,
        }
    }

    /// Inserts a new key-value pair into this [SCertifiedBTreeMap], leaving it in the `uncommited`
    /// state, if the insertion was successful
    ///
    /// * See also [SCertifiedBTreeMap::commit]
    /// * See also [SCertifiedBTreeMap::insert_and_commit]
    /// * See also [SBTreeMap::insert]
    #[inline]
    pub fn insert(&mut self, key: K, value: V) -> Result<Option<V>, (K, V)> {
        let res = self.inner._insert(key, value, &mut self.modified);

        if res.is_ok() && !self.uncommited {
            self.uncommited = true;
        }

        res
    }

    /// Inserts a new key-value pair into this [SCertifiedBTreeMap], immediately commiting changes to
    /// the underlying Merkle tree, if the insertion was successful
    ///
    /// See also [SCertifiedBTreeMap::insert]
    #[inline]
    pub fn insert_and_commit(&mut self, key: K, value: V) -> Result<Option<V>, (K, V)> {
        let it = self.insert(key, value)?;
        self.commit();

        Ok(it)
    }

    /// Removes a key-value pair from this [SCertifiedBTreeMap], leaving it in the `uncommited` state,
    /// if the removal was successful
    ///
    /// * See also [SCertifiedBTreeMap::commit]
    /// * See also [SCertifiedBTreeMap::remove_and_commit]
    /// * See also [SBTreeMap::remove]
    #[inline]
    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        if !self.uncommited {
            self.uncommited = true;
        }

        self.inner._remove(key, &mut self.modified)
    }

    /// Removes a key-value pair from this [SCertifiedBTreeMap], immediately commiting changes to
    /// the underlying Merkle tree, if the removal was successful
    ///
    /// * See also [SCertifiedBTreeMap::remove]
    #[inline]
    pub fn remove_and_commit<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        let it = self.remove(key);
        self.commit();

        it
    }

    /// Removes all key-value pairs from this map, swapping the underlying Merkle with a fresh one
    /// and leaving it in the `commited` state
    #[inline]
    pub fn clear(&mut self) {
        self.uncommited = false;
        self.modified = LeveledList::new();

        self.inner.clear();
    }

    /// See [SBTreeMap::get]
    #[inline]
    pub fn get<Q>(&self, key: &Q) -> Option<SRef<'_, V>>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.inner.get(key)
    }

    /// See [SBTreeMap::get]
    #[inline]
    pub fn get_random_key(&self, seed: u32) -> Option<SRef<K>> {
        self.inner.get_random_key(seed)
    }

    /// Allows mutation of the value stored by the provided key, accepting a lambda to perform it
    ///
    /// This method recomputes the underlying Merkle tree, if the key-value pair is found
    #[inline]
    pub fn with_key<Q, R, F: FnOnce(Option<SRefMut<V>>) -> R>(&mut self, key: &Q, f: F) -> R
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        let val = self.inner._get_mut(key, &mut self.modified);

        if val.is_some() && !self.uncommited {
            self.uncommited = true;
        }

        let res = f(val);

        self.commit();

        res
    }

    /// See [SBTreeMap::contains_key]
    #[inline]
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.inner.contains_key(key)
    }

    /// See [SBTreeMap::len]
    #[inline]
    pub fn len(&self) -> u64 {
        self.inner.len()
    }

    /// See [SBTreeMap::is_empty]
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// See [SBTreeMap::iter]
    #[inline]
    pub fn iter(&self) -> SBTreeMapIter<'_, K, V> {
        self.inner.iter()
    }

    /// Commits all `uncommited` changes to this data structure, recalculating the underlying Merkle
    /// tree
    ///
    /// Merkle tree recomputation is a very expensive operation. But you can save a lot of cycles,
    /// if you're able to commit changes in batches.
    ///
    /// While [SCertifiedBTreeMap] is in the `uncommited` state, every call that touches the underlying
    /// Merkle tree will panic ([SCertifiedBTreeMap::prove_absence], [SCertifiedBTreeMap::witness_with],
    /// [SCertifiedBTreeMap::prove_range], [SCertifiedBTreeMap::as_hash_tree]).
    pub fn commit(&mut self) {
        if !self.uncommited {
            return;
        }
        self.uncommited = false;

        while let Some(ptr) = self.modified.pop() {
            let mut node = BTreeNode::<K, V>::from_ptr(ptr);
            match &mut node {
                BTreeNode::Internal(n) => n.commit::<V>(),
                BTreeNode::Leaf(n) => n.commit(),
            };
        }
    }

    /// Constructs a Merkle proof that is enough to be sure that the requested key **is not** present
    /// in this [SCertifiedBTreeMap]
    ///
    /// This proof is simply a proof that two keys around the requested one (remember, this is a BTree,
    /// keys are arranged in the ascending order) don't have anything in between them.
    ///
    /// Borrowed type is also accepted. If your key type is, for example, [SBox] of [String],
    /// then you can get the value by [String].
    ///
    /// # Panics
    /// Panics if this map is the `uncommited` state.
    /// Panics if the key is actually present in this map.
    pub fn prove_absence<Q>(&self, index: &Q) -> HashTree
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        assert!(!self.uncommited);

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

    /// Constructs a Merkle proof that includes all keys of the requested range
    ///
    /// Borrowed type is also accepted. If your key type is, for example, [SBox] of [String],
    /// then you can get the value by [String].
    ///
    /// # Panics
    /// Panics if this map is the `uncommited` state.
    pub fn prove_range<Q>(&self, from: &Q, to: &Q) -> HashTree
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        assert!(!self.uncommited);
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

    /// Proves that the key-value pair is present in this [SCertifiedBTreeMap], revealing the value itself
    ///
    /// This method accepts a lambda, so it is possible to witness nested [SCertifiedBTreeMap]s.
    ///
    /// Borrowed type is also accepted. If your key type is, for example, [SBox] of [String],
    /// then you can get the value by [String].
    ///
    /// # Panics
    /// Panics if this map is the `uncommited` state.
    pub fn witness_with<Q, Fn: FnMut(&V) -> HashTree>(&self, index: &Q, f: Fn) -> HashTree
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        assert!(!self.uncommited);

        let root_opt = self.inner.get_root();
        if root_opt.is_none() {
            return HashTree::Empty;
        }

        let node = unsafe { root_opt.unwrap_unchecked() };
        witness_node(&node, index, f)
    }

    /// Same as [SCertifiedBTreeMap::witness_with], but uses [AsHashTree::hash_tree] as lambda
    ///
    /// Use to witness non-nested maps
    ///
    /// Borrowed type is also accepted. If your key type is, for example, [SBox] of [String],
    /// then you can get the value by [String].
    #[inline]
    pub fn witness<Q>(&self, index: &Q) -> HashTree
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.witness_with(index, |value| value.hash_tree())
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

    /// Returns the entire Merkle tree of this [SCertifiedBTreeMap], without revealing values
    ///
    /// # Important
    /// This method can make your canister easily reach cycles message limit and is present entirely
    /// because of compatibility with [Dfinity's RBTree](https://github.com/dfinity/cdk-rs/tree/main/library/ic-certified-map).
    /// Only use it with small enough trees.
    ///
    /// # Panics
    /// Panics if this map is the `uncommited` state.
    fn hash_tree(&self) -> HashTree {
        assert!(!self.uncommited);

        let e1 = self.inner.iter().next();
        let e2 = self.inner.iter().rev().next();

        match (e1, e2) {
            (None, None) => HashTree::Empty,
            (Some((k1, _)), Some((k2, _))) => self.prove_range(k1.deref(), k2.deref()),
            _ => unreachable!(),
        }
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
    Q: Ord + ?Sized,
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
        assert!(!self.uncommited);

        self.inner.as_fixed_size_bytes(buf)
    }

    fn from_fixed_size_bytes(buf: &[u8]) -> Self {
        let mut inner = SBTreeMap::<K, V>::from_fixed_size_bytes(buf);
        inner.set_certified(true);

        Self {
            inner,
            modified: LeveledList::new(),
            uncommited: false,
        }
    }
}

impl<
        K: StableType + AsFixedSizeBytes + Ord + AsHashableBytes,
        V: StableType + AsFixedSizeBytes + AsHashTree,
    > StableType for SCertifiedBTreeMap<K, V>
{
    #[inline]
    unsafe fn stable_drop_flag_on(&mut self) {
        self.inner.stable_drop_flag_on();
    }

    #[inline]
    unsafe fn stable_drop_flag_off(&mut self) {
        self.inner.stable_drop_flag_off();
    }
}

impl<
        K: StableType + AsFixedSizeBytes + Ord + AsHashableBytes + Debug,
        V: StableType + AsFixedSizeBytes + AsHashTree + Debug,
    > Debug for SCertifiedBTreeMap<K, V>
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("{")?;
        for (idx, (k, v)) in self.iter().enumerate() {
            k.fmt(f)?;
            f.write_str(": ")?;
            v.fmt(f)?;

            if idx < (self.len() - 1) as usize {
                f.write_str(", ")?;
            }
        }
        f.write_str("}")
    }
}

impl<
        K: StableType + AsFixedSizeBytes + Ord + AsHashableBytes,
        V: StableType + AsFixedSizeBytes + AsHashTree,
    > LeafBTreeNode<K, V>
{
    pub(crate) fn commit(&mut self) {
        let len = self.read_len();

        let mut hash = HashForker::default();

        for i in 0..len {
            let k = self.get_key(i);
            let v = self.get_value(i);

            hash.fork_with(labeled_hash(&k.as_hashable_bytes(), &v.root_hash()));
        }

        self.write_root_hash(&hash.finish(), true);
    }

    #[inline]
    pub(crate) fn root_hash(&self) -> Hash {
        self.read_root_hash(true)
    }

    pub(crate) fn prove_absence(&self, index: usize, len: usize) -> Result<HashTree, HashTree> {
        let mut witness = WitnessForker::default();

        let from = index as isize - 1;
        let to = index;

        for i in 0..len {
            let k = self.get_key(i);
            let v = self.get_value(i);

            // it is safe to cast from to usize, since i can never reach 2**31
            let rh = if i == from as usize || i == to {
                labeled(k.as_hashable_bytes(), pruned(v.root_hash()))
            } else {
                pruned(labeled_hash(&k.as_hashable_bytes(), &v.root_hash()))
            };

            witness.fork_with(rh);
        }

        if to == len && len != 0 {
            Err(witness.finish())
        } else {
            Ok(witness.finish())
        }
    }

    pub(crate) fn prove_range<Q>(&self, from: &Q, to: &Q) -> HashTree
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        let len = self.read_len();

        if len == 0 {
            return HashTree::Empty;
        }

        let from_idx = match self.binary_search(from, len) {
            Ok(idx) => idx,
            Err(idx) => idx,
        };

        let to_idx = match self.binary_search(to, len) {
            Ok(idx) => idx,
            Err(idx) => idx,
        };

        let mut witness = WitnessForker::default();

        for i in 0..from_idx {
            let k = self.get_key(i);
            let v = self.get_value(i);

            witness.fork_with(pruned(labeled_hash(&k.as_hashable_bytes(), &v.root_hash())));
        }

        for i in from_idx..(to_idx + 1).min(len) {
            let k = self.get_key(i);
            let v = self.get_value(i);

            witness.fork_with(labeled(k.as_hashable_bytes(), pruned(v.root_hash())));
        }

        for i in (to_idx + 1)..len {
            let k = self.get_key(i);
            let v = self.get_value(i);

            witness.fork_with(pruned(labeled_hash(&k.as_hashable_bytes(), &v.root_hash())));
        }

        witness.finish()
    }

    pub(crate) fn witness_with<Q, Fn: FnMut(&V) -> HashTree>(
        &self,
        index: &Q,
        mut f: Fn,
    ) -> HashTree
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        let len = self.read_len();

        assert!(len > 0, "The key is NOT present!");

        let index = match self.binary_search(index, len) {
            Ok(idx) => idx,
            Err(_) => panic!("The key is NOT present!"),
        };

        let mut witness = WitnessForker::default();

        for i in 0..len {
            let k = self.get_key(i);
            let v = self.get_value(i);

            let rh = if i == index {
                labeled(k.as_hashable_bytes(), f(&v))
            } else {
                pruned(labeled_hash(&k.as_hashable_bytes(), &v.root_hash()))
            };

            witness.fork_with(rh);
        }

        witness.finish()
    }
}

impl<K: StableType + AsFixedSizeBytes + Ord + AsHashableBytes> InternalBTreeNode<K> {
    pub(crate) fn commit<V: StableType + AsFixedSizeBytes + AsHashTree>(&mut self) {
        let len = self.read_len();
        let mut hash = HashForker::default();

        for i in 0..(len + 1) {
            hash.fork_with(self.read_child_root_hash::<V>(i, true));
        }

        self.write_root_hash(&hash.finish(), true);
    }

    #[inline]
    pub(crate) fn root_hash(&self) -> Hash {
        self.read_root_hash(true)
    }

    pub(crate) fn prove_absence<V: StableType + AsFixedSizeBytes + AsHashTree, Q>(
        &self,
        key: &Q,
    ) -> Result<HashTree, HashTree>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        let len = self.read_len();

        debug_assert!(len > 0);

        let index = match self.binary_search(key, len) {
            Ok(_) => panic!("The key is present!"),
            Err(idx) => idx,
        };

        let mut witness = WitnessForker::default();

        let mut i = 0;
        loop {
            if i == len + 1 {
                break;
            }

            let mut ptr = u64::from_fixed_size_bytes(&self.read_child_ptr_buf(i));
            let mut child = BTreeNode::<K, V>::from_ptr(ptr);

            let result = if i == index {
                match child {
                    BTreeNode::Internal(n) => n.prove_absence::<V, Q>(key),
                    BTreeNode::Leaf(n) => {
                        let len = n.read_len();
                        let idx = match n.binary_search(key, len) {
                            Ok(_) => panic!("The key is present!"),
                            Err(idx) => idx,
                        };

                        n.prove_absence(idx, len)
                    }
                }
            } else {
                match child {
                    BTreeNode::Internal(n) => Ok(HashTree::Pruned(n.read_root_hash(true))),
                    BTreeNode::Leaf(n) => Ok(HashTree::Pruned(n.read_root_hash(true))),
                }
            };

            match result {
                Ok(h) => {
                    witness.fork_with(h);

                    i += 1;
                }
                Err(h) => {
                    witness.fork_with(h);

                    if i == len {
                        return Err(witness.finish());
                    }

                    // simply take from the next one
                    ptr = u64::from_fixed_size_bytes(&self.read_child_ptr_buf(i + 1));
                    child = BTreeNode::<K, V>::from_ptr(ptr);

                    let rh = match child {
                        BTreeNode::Internal(n) => n.prove_absence::<V, Q>(key),
                        BTreeNode::Leaf(n) => {
                            let len = n.read_len();
                            n.prove_absence(0, len)
                        }
                    }
                    .unwrap();

                    witness.fork_with(rh);

                    i += 2;
                }
            }
        }

        Ok(witness.finish())
    }

    pub(crate) fn prove_range<V: AsHashTree + StableType + AsFixedSizeBytes, Q>(
        &self,
        from: &Q,
        to: &Q,
    ) -> HashTree
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        let len = self.read_len();

        debug_assert!(len > 0);

        let from_idx = match self.binary_search(from, len) {
            Ok(idx) => idx,
            Err(idx) => idx,
        };

        let to_idx = match self.binary_search(to, len) {
            Ok(idx) => idx + 1,
            Err(idx) => idx,
        };

        let mut witness = WitnessForker::default();

        for i in 0..from_idx {
            witness.fork_with(pruned(self.read_child_root_hash::<V>(i, true)));
        }

        for i in from_idx..(to_idx + 1).min(len + 1) {
            let ptr = u64::from_fixed_size_bytes(&self.read_child_ptr_buf(i));
            let child = BTreeNode::<K, V>::from_ptr(ptr);

            let rh = match child {
                BTreeNode::Internal(n) => n.prove_range::<V, Q>(from, to),
                BTreeNode::Leaf(n) => n.prove_range(from, to),
            };

            witness.fork_with(rh);
        }

        for i in (to_idx + 1)..(len + 1) {
            witness.fork_with(pruned(self.read_child_root_hash::<V>(i, true)));
        }

        witness.finish()
    }

    pub(crate) fn witness_with_replacement<V: StableType + AsFixedSizeBytes + AsHashTree>(
        &self,
        index: usize,
        replace: HashTree,
        len: usize,
    ) -> HashTree {
        debug_assert!(len > 0);

        let mut witness = WitnessForker::default();

        for i in 0..index {
            witness.fork_with(pruned(self.read_child_root_hash::<V>(i, true)));
        }

        witness.fork_with(replace);

        for i in (index + 1)..(len + 1) {
            witness.fork_with(pruned(self.read_child_root_hash::<V>(i, true)));
        }

        witness.finish()
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::certified_btree_map::SCertifiedBTreeMap;
    use crate::utils::certification::{
        leaf, leaf_hash, merge_hash_trees, traverse_hashtree, AsHashTree, AsHashableBytes, Hash,
        HashTree,
    };
    use crate::utils::test::generate_random_string;
    use crate::{
        _debug_validate_allocator, get_allocated_size, init_allocator, retrieve_custom_data,
        stable, stable_memory_init, stable_memory_post_upgrade, stable_memory_pre_upgrade,
        store_custom_data, SBox,
    };
    use ic_certified_map::RbTree;
    use rand::rngs::ThreadRng;
    use rand::seq::SliceRandom;
    use rand::{thread_rng, Rng};
    use std::borrow::Cow;

    impl AsHashTree for u64 {
        fn root_hash(&self) -> Hash {
            leaf_hash(&self.to_le_bytes())
        }

        fn hash_tree(&self) -> HashTree {
            leaf(self.to_le_bytes().to_vec())
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

        {
            let iterations = 1000;
            let mut map = SCertifiedBTreeMap::<u64, u64>::default();

            let mut example = Vec::new();
            for i in 0..iterations {
                example.push(i as u64);
            }
            example.shuffle(&mut thread_rng());

            for i in 0..iterations {
                assert!(map.insert(example[i], example[i]).unwrap().is_none());
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

            assert_eq!(map.insert(0, 1).unwrap().unwrap(), 0);
            assert_eq!(map.insert(0, 0).unwrap().unwrap(), 1);

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

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
    }

    #[test]
    fn random_in_batches_works_fine() {
        stable::clear();
        stable_memory_init();

        {
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

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
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

        {
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

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
    }

    #[test]
    fn merge_works_fine() {
        stable::clear();
        stable_memory_init();

        #[derive(Debug, Default, Ord, PartialOrd, Eq, PartialEq, Copy, Clone)]
        struct U64(pub u64);

        impl ic_certified_map::AsHashTree for U64 {
            fn as_hash_tree(&self) -> ic_certified_map::HashTree<'_> {
                ic_certified_map::HashTree::Leaf(Cow::Owned(self.0.to_le_bytes().to_vec()))
            }

            fn root_hash(&self) -> ic_certified_map::Hash {
                ic_certified_map::leaf_hash(&self.0.to_le_bytes())
            }
        }

        {
            let mut map = SCertifiedBTreeMap::<SBox<u64>, SBox<u64>>::default();
            let mut rb = RbTree::<[u8; 8], U64>::new();

            for i in 0..100 {
                map.insert(SBox::new(i * 2).unwrap(), SBox::new(i * 2).unwrap())
                    .unwrap();

                rb.insert((i * 2).to_le_bytes(), U64(i * 2));
            }

            map.commit();

            let map_w1 = map.prove_absence(&11);
            let map_w2 = map.witness_with(&22, |val| leaf(val.as_hashable_bytes()));

            assert_eq!(map_w1.reconstruct(), map.root_hash());
            assert_eq!(map_w2.reconstruct(), map.root_hash());

            let w3 = merge_hash_trees(map_w1, map_w2);
            assert_eq!(w3.reconstruct(), map.root_hash());

            let rb_w1 = rb.witness(&11u64.to_le_bytes());
            let rb_w2 = rb.witness(&22u64.to_le_bytes());

            let rb_w3 = rb.key_range(&9u64.to_le_bytes(), &100u64.to_le_bytes());
            let map_w3 = map.prove_range(&9, &100);
        }

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
    }

    #[test]
    fn range_proofs_work_fine() {
        stable::clear();
        stable_memory_init();

        {
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

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
    }

    #[test]
    fn nested_maps_work_fine() {
        stable::clear();
        stable_memory_init();

        {
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

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
    }

    impl AsHashTree for String {
        fn root_hash(&self) -> Hash {
            leaf_hash(&self.as_hashable_bytes())
        }

        fn hash_tree(&self) -> HashTree {
            leaf(self.as_hashable_bytes())
        }
    }

    impl AsHashableBytes for String {
        fn as_hashable_bytes(&self) -> Vec<u8> {
            self.as_bytes().to_vec()
        }
    }

    #[derive(Debug)]
    enum Action {
        Insert,
        Batch,
        Remove,
        Clear,
        CanisterUpgrade,
    }

    struct Fuzzer {
        map: Option<SCertifiedBTreeMap<SBox<String>, SBox<String>>>,
        keys: Vec<String>,
        rng: ThreadRng,
        log: Vec<Action>,
    }

    impl Fuzzer {
        fn new() -> Fuzzer {
            Fuzzer {
                map: Some(SCertifiedBTreeMap::new()),
                keys: Vec::new(),
                rng: thread_rng(),
                log: Vec::new(),
            }
        }

        fn map(&mut self) -> &mut SCertifiedBTreeMap<SBox<String>, SBox<String>> {
            self.map.as_mut().unwrap()
        }

        fn next(&mut self) {
            let action = self.rng.gen_range(0..120);

            match action {
                // INSERT ~60%
                0..=59 => {
                    let key = generate_random_string(&mut self.rng);
                    let value = generate_random_string(&mut self.rng);

                    if let Ok(key_data) = SBox::new(key.clone()) {
                        if let Ok(val_data) = SBox::new(value.clone()) {
                            if self.map().insert_and_commit(key_data, val_data).is_err() {
                                return;
                            }

                            match self.keys.binary_search(&key) {
                                Ok(idx) => {}
                                Err(idx) => {
                                    self.keys.insert(idx, key);
                                }
                            };

                            self.log.push(Action::Insert);
                        }
                    }
                }
                // REMOVE
                60..=89 => {
                    let len = self.map().len();

                    if len == 0 {
                        return self.next();
                    }

                    let idx: u64 = self.rng.gen_range(0..len);
                    let key = self.keys.remove(idx as usize);

                    self.map().remove_and_commit(&key).unwrap();

                    self.log.push(Action::Remove);
                }
                // CANISTER UPGRADE
                90..=99 => match SBox::new(self.map.take().unwrap()) {
                    Ok(data) => {
                        store_custom_data(1, data);

                        if stable_memory_pre_upgrade().is_ok() {
                            stable_memory_post_upgrade();
                        }

                        self.map = retrieve_custom_data::<
                            SCertifiedBTreeMap<SBox<String>, SBox<String>>,
                        >(1)
                        .map(|it| it.into_inner());

                        self.log.push(Action::CanisterUpgrade);
                    }
                    Err(map) => {
                        self.map = Some(map);
                    }
                },
                100..=101 => {
                    self.map().clear();
                    self.keys.clear();

                    self.log.push(Action::Clear);
                }
                // BATCH
                _ => {
                    let count = self.rng.gen_range(0..10);

                    for i in 0..count {
                        let act = self.rng.gen_range(0..10);
                        match act {
                            0..=7 => {
                                let key = generate_random_string(&mut self.rng);
                                let value = generate_random_string(&mut self.rng);

                                if let Ok(key_data) = SBox::new(key.clone()) {
                                    if let Ok(val_data) = SBox::new(value.clone()) {
                                        if self.map().insert(key_data, val_data).is_err() {
                                            continue;
                                        }

                                        match self.keys.binary_search(&key) {
                                            Ok(idx) => {}
                                            Err(idx) => {
                                                self.keys.insert(idx, key);
                                            }
                                        };
                                    }
                                }
                            }
                            _ => {
                                let len = self.map().len();

                                if len == 0 {
                                    continue;
                                }

                                let idx: u64 = self.rng.gen_range(0..len);
                                let key = self.keys.remove(idx as usize);

                                self.map().remove(&key).unwrap();
                            }
                        }
                    }

                    self.map().commit();
                    self.log.push(Action::Batch);
                }
            }

            _debug_validate_allocator();

            let root_hash = self.map().root_hash();

            for key in self.keys.clone() {
                let witness = self
                    .map()
                    .witness_with(&key, |it| leaf(it.as_hashable_bytes()));

                assert_eq!(witness.reconstruct(), root_hash);
            }

            for _ in 0..10 {
                let k = generate_random_string(&mut self.rng);
                let witness = self.map().prove_absence(&k);

                assert_eq!(witness.reconstruct(), root_hash);

                let k1 = generate_random_string(&mut self.rng);

                let (max, min) = if k > k1 { (k, k1) } else { (k1, k) };

                let witness = self.map().prove_range(&min, &max);
                assert_eq!(witness.reconstruct(), root_hash);
            }
        }
    }

    #[test]
    fn fuzzer_works_fine() {
        stable::clear();
        init_allocator(0);

        {
            let mut fuzzer = Fuzzer::new();

            for _ in 0..500 {
                fuzzer.next();
            }
        }

        assert_eq!(get_allocated_size(), 0);
    }

    #[test]
    fn fuzzer_works_fine_limited_memory() {
        stable::clear();
        init_allocator(1);

        {
            let mut fuzzer = Fuzzer::new();

            for _ in 0..1000 {
                fuzzer.next();
            }
        }

        assert_eq!(get_allocated_size(), 0);
    }
}
