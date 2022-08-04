use crate::collections::hash_map::SHashMap;
use speedy::{LittleEndian, Readable, Writable};
use std::hash::Hash;

#[derive(Readable, Writable)]
pub struct SHashSet<T> {
    map: SHashMap<T, ()>,
}

impl<'a, T: Readable<'a, LittleEndian> + Writable<LittleEndian> + Hash + Eq> SHashSet<T> {
    pub fn new() -> Self {
        Self {
            map: SHashMap::new(),
        }
    }

    pub fn new_with_capacity(capacity: u32) -> Self {
        Self {
            map: SHashMap::new_with_capacity(capacity),
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

impl<'a, T: Readable<'a, LittleEndian> + Writable<LittleEndian> + Hash + Eq> Default
    for SHashSet<T>
{
    fn default() -> Self {
        SHashSet::new()
    }
}
