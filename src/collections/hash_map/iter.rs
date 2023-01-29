use crate::collections::hash_map::{HashMapKey, SHashMap};
use crate::primitive::StableAllocated;
use crate::primitive::s_ref::SRef;
use std::hash::Hash;
use crate::primitive::s_ref_mut::SRefMut;

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
    type Item = (SRef<'a, K>, SRef<'a, V>);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.i == self.map.capacity() {
                break None;
            }
            
            match self.map.read_key_at(self.i, false) {
                HashMapKey::Empty => {
                    self.i += 1;
                    continue;
                }
                HashMapKey::OccupiedNull => {
                    let key = SRef::new(self.map.get_key_ptr(self.i));
                    let val = SRef::new(self.map.get_value_ptr(self.i));
                    
                    self.i += 1;

                    break Some((key, val));
                }
                _ => unreachable!(),
            }
        }
    }
}

pub struct SHashMapIterMut<'a, K, V> {
    map: &'a mut SHashMap<K, V>,
    i: usize,
}

impl<'a, K, V> SHashMapIterMut<'a, K, V> {
    pub fn new(map: &'a mut SHashMap<K, V>) -> Self {
        Self { map, i: 0 }
    }
}

impl<'a, K: StableAllocated + Eq + Hash, V: StableAllocated> Iterator for SHashMapIterMut<'a, K, V>
    where
        [(); K::SIZE]: Sized,
        [(); V::SIZE]: Sized,
{
    type Item = (SRefMut<'a, K>, SRefMut<'a, V>);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.i == self.map.capacity() {
                break None;
            }

            match self.map.read_key_at(self.i, false) {
                HashMapKey::Empty => {
                    self.i += 1;
                    continue;
                }
                HashMapKey::OccupiedNull => {
                    let key = SRefMut::new(self.map.get_key_ptr(self.i));
                    let val = SRefMut::new(self.map.get_value_ptr(self.i));

                    self.i += 1;

                    break Some((key, val));
                }
                _ => unreachable!(),
            }
        }
    }
}

pub struct SHashMapIterCopy<'a, K, V> {
    map: &'a SHashMap<K, V>,
    i: usize,
}

impl<'a, K, V> SHashMapIterCopy<'a, K, V> {
    pub fn new(map: &'a SHashMap<K, V>) -> Self {
        Self { map, i: 0 }
    }
}

impl<'a, K: StableAllocated + Eq + Hash, V: StableAllocated> Iterator for SHashMapIterCopy<'a, K, V>
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
