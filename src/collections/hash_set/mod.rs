use crate::collections::hash_map::SHashMap;
use copy_as_bytes::traits::{AsBytes, SuperSized};
use speedy::{Context, LittleEndian, Readable, Reader, Writable, Writer};
use std::hash::Hash;
use crate::collections::hash_set::iter::SHashSetIter;

pub mod iter;

pub struct SHashSet<T> {
    map: SHashMap<T, ()>,
}

impl<T> SHashSet<T> {
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

impl<T: AsBytes + Hash + Eq> SHashSet<T>
where
    [u8; T::SIZE]: Sized,
{
    pub fn insert(&mut self, value: T) -> bool {
        self.map.insert(value, ()).is_some()
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
    
    pub fn iter(&self) -> SHashSetIter<T> {
        SHashSetIter::new(self)
    }
}

impl<T> Default for SHashSet<T> {
    fn default() -> Self {
        SHashSet::new()
    }
}

impl<'a, T> Readable<'a, LittleEndian> for SHashSet<T> {
    fn read_from<R: Reader<'a, LittleEndian>>(
        reader: &mut R,
    ) -> Result<Self, <speedy::LittleEndian as Context>::Error> {
        let map = SHashMap::read_from(reader)?;

        Ok(Self { map })
    }
}

impl<T> Writable<LittleEndian> for SHashSet<T> {
    fn write_to<W: ?Sized + Writer<LittleEndian>>(
        &self,
        writer: &mut W,
    ) -> Result<(), <speedy::LittleEndian as Context>::Error> {
        self.map.write_to(writer)
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

        let set = SHashSet::<u64>::new_with_capacity(10);
        unsafe { set.drop() };
    }
}
