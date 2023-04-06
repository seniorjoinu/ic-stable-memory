use crate::collections::certified_btree_map::SCertifiedBTreeMap;
use crate::collections::certified_btree_set::iter::SCertifiedBTreeSetIter;
use crate::encoding::AsFixedSizeBytes;
use crate::primitive::s_ref::SRef;
use crate::primitive::StableType;
use crate::utils::certification::HashTree;
use crate::{AsHashTree, AsHashableBytes};
use std::borrow::Borrow;
use std::fmt::{Debug, Formatter};

pub mod iter;

/// Certified B-plus tree based set data structure
///
/// This is just a wrapper around [SCertifiedBTreeMap]`<T, ()>`, read its documentation for more info on the internals.
/// () is encoded as `empty` [utils::certification::HashTree].
pub struct SCertifiedBTreeSet<T: StableType + AsFixedSizeBytes + Ord + AsHashableBytes> {
    map: SCertifiedBTreeMap<T, ()>,
}

impl<T: Ord + StableType + AsFixedSizeBytes + AsHashableBytes> SCertifiedBTreeSet<T> {
    /// See [SCertifiedBTreeMap::new]
    #[inline]
    pub fn new() -> Self {
        Self {
            map: SCertifiedBTreeMap::new(),
        }
    }

    /// See [SCertifiedBTreeMap::len]
    #[inline]
    pub fn len(&self) -> u64 {
        self.map.len()
    }

    /// See [SCertifiedBTreeMap::is_empty]
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// See [SCertifiedBTreeMap::insert]
    #[inline]
    pub fn insert(&mut self, value: T) -> Result<bool, T> {
        self.map
            .insert(value, ())
            .map(|it| it.is_some())
            .map_err(|(k, _)| k)
    }

    /// See [SCertifiedBTreeMap::insert_and_commit]
    #[inline]
    pub fn insert_and_commit(&mut self, value: T) -> Result<bool, T> {
        self.map
            .insert_and_commit(value, ())
            .map(|it| it.is_some())
            .map_err(|(k, _)| k)
    }

    /// See [SCertifiedBTreeMap::remove]
    #[inline]
    pub fn remove<Q>(&mut self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.map.remove(value).is_some()
    }

    /// See [SCertifiedBTreeMap::remove_and_commit]
    #[inline]
    pub fn remove_and_commit<Q>(&mut self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.map.remove_and_commit(value).is_some()
    }

    /// See [SCertifiedBTreeMap::commit]
    #[inline]
    pub fn commit(&mut self) {
        self.map.commit();
    }

    /// See [SCertifiedBTreeMap::prove_absence]
    #[inline]
    pub fn prove_absence<Q>(&self, index: &Q) -> HashTree
    where
        T: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.map.prove_absence(index)
    }

    /// See [SCertifiedBTreeMap::prove_range]
    #[inline]
    pub fn prove_range<Q>(&self, from: &Q, to: &Q) -> HashTree
    where
        T: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.map.prove_range(from, to)
    }

    /// See [SCertifiedBTreeMap::witness]
    #[inline]
    pub fn witness<Q>(&self, index: &Q) -> HashTree
    where
        T: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.map.witness(index)
    }

    /// See [SCertifiedBTreeMap::clear]
    #[inline]
    pub fn clear(&mut self) {
        self.map.clear();
    }

    /// See [SCertifiedBTreeMap::contains_key]
    #[inline]
    pub fn contains<Q>(&self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.map.contains_key(value)
    }

    /// See [SBTreeMap::get]
    #[inline]
    pub fn get_random(&self, seed: u32) -> Option<SRef<T>> {
        self.map.get_random_key(seed)
    }

    /// See [SCertifiedBTreeMap::iter]
    #[inline]
    pub fn iter(&self) -> SCertifiedBTreeSetIter<T> {
        SCertifiedBTreeSetIter::new(self)
    }
}

impl<T: StableType + AsFixedSizeBytes + Ord + AsHashableBytes> AsHashTree
    for SCertifiedBTreeSet<T>
{
    #[inline]
    fn root_hash(&self) -> crate::utils::certification::Hash {
        self.map.root_hash()
    }

    #[inline]
    fn hash_tree(&self) -> HashTree {
        self.map.hash_tree()
    }
}

impl<T: Ord + StableType + AsFixedSizeBytes + AsHashableBytes> Default for SCertifiedBTreeSet<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: StableType + AsFixedSizeBytes + Ord + AsHashableBytes> AsFixedSizeBytes
    for SCertifiedBTreeSet<T>
{
    const SIZE: usize = SCertifiedBTreeMap::<T, ()>::SIZE;
    type Buf = <SCertifiedBTreeMap<T, ()> as AsFixedSizeBytes>::Buf;

    #[inline]
    fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
        self.map.as_fixed_size_bytes(buf);
    }

    #[inline]
    fn from_fixed_size_bytes(arr: &[u8]) -> Self {
        let map = SCertifiedBTreeMap::<T, ()>::from_fixed_size_bytes(&arr);
        Self { map }
    }
}

impl<T: StableType + AsFixedSizeBytes + Ord + AsHashableBytes> StableType
    for SCertifiedBTreeSet<T>
{
    #[inline]
    unsafe fn stable_drop_flag_on(&mut self) {
        self.map.stable_drop_flag_on();
    }

    #[inline]
    unsafe fn stable_drop_flag_off(&mut self) {
        self.map.stable_drop_flag_off()
    }
}

impl<T: StableType + AsFixedSizeBytes + Ord + Debug + AsHashableBytes> Debug
    for SCertifiedBTreeSet<T>
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("(")?;
        for (idx, elem) in self.iter().enumerate() {
            elem.fmt(f)?;

            if idx < (self.len() - 1) as usize {
                f.write_str(", ")?;
            }
        }
        f.write_str(")")
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::certified_btree_set::SCertifiedBTreeSet;
    use crate::encoding::{AsFixedSizeBytes, Buffer};
    use crate::{_debug_validate_allocator, get_allocated_size, stable, stable_memory_init};

    #[test]
    fn serialization_works_fine() {
        stable::clear();
        stable_memory_init();

        {
            let set = SCertifiedBTreeSet::<usize>::new();

            let buf = set.as_new_fixed_size_bytes();
            SCertifiedBTreeSet::<usize>::from_fixed_size_bytes(buf._deref());
        }

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
    }

    #[test]
    fn iter_works_fine() {
        stable::clear();
        stable_memory_init();

        {
            let mut set = SCertifiedBTreeSet::<usize>::default();
            for i in 0..100 {
                set.insert(i);
            }

            for (idx, mut i) in set.iter().enumerate() {
                assert_eq!(idx, *i);
            }
        }

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
    }
}
