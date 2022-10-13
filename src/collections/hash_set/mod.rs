use crate::collections::hash_map::SHashMap;
use crate::primitive::StackAllocated;
use speedy::{Readable, Writable};
use std::hash::Hash;

#[derive(Readable, Writable)]
pub struct SHashSet<T, A> {
    map: SHashMap<T, (), A, [u8; 0]>,
}

impl<A, T> SHashSet<T, A> {
    pub fn new() -> Self {
        Self {
            map: SHashMap::new(),
        }
    }

    pub fn new_with_capacity(capacity: usize) -> Self {
        Self {
            map: SHashMap::new_with_capacity(capacity),
        }
    }
}

impl<A: AsMut<[u8]>, T: StackAllocated<T, A> + Hash + Eq> SHashSet<T, A> {
    pub fn insert(&mut self, value: T) -> bool {
        self.map.insert(&value, &()).is_some()
    }

    pub fn remove(&mut self, value: &T) -> bool {
        self.map.remove(value).is_some()
    }

    pub fn contains(&self, value: &T) -> bool {
        self.map.contains_key(value)
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub unsafe fn drop(self) {
        self.map.drop()
    }
}

impl<A, T> Default for SHashSet<T, A> {
    fn default() -> Self {
        SHashSet::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::hash_set::SHashSet;
    use crate::{init_allocator, stable};
    use std::mem::size_of;

    #[test]
    fn basic_flow_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let mut set = SHashSet::default();

        assert!(set.is_empty());

        assert!(!set.insert(10));
        assert!(!set.insert(20));
        assert!(set.insert(10));

        assert!(set.contains(&10));
        assert!(!set.contains(&100));

        assert_eq!(set.len(), 2);

        assert!(!set.remove(&100));
        assert!(set.remove(&10));

        unsafe { set.drop() };

        let set = SHashSet::<u64, [u8; size_of::<u64>()]>::new_with_capacity(10);
        unsafe { set.drop() };
    }
}
