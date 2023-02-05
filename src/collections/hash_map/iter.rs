use crate::collections::hash_map::{SHashMap};
use crate::encoding::AsFixedSizeBytes;
use crate::primitive::s_ref::SRef;
use crate::primitive::StableType;
use std::hash::Hash;

pub struct SHashMapIter<
    'a,
    K: StableType + AsFixedSizeBytes + Hash + Eq,
    V: StableType + AsFixedSizeBytes,
> {
    map: &'a SHashMap<K, V>,
    i: usize,
}

impl<'a, K: StableType + AsFixedSizeBytes + Hash + Eq, V: StableType + AsFixedSizeBytes>
    SHashMapIter<'a, K, V>
{
    pub fn new(map: &'a SHashMap<K, V>) -> Self {
        Self { map, i: 0 }
    }
}

impl<'a, K: StableType + AsFixedSizeBytes + Eq + Hash, V: StableType + AsFixedSizeBytes> Iterator
    for SHashMapIter<'a, K, V>
{
    type Item = (SRef<'a, K>, SRef<'a, V>);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.i == self.map.capacity() {
                break None;
            }

            if let Some(k) = self.map.get_key(self.i) {
                let v = self.map.get_val(self.i);
                
                self.i += 1;
                
                return Some((k, v));
            }
            
            self.i += 1;
        }
    }
}
