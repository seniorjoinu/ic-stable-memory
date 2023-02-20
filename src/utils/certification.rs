use serde::{ser::SerializeSeq, Serialize, Serializer};
use serde_bytes::Bytes;
use sha2::{Digest, Sha256};
use std::mem;

/// Handy alias to [u8; 32]
pub type Hash = [u8; 32];

/// Constant for zeroed [Hash].
///
/// **Different from [empty()]**
pub const EMPTY_HASH: Hash = [0u8; 32];

/// Same as [Dfinity's HashTree](https://sdk.dfinity.org/docs/interface-spec/index.html#_certificate),
/// but works with owned values, instead of references.
#[derive(Debug, Clone)]
pub enum HashTree {
    #[doc(hidden)]
    Empty,
    #[doc(hidden)]
    Fork(Box<(HashTree, HashTree)>),
    #[doc(hidden)]
    Labeled(Vec<u8>, Box<HashTree>),
    #[doc(hidden)]
    Leaf(Vec<u8>),
    #[doc(hidden)]
    Pruned(Hash),
}

/// Merges two [HashTree]s together. Useful when you need a proof like "has A, but not B".
pub fn merge_hash_trees(lhs: HashTree, rhs: HashTree) -> HashTree {
    use HashTree::{Empty, Fork, Labeled, Leaf, Pruned};

    match (lhs, rhs) {
        (Pruned(l), Pruned(r)) => {
            if l != r {
                panic!("merge_hash_trees: inconsistent hashes");
            }
            Pruned(l)
        }
        (Pruned(_), r) => r,
        (l, Pruned(_)) => l,
        (Fork(l), Fork(r)) => Fork(Box::new((
            merge_hash_trees(l.0, r.0),
            merge_hash_trees(l.1, r.1),
        ))),
        (Labeled(l_label, l), Labeled(r_label, r)) => {
            if l_label != r_label {
                panic!("merge_hash_trees: inconsistent hash tree labels");
            }
            Labeled(l_label, Box::new(merge_hash_trees(*l, *r)))
        }
        (Empty, Empty) => Empty,
        (Leaf(l), Leaf(r)) => {
            if l != r {
                panic!("merge_hash_trees: inconsistent leaves");
            }
            Leaf(l)
        }
        (_l, _r) => {
            panic!("merge_hash_trees: inconsistent tree structure");
        }
    }
}

/// Performs left-to-right tree traversal of a [HashTree], executing a custom lambda on each node.
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

#[doc(hidden)]
pub fn empty() -> HashTree {
    HashTree::Empty
}
#[doc(hidden)]
pub fn fork(l: HashTree, r: HashTree) -> HashTree {
    HashTree::Fork(Box::new((l, r)))
}
#[doc(hidden)]
pub fn labeled(l: Vec<u8>, t: HashTree) -> HashTree {
    HashTree::Labeled(l, Box::new(t))
}
#[doc(hidden)]
pub fn leaf(val: Vec<u8>) -> HashTree {
    HashTree::Leaf(val)
}
#[doc(hidden)]
pub fn pruned(h: Hash) -> HashTree {
    HashTree::Pruned(h)
}

/// Allows prettier forking at a small runtime cost
///
/// See also [HashForker]
///
/// # Example
/// ```rust
/// # use ic_stable_memory::utils::certification::{empty, WitnessForker};
/// # let subtree_1 = empty();
/// # let subtree_2 = empty();
///
/// let mut witness = WitnessForker::default();
/// witness.fork_with(subtree_1);
/// witness.fork_with(subtree_2);
///
/// let hash_tree = witness.finish();
/// ```
pub struct WitnessForker(HashTree);

impl Default for WitnessForker {
    fn default() -> Self {
        Self(HashTree::Empty)
    }
}

impl WitnessForker {
    #[doc(hidden)]
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

    #[doc(hidden)]
    #[inline]
    pub fn finish(self) -> HashTree {
        self.0
    }
}

/// Same as [WitnessForker], but for hashes.
pub struct HashForker(Hash);

impl Default for HashForker {
    fn default() -> Self {
        Self(EMPTY_HASH)
    }
}

impl HashForker {
    #[doc(hidden)]
    #[inline]
    pub fn fork_with(&mut self, rh: Hash) {
        if self.0 == EMPTY_HASH {
            self.0 = rh;
        } else {
            self.0 = fork_hash(&self.0, &rh);
        }
    }

    #[doc(hidden)]
    #[inline]
    pub fn finish(self) -> Hash {
        if self.0 == EMPTY_HASH {
            empty_hash()
        } else {
            self.0
        }
    }
}

#[doc(hidden)]
pub fn fork_hash(l: &Hash, r: &Hash) -> Hash {
    let mut h = domain_sep("ic-hashtree-fork");
    h.update(&l[..]);
    h.update(&r[..]);
    h.finalize().into()
}

#[doc(hidden)]
pub fn leaf_hash(data: &[u8]) -> Hash {
    let mut h = domain_sep("ic-hashtree-leaf");
    h.update(data);
    h.finalize().into()
}

#[doc(hidden)]
pub fn labeled_hash(label: &[u8], content_hash: &Hash) -> Hash {
    let mut h = domain_sep("ic-hashtree-labeled");
    h.update(label);
    h.update(&content_hash[..]);
    h.finalize().into()
}

#[doc(hidden)]
pub fn empty_hash() -> Hash {
    domain_sep("ic-hashtree-empty").finalize().into()
}

impl HashTree {
    /// Recalculates the root hash of this [HashTree]
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

/// Trait that is used to serialize labels of a [HashTree] into bytes.
///
/// See also [SCertifiedBTreeMap](crate::collections::SCertifiedBTreeMap)
pub trait AsHashableBytes {
    #[doc(hidden)]
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

/// Trait that is used to hash a leaf value of a [HashTree].
///
/// This trait should **always** be implemented on user-side.
///
/// See also [SCertifiedBTreeMap](crate::collections::SCertifiedBTreeMap)
pub trait AsHashTree {
    /// Returns the root hash of the tree without constructing it.
    /// Must be equivalent to [HashTree::reconstruct].
    fn root_hash(&self) -> Hash;

    /// Returns a [HashTree] of this value. Must be equivalent to [AsHashTree::root_hash].
    fn hash_tree(&self) -> HashTree;
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
