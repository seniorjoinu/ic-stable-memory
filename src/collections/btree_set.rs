use crate::collections::btree_map::SBTreeMap;
use speedy::{LittleEndian, Readable, Writable};

#[derive(Readable, Writable)]
pub struct SBTreeSet<T> {
    map: SBTreeMap<T, ()>,
}

impl<'a, T: Readable<'a, LittleEndian> + Writable<LittleEndian> + Ord> SBTreeSet<T> {
    pub fn new() -> Self {
        Self {
            map: SBTreeMap::new(),
        }
    }

    pub fn new_with_degree(degree: usize) -> Self {
        Self {
            map: SBTreeMap::new_with_degree(degree),
        }
    }

    pub fn insert(&mut self, value: T) -> bool {
        self.map.insert(value, &()).is_some()
    }

    pub fn remove(&mut self, value: &T) -> bool {
        self.map.remove(value).is_some()
    }

    pub fn contains(&self, value: &T) -> bool {
        self.map.contains_key(value)
    }

    pub fn len(&self) -> u64 {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn drop(self) {
        self.map.drop()
    }
}

impl<'a, T: Readable<'a, LittleEndian> + Writable<LittleEndian> + Ord> Default for SBTreeSet<T> {
    fn default() -> Self {
        SBTreeSet::new()
    }
}
