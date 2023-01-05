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

pub trait AsHashTree<I = ()> {
    /// Returns the root hash of the tree without constructing it.
    /// Must be equivalent to `HashTree::reconstruct()`.
    fn root_hash(&self) -> Hash;

    /// Creates a HashTree witnessing the value indexed by index of type I.
    fn witness(&self, index: I, indexed_subtree: Option<HashTree>) -> HashTree;
}

#[cfg(test)]
mod tests {
    use crate::utils::certification::{
        fork, fork_hash, labeled, labeled_hash, leaf, leaf_hash, pruned,
    };

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
}
