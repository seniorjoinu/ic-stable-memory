use crate::collections::certified_hash_map::node::SCertifiedHashMapNode;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fmt::Debug;

pub type Sha256Digest = [u8; 32];
pub const EMPTY_SHA256: Sha256Digest = [0u8; 32];

#[derive(Debug)]
pub enum MerkleHash {
    Inline(Sha256Digest),
    Pruned(Sha256Digest),
    None,
}

#[derive(Debug)]
pub enum MerkleChild {
    Hole(MerkleNode),
    Pruned(Sha256Digest),
    None,
}

#[derive(Debug)]
pub struct MerkleNode {
    pub entry_hash: MerkleHash,
    pub left_child: MerkleChild,
    pub right_child: MerkleChild,
    pub additional_left_child: MerkleChild,
    pub additional_right_child: MerkleChild,
}

// TODO: think about making a tip into a normal merkle tree

impl MerkleNode {
    pub fn new(entry_hash: MerkleHash, left_child: MerkleChild, right_child: MerkleChild) -> Self {
        Self {
            entry_hash,
            left_child,
            right_child,
            additional_left_child: MerkleChild::None,
            additional_right_child: MerkleChild::None,
        }
    }

    fn reconstruct(
        &self,
        inlined_hashes: &mut HashSet<Sha256Digest>,
        hasher: &mut Sha256,
    ) -> Option<Sha256Digest> {
        let entry_hash = match &self.entry_hash {
            MerkleHash::None => EMPTY_SHA256,
            MerkleHash::Pruned(h) => *h,
            MerkleHash::Inline(h) => {
                if !inlined_hashes.remove(h) {
                    return None;
                }

                *h
            }
        };

        let left_child_hash = match self.left_child {
            MerkleChild::None => EMPTY_SHA256,
            MerkleChild::Pruned(h) => *h,
            MerkleChild::Hole(n) => n.reconstruct(inlined_hashes, hasher)?,
        };

        let right_child_hash = match self.right_child {
            MerkleChild::None => EMPTY_SHA256,
            MerkleChild::Pruned(h) => *h,
            MerkleChild::Hole(n) => n.reconstruct(inlined_hashes, hasher)?,
        };

        hasher.update(self.entry_hash);
        hasher.update(left_child_hash);
        hasher.update(right_child_hash);

        hasher.finalize_reset().into()
    }
}

#[derive(Debug)]
pub struct MerkleWitness {
    pub tree: MerkleNode,
    pub inlined_hashes: HashSet<Sha256Digest>,
}

#[derive(Debug, Copy, Clone)]
pub enum ReconstructionError {
    CopiesOrUnknownEntriesInTree,
    UnkownEntriesInlined,
}

impl MerkleWitness {
    pub fn new<I>(tree: MerkleNode, inlined_hashes: I) -> Self
    where
        I: IntoIterator<Item = Sha256Digest>,
    {
        Self {
            tree,
            inlined_hashes: inlined_hashes.into_iter().collect(),
        }
    }

    pub fn reconstruct(self) -> Result<(HashSet<Sha256Digest>, Sha256Digest), ReconstructionError> {
        let mut hasher = Sha256::default();
        let mut i_hashes = self.inlined_hashes.clone();

        let root_hash = self
            .tree
            .reconstruct(&mut i_hashes, &mut hasher)
            .ok_or(ReconstructionError::CopiesOrUnknownEntriesInTree)?;

        // no extra hashes should present
        if !i_hashes.is_empty() {
            return Err(ReconstructionError::UnkownEntriesInlined);
        }

        Ok((self.inlined_hashes, root_hash))
    }
}
