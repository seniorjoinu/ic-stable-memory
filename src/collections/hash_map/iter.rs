use crate::collections::hash_map::{HashMapKey, SHashMap};
use crate::primitive::StableAllocated;
use std::hash::Hash;

pub struct SHashMapIter<'a, K, V> {
    map: &'a SHashMap<K, V>,
    i: usize,
}

impl<'a, K, V> SHashMapIter<'a, K, V> {
    pub fn new(map: &'a SHashMap<K, V>) -> Self {
        Self { map, i: 0 }
    }
}

impl<'a, K: StableAllocated + Eq + Hash, V: StableAllocated> Iterator for SHashMapIter<'a, K, V>
where
    [(); K::SIZE]: Sized,
    [(); V::SIZE]: Sized,
{
    type Item = (K, V);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.i == self.map.capacity() {
                break None;
            }

            match self.map.read_key_at(self.i, true) {
                HashMapKey::Empty => {
                    self.i += 1;
                    continue;
                }
                HashMapKey::Occupied(key) => {
                    let val = self.map.read_val_at(self.i);
                    self.i += 1;

                    break Some((key, val));
                }
                _ => unreachable!(),
            }
        }
    }
}
