use crate::collections::btree_map::internal_node::InternalBTreeNode;
use crate::collections::btree_map::leaf_node::LeafBTreeNode;
use crate::collections::btree_map::BTreeNode;
use crate::primitive::StableAllocated;
use serde::{ser::SerializeSeq, Serialize, Serializer};
use serde_bytes::Bytes;
use sha2::{Digest, Sha256};

pub type Hash = [u8; 32];
pub const EMPTY_HASH: Hash = [0u8; 32];

/// Compatible with https://sdk.dfinity.org/docs/interface-spec/index.html#_certificate
#[derive(Debug)]
pub enum HashTree {
    Empty,
    Fork(Box<(HashTree, HashTree)>),
    Labeled(Vec<u8>, Box<HashTree>),
    Leaf(Vec<u8>),
    Pruned(Hash),
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

impl HashTree {
    pub fn reconstruct(&self) -> Hash {
        match self {
            Self::Empty => domain_sep("ic-hashtree-empty").finalize().into(),
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

fn domain_sep(s: &str) -> sha2::Sha256 {
    let buf: [u8; 1] = [s.len() as u8];
    let mut h = Sha256::new();
    h.update(&buf[..]);
    h.update(s.as_bytes());
    h
}

pub trait AsHashableBytes {
    fn as_hashable_bytes(&self) -> Vec<u8>;
    fn from_hashable_bytes(bytes: Vec<u8>) -> Self;
}

impl AsHashableBytes for Hash {
    fn as_hashable_bytes(&self) -> Vec<u8> {
        self.to_vec()
    }

    fn from_hashable_bytes(bytes: Vec<u8>) -> Self {
        bytes.try_into().unwrap()
    }
}

pub trait AsHashTree<I = ()> {
    /// Returns the root hash of the tree without constructing it.
    /// Must be equivalent to `HashTree::reconstruct()`.
    fn root_hash(&self) -> Hash;

    /// Creates a HashTree witnessing the value indexed by index of type I.
    fn witness(&self, index: I, indexed_subtree: Option<HashTree>) -> HashTree;
}

impl<K: StableAllocated + Ord + AsHashableBytes, V: StableAllocated + AsHashableBytes>
    AsHashTree<usize> for LeafBTreeNode<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    fn root_hash(&self) -> Hash {
        let len = self.read_len();

        let mut k = K::from_fixed_size_bytes(&self.read_key(0));
        let mut v = V::from_fixed_size_bytes(&self.read_value(0));

        let mut lh = labeled_hash(&k.as_hashable_bytes(), &leaf_hash(&v.as_hashable_bytes()));

        for i in 1..len {
            k = K::from_fixed_size_bytes(&self.read_key(i));
            v = V::from_fixed_size_bytes(&self.read_value(i));

            lh = fork_hash(
                &lh,
                &labeled_hash(&k.as_hashable_bytes(), &leaf_hash(&v.as_hashable_bytes())),
            );
        }

        lh
    }

    fn witness(&self, index: usize, indexed_subtree: Option<HashTree>) -> HashTree {
        let len = self.read_len();
        debug_assert!(index < len);

        let mut k = K::from_fixed_size_bytes(&self.read_key(0));
        let mut v = V::from_fixed_size_bytes(&self.read_value(0));

        if index == 0 {
            let mut lh = if let Some(is) = indexed_subtree {
                labeled(k.as_hashable_bytes(), is)
            } else {
                labeled(k.as_hashable_bytes(), leaf(v.as_hashable_bytes()))
            };

            for i in 1..len {
                k = K::from_fixed_size_bytes(&self.read_key(i));
                v = V::from_fixed_size_bytes(&self.read_value(i));

                lh = fork(
                    lh,
                    pruned(labeled_hash(
                        &k.as_hashable_bytes(),
                        &leaf_hash(&v.as_hashable_bytes()),
                    )),
                );
            }

            lh
        } else {
            let mut lh = pruned(labeled_hash(
                &k.as_hashable_bytes(),
                &leaf_hash(&v.as_hashable_bytes()),
            ));

            for i in 1..index {
                k = K::from_fixed_size_bytes(&self.read_key(i));
                v = V::from_fixed_size_bytes(&self.read_value(i));

                lh = fork(
                    lh,
                    pruned(labeled_hash(
                        &k.as_hashable_bytes(),
                        &leaf_hash(&v.as_hashable_bytes()),
                    )),
                );
            }

            lh = if let Some(is) = indexed_subtree {
                k = K::from_fixed_size_bytes(&self.read_key(index));

                fork(lh, labeled(k.as_hashable_bytes(), is))
            } else {
                k = K::from_fixed_size_bytes(&self.read_key(index));
                v = V::from_fixed_size_bytes(&self.read_value(index));

                fork(
                    lh,
                    labeled(k.as_hashable_bytes(), leaf(v.as_hashable_bytes())),
                )
            };

            for i in (index + 1)..len {
                k = K::from_fixed_size_bytes(&self.read_key(i));
                v = V::from_fixed_size_bytes(&self.read_value(i));

                lh = fork(
                    lh,
                    pruned(labeled_hash(
                        &k.as_hashable_bytes(),
                        &leaf_hash(&v.as_hashable_bytes()),
                    )),
                );
            }

            lh
        }
    }
}

impl<K: StableAllocated + Ord + AsHashableBytes> AsHashTree<usize> for InternalBTreeNode<K>
where
    [(); K::SIZE]: Sized,
{
    fn root_hash(&self) -> Hash {
        let len = self.read_len() + 1;
        let mut lh = self.read_child_hash(0, true);

        for i in 1..len {
            lh = fork_hash(&lh, &self.read_child_hash(i, true));
        }

        lh
    }

    fn witness(&self, index: usize, indexed_subtree: Option<HashTree>) -> HashTree {
        debug_assert!(indexed_subtree.is_some());

        let len = self.read_len() + 1;
        if index == 0 {
            let mut lh = unsafe { indexed_subtree.unwrap_unchecked() };

            for i in 1..len {
                lh = fork(lh, pruned(self.read_child_hash(i, true)));
            }

            lh
        } else {
            let mut lh = pruned(self.read_child_hash(0, true));

            for i in 1..index {
                lh = fork(lh, pruned(self.read_child_hash(i, true)));
            }

            lh = fork(lh, unsafe { indexed_subtree.unwrap_unchecked() });

            for i in (index + 1)..len {
                lh = fork(lh, pruned(self.read_child_hash(i, true)));
            }

            lh
        }
    }
}

impl<K: StableAllocated + Ord + AsHashableBytes, V: StableAllocated + AsHashableBytes>
    BTreeNode<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    pub(crate) fn root_hash(&self) -> Hash {
        match &self {
            Self::Internal(i) => i.root_hash(),
            Self::Leaf(l) => l.root_hash(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::utils::certification::{
        domain_sep, empty, fork, fork_hash, labeled, labeled_hash, leaf, leaf_hash, pruned, Hash,
        EMPTY_HASH,
    };
    use serde_test::{assert_ser_tokens, assert_tokens, Token};
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
