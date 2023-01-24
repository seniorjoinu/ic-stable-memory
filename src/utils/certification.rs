use crate::collections::btree_map::internal_node::InternalBTreeNode;
use crate::collections::btree_map::leaf_node::LeafBTreeNode;
use crate::collections::btree_map::{BTreeNode, IBTreeNode};
use crate::primitive::StableAllocated;
use crate::utils::encoding::AsFixedSizeBytes;
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

const FORK_DOMAIN: &str = "ic-hashtree-fork";
static mut FORK_DOMAIN_SEP: Option<Sha256> = None;

const LEAF_DOMAIN: &str = "ic-hashtree-leaf";
static mut LEAF_DOMAIN_SEP: Option<Sha256> = None;

const LABELED_DOMAIN: &str = "ic-hashtree-labeled";
static mut LABELED_DOMAIN_SEP: Option<Sha256> = None;

const EMPTY_DOMAIN: &str = "ic-hashtree-empty";
static mut EMPTY_DOMAIN_SEP: Option<Sha256> = None;

fn domain_sep(s: &str) -> Sha256 {
    let sep = match s {
        FORK_DOMAIN => unsafe { &mut FORK_DOMAIN_SEP },
        LEAF_DOMAIN => unsafe { &mut LEAF_DOMAIN_SEP },
        LABELED_DOMAIN => unsafe { &mut LABELED_DOMAIN_SEP },
        EMPTY_DOMAIN => unsafe { &mut EMPTY_DOMAIN_SEP },
        _ => unreachable!(),
    };

    if let Some(s) = sep {
        s.clone()
    } else {
        let buf: [u8; 1] = [s.len() as u8];
        let mut h = Sha256::new();
        h.update(&buf[..]);
        h.update(s.as_bytes());

        *sep = Some(h.clone());

        h
    }
}

pub trait AsHashableBytes {
    fn as_hashable_bytes(&self) -> Vec<u8>;
}

impl AsHashableBytes for Hash {
    fn as_hashable_bytes(&self) -> Vec<u8> {
        self.to_vec()
    }
}

pub trait AsHashTree<T, I = ()> {
    /// Returns the root hash of the tree without constructing it.
    /// Must be equivalent to `HashTree::reconstruct()`.
    fn root_hash(&self) -> Hash;

    fn witness_with<Fn: FnMut(&T) -> HashTree>(&self, index: I, f: Fn) -> HashTree;

    fn commit(&mut self);

    // Creates a HashTree witnessing all keys [from .. to] (values are pruned)
    // If [from_opt] is None, it is considered as "from minimum stored key"
    // If [to_opt] is None, it is considered as "to maximum stored key"
    // Both [from] and [to] are clamped, if out of bounds
    // If [from] is bigger than [to] or equal, panics
    // fn range_witness(&self, from_opt: Option<I>, to_opt: Option<I>) -> HashTree;
}

impl<K: StableAllocated + Ord + AsHashableBytes, V: StableAllocated + AsHashableBytes>
    AsHashTree<V, &K> for LeafBTreeNode<K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    fn commit(&mut self) {
        let len = self.read_len();

        if len == 0 {
            self.write_root_hash(&EMPTY_HASH, true);
            return;
        }

        let (mut k, mut v) = self.read_entry(0);

        let mut lh = labeled_hash(&k.as_hashable_bytes(), &leaf_hash(&v.as_hashable_bytes()));

        for i in 1..len {
            (k, v) = self.read_entry(i);

            lh = fork_hash(
                &lh,
                &labeled_hash(&k.as_hashable_bytes(), &leaf_hash(&v.as_hashable_bytes())),
            );
        }

        self.write_root_hash(&lh, true);
    }

    #[inline]
    fn root_hash(&self) -> Hash {
        self.read_root_hash(true)
    }

    fn witness_with<Fn: FnMut(&V) -> HashTree>(&self, index: &K, mut f: Fn) -> HashTree {
        let len = self.read_len();
        if len == 0 {
            return HashTree::Empty;
        }

        let index = match self.binary_search(index, len) {
            Ok(idx) => idx,
            Err(_) => return HashTree::Empty,
        };

        let (mut k, mut v) = self.read_entry(0);

        let mut lh = if index == 0 {
            labeled(k.as_hashable_bytes(), f(&v))
        } else {
            let mut lh = pruned(labeled_hash(&k.as_hashable_bytes(), &f(&v).reconstruct()));

            for i in 1..index {
                (k, v) = self.read_entry(i);
                lh = fork(
                    lh,
                    pruned(labeled_hash(&k.as_hashable_bytes(), &f(&v).reconstruct())),
                );
            }

            (k, v) = self.read_entry(index);
            lh = fork(lh, labeled(k.as_hashable_bytes(), f(&v)));

            lh
        };

        for i in (index + 1)..len {
            (k, v) = self.read_entry(i);
            lh = fork(
                lh,
                pruned(labeled_hash(&k.as_hashable_bytes(), &f(&v).reconstruct())),
            );
        }

        lh
    }
}

impl<K: StableAllocated + Ord + AsHashableBytes> InternalBTreeNode<K>
where
    [(); K::SIZE]: Sized,
{
    pub(crate) fn commit<V: StableAllocated + AsHashableBytes>(&mut self)
    where
        [(); V::SIZE]: Sized,
    {
        let len = self.read_len() + 1;
        let mut lh = self.read_child_root_hash::<V>(0, true);

        for i in 1..len {
            lh = fork_hash(&lh, &self.read_child_root_hash::<V>(i, true));
        }

        self.write_root_hash(&lh, true);
    }

    #[inline]
    pub(crate) fn root_hash(&self) -> Hash {
        self.read_root_hash(true)
    }

    pub(crate) fn witness_with_replacement<V: StableAllocated + AsHashableBytes>(
        &self,
        index: usize,
        replace: HashTree,
        len: usize,
    ) -> HashTree
    where
        [(); V::SIZE]: Sized,
    {
        let mut lh = if index == 0 {
            replace
        } else {
            let mut lh = pruned(self.read_child_root_hash::<V>(0, true));

            for i in 1..index {
                lh = fork(lh, pruned(self.read_child_root_hash::<V>(i, true)));
            }

            lh = fork(lh, replace);

            lh
        };

        for i in (index + 1)..(len + 1) {
            lh = fork(lh, pruned(self.read_child_root_hash::<V>(i, true)));
        }

        lh
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
