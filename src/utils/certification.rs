use crate::collections::btree_map::internal_node::InternalBTreeNode;
use crate::collections::btree_map::leaf_node::LeafBTreeNode;
use crate::collections::btree_map::BTreeNode;
use crate::encoding::AsFixedSizeBytes;
use crate::primitive::StableType;
use serde::{ser::SerializeSeq, Serialize, Serializer};
use serde_bytes::Bytes;
use sha2::{Digest, Sha256};
use std::borrow::Borrow;
use std::mem;

pub type Hash = [u8; 32];
pub const EMPTY_HASH: Hash = [0u8; 32];

/// Compatible with https://sdk.dfinity.org/docs/interface-spec/index.html#_certificate
#[derive(Debug, Clone)]
pub enum HashTree {
    Empty,
    Fork(Box<(HashTree, HashTree)>),
    Labeled(Vec<u8>, Box<HashTree>),
    Leaf(Vec<u8>),
    Pruned(Hash),
}

pub fn traverse_hashtree<Fn: FnMut(&HashTree)>(tree: &HashTree, f: &mut Fn) {
    f(tree);

    match tree {
        HashTree::Empty => {}
        HashTree::Pruned(_) => {}
        HashTree::Fork(x) => {
            let a = &x.0;
            let b = &x.1;

            traverse_hashtree(a, f);
            traverse_hashtree(b, f);
        }
        HashTree::Labeled(_, x) => {
            traverse_hashtree(x, f);
        }
        HashTree::Leaf(_) => {}
    }
}

pub fn empty() -> HashTree {
    HashTree::Empty
}

pub fn fork(l: HashTree, r: HashTree) -> HashTree {
    HashTree::Fork(Box::new((l, r)))
}

pub fn labeled(l: Vec<u8>, t: HashTree) -> HashTree {
    HashTree::Labeled(l, Box::new(t))
}

pub fn leaf(val: Vec<u8>) -> HashTree {
    HashTree::Leaf(val)
}

pub fn pruned(h: Hash) -> HashTree {
    HashTree::Pruned(h)
}

pub struct WitnessForker(HashTree);

impl Default for WitnessForker {
    fn default() -> Self {
        Self(HashTree::Empty)
    }
}

impl WitnessForker {
    #[inline]
    pub fn fork_with(&mut self, rh: HashTree) {
        match &mut self.0 {
            HashTree::Empty => {
                self.0 = rh;
            }
            it => {
                let lh = mem::replace(it, HashTree::Empty);
                *it = fork(lh, rh);
            }
        }
    }

    #[inline]
    pub fn finish(self) -> HashTree {
        self.0
    }
}

pub struct HashForker(Hash);

impl Default for HashForker {
    fn default() -> Self {
        Self(EMPTY_HASH)
    }
}

impl HashForker {
    #[inline]
    pub fn fork_with(&mut self, rh: Hash) {
        if self.0 == EMPTY_HASH {
            self.0 = rh;
        } else {
            self.0 = fork_hash(&self.0, &rh);
        }
    }

    #[inline]
    pub fn finish(self) -> Hash {
        if self.0 == EMPTY_HASH {
            empty_hash()
        } else {
            self.0
        }
    }
}

pub fn fork_hash(l: &Hash, r: &Hash) -> Hash {
    let mut h = domain_sep("ic-hashtree-fork");
    h.update(&l[..]);
    h.update(&r[..]);
    h.finalize().into()
}

pub fn leaf_hash(data: &[u8]) -> Hash {
    let mut h = domain_sep("ic-hashtree-leaf");
    h.update(data);
    h.finalize().into()
}

pub fn labeled_hash(label: &[u8], content_hash: &Hash) -> Hash {
    let mut h = domain_sep("ic-hashtree-labeled");
    h.update(label);
    h.update(&content_hash[..]);
    h.finalize().into()
}

pub fn empty_hash() -> Hash {
    domain_sep("ic-hashtree-empty").finalize().into()
}

impl HashTree {
    pub fn reconstruct(&self) -> Hash {
        match self {
            Self::Empty => empty_hash(),
            Self::Fork(f) => fork_hash(&f.0.reconstruct(), &f.1.reconstruct()),
            Self::Labeled(l, t) => {
                let thash = t.reconstruct();
                labeled_hash(l, &thash)
            }
            Self::Leaf(data) => leaf_hash(data),
            Self::Pruned(h) => *h,
        }
    }
}

impl Serialize for HashTree {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        match self {
            HashTree::Empty => {
                let mut seq = serializer.serialize_seq(Some(1))?;
                seq.serialize_element(&0u8)?;
                seq.end()
            }
            HashTree::Fork(p) => {
                let mut seq = serializer.serialize_seq(Some(3))?;
                seq.serialize_element(&1u8)?;
                seq.serialize_element(&p.0)?;
                seq.serialize_element(&p.1)?;
                seq.end()
            }
            HashTree::Labeled(label, tree) => {
                let mut seq = serializer.serialize_seq(Some(3))?;
                seq.serialize_element(&2u8)?;
                seq.serialize_element(Bytes::new(label))?;
                seq.serialize_element(&tree)?;
                seq.end()
            }
            HashTree::Leaf(leaf_bytes) => {
                let mut seq = serializer.serialize_seq(Some(2))?;
                seq.serialize_element(&3u8)?;
                seq.serialize_element(Bytes::new(leaf_bytes.as_ref()))?;
                seq.end()
            }
            HashTree::Pruned(digest) => {
                let mut seq = serializer.serialize_seq(Some(2))?;
                seq.serialize_element(&4u8)?;
                seq.serialize_element(Bytes::new(&digest[..]))?;
                seq.end()
            }
        }
    }
}

fn domain_sep(s: &str) -> Sha256 {
    let buf: [u8; 1] = [s.len() as u8];
    let mut h = Sha256::new();
    h.update(&buf[..]);
    h.update(s.as_bytes());

    h
}

pub trait AsHashableBytes {
    fn as_hashable_bytes(&self) -> Vec<u8>;
}

impl AsHashableBytes for Hash {
    #[inline]
    fn as_hashable_bytes(&self) -> Vec<u8> {
        self.to_vec()
    }
}

impl AsHashableBytes for () {
    #[inline]
    fn as_hashable_bytes(&self) -> Vec<u8> {
        Vec::new()
    }
}

pub trait AsHashTree {
    /// Returns the root hash of the tree without constructing it.
    /// Must be equivalent to `HashTree::reconstruct()`.
    fn root_hash(&self) -> Hash;
}

impl AsHashTree for Hash {
    #[inline]
    fn root_hash(&self) -> Hash {
        *self
    }
}

impl AsHashTree for () {
    #[inline]
    fn root_hash(&self) -> Hash {
        empty_hash()
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
        Q: Ord,
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
        Q: Ord,
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
        Q: Ord,
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
        Q: Ord,
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
    use crate::utils::certification::{
        domain_sep, empty, fork, fork_hash, labeled, labeled_hash, leaf, leaf_hash, pruned, Hash,
        EMPTY_HASH,
    };
    use serde_test::{assert_ser_tokens, Token};
    use sha2::Digest;

    #[test]
    fn test() {
        let k1 = 1u64;
        let v1 = 10u64;

        let k2 = 2u64;
        let v2 = 20u64;

        let wit = fork(
            pruned(labeled_hash(
                &k1.to_le_bytes(),
                &leaf_hash(&v1.to_le_bytes()),
            )),
            labeled(k2.to_le_bytes().to_vec(), leaf(v2.to_le_bytes().to_vec())),
        );

        let root_hash = fork_hash(
            &labeled_hash(&k1.to_le_bytes(), &leaf_hash(&v1.to_le_bytes())),
            &labeled_hash(&k2.to_le_bytes(), &leaf_hash(&v2.to_le_bytes())),
        );

        assert_eq!(wit.reconstruct(), root_hash);

        let wit = fork(
            labeled(k1.to_le_bytes().to_vec(), leaf(v1.to_le_bytes().to_vec())),
            pruned(labeled_hash(
                &k2.to_le_bytes(),
                &leaf_hash(&v2.to_le_bytes()),
            )),
        );

        assert_eq!(wit.reconstruct(), root_hash);
    }

    #[test]
    fn works_fine() {
        let e: Hash = domain_sep("ic-hashtree-empty").finalize().into();
        assert_eq!(empty().reconstruct(), e);
    }

    const c: [u8; 10] = [0u8; 10];

    #[test]
    fn ser_works_fine() {
        let w1 = empty();
        let w2 = fork(empty(), empty());
        let w3 = labeled(vec![0u8; 10], empty());
        let w4 = leaf(vec![0u8; 10]);
        let w5 = pruned(EMPTY_HASH);

        assert_ser_tokens(
            &w1,
            &[Token::Seq { len: Some(1) }, Token::U8(0), Token::SeqEnd],
        );

        assert_ser_tokens(
            &w2,
            &[
                Token::Seq { len: Some(3) },
                Token::U8(1),
                Token::Seq { len: Some(1) },
                Token::U8(0),
                Token::SeqEnd,
                Token::Seq { len: Some(1) },
                Token::U8(0),
                Token::SeqEnd,
                Token::SeqEnd,
            ],
        );

        assert_ser_tokens(
            &w3,
            &[
                Token::Seq { len: Some(3) },
                Token::U8(2),
                Token::Bytes(&c),
                Token::Seq { len: Some(1) },
                Token::U8(0),
                Token::SeqEnd,
                Token::SeqEnd,
            ],
        );

        assert_ser_tokens(
            &w4,
            &[
                Token::Seq { len: Some(2) },
                Token::U8(3),
                Token::Bytes(&c),
                Token::SeqEnd,
            ],
        );

        assert_ser_tokens(
            &w5,
            &[
                Token::Seq { len: Some(2) },
                Token::U8(4),
                Token::Bytes(&EMPTY_HASH),
                Token::SeqEnd,
            ],
        );
    }
}
