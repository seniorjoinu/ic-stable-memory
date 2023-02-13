use crate::collections::btree_map::SBTreeMap;
use crate::collections::btree_set::iter::SBTreeSetIter;
use crate::encoding::AsFixedSizeBytes;
use crate::primitive::StableType;
use crate::OutOfMemory;
use std::borrow::Borrow;
use std::fmt::{Debug, Formatter};

pub mod iter;

pub struct SBTreeSet<T: StableType + AsFixedSizeBytes + Ord> {
    map: SBTreeMap<T, ()>,
}

impl<T: Ord + StableType + AsFixedSizeBytes> SBTreeSet<T> {
    #[inline]
    pub fn new() -> Self {
        Self {
            map: SBTreeMap::new(),
        }
    }

    #[inline]
    pub fn len(&self) -> u64 {
        self.map.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    #[inline]
    pub fn insert(&mut self, value: T) -> Result<bool, T> {
        self.map
            .insert(value, ())
            .map(|it| it.is_some())
            .map_err(|(k, _)| k)
    }

    #[inline]
    pub fn remove<Q>(&mut self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: Ord,
    {
        self.map.remove(value).is_some()
    }

    #[inline]
    pub fn contains<Q>(&self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: Ord,
    {
        self.map.contains_key(value)
    }

    #[inline]
    pub fn iter(&self) -> SBTreeSetIter<T> {
        SBTreeSetIter::new(self)
    }
}

impl<T: StableType + AsFixedSizeBytes + Ord> SBTreeSet<T> {
    #[inline]
    pub fn clear(&mut self) {
        self.map.clear();
    }
}

impl<T: Ord + StableType + AsFixedSizeBytes> Default for SBTreeSet<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: StableType + AsFixedSizeBytes + Ord> AsFixedSizeBytes for SBTreeSet<T> {
    const SIZE: usize = SBTreeMap::<T, ()>::SIZE;
    type Buf = <SBTreeMap<T, ()> as AsFixedSizeBytes>::Buf;

    #[inline]
    fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
        self.map.as_fixed_size_bytes(buf);
    }

    #[inline]
    fn from_fixed_size_bytes(arr: &[u8]) -> Self {
        let map = SBTreeMap::<T, ()>::from_fixed_size_bytes(&arr);
        Self { map }
    }
}

impl<T: StableType + AsFixedSizeBytes + Ord> StableType for SBTreeSet<T> {
    #[inline]
    unsafe fn assume_not_owned_by_stable_memory(&mut self) {
        self.map.assume_not_owned_by_stable_memory();
    }

    #[inline]
    unsafe fn assume_owned_by_stable_memory(&mut self) {
        self.map.assume_owned_by_stable_memory()
    }
}

impl<T: StableType + AsFixedSizeBytes + Ord + Debug> Debug for SBTreeSet<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("(")?;
        for (idx, elem) in self.iter().enumerate() {
            elem.fmt(f)?;

            if idx < (self.len() - 1) as usize {
                f.write_str(", ")?;
            }
        }
        f.write_str(")")
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::btree_set::SBTreeSet;
    use crate::encoding::{AsFixedSizeBytes, Buffer};
    use crate::utils::test::generate_random_string;
    use crate::{
        _debug_validate_allocator, get_allocated_size, init_allocator, retrieve_custom_data,
        stable, stable_memory_init, stable_memory_post_upgrade, stable_memory_pre_upgrade,
        store_custom_data, SBox,
    };
    use rand::rngs::ThreadRng;
    use rand::{thread_rng, Rng};
    use std::collections::BTreeSet;

    #[test]
    fn it_works_fine() {
        stable::clear();
        stable_memory_init();

        {
            let mut set = SBTreeSet::default();
            set.insert(10);
            set.insert(20);

            assert!(set.contains(&10));
            assert_eq!(set.len(), 2);
            assert!(!set.is_empty());

            assert!(set.remove(&10));
            assert!(!set.remove(&10));

            let set = SBTreeSet::<u64>::new();
        }

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
    }

    #[test]
    fn serialization_works_fine() {
        stable::clear();
        stable_memory_init();

        {
            let set = SBTreeSet::<u32>::new();

            let buf = set.as_new_fixed_size_bytes();
            SBTreeSet::<u32>::from_fixed_size_bytes(buf._deref());
        }

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
    }

    #[test]
    fn iter_works_fine() {
        stable::clear();
        stable_memory_init();

        {
            let mut set = SBTreeSet::<u32>::default();
            for i in 0..100 {
                set.insert(i);
            }

            for (idx, mut i) in set.iter().enumerate() {
                assert_eq!(idx as u32, *i);
            }
        }

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
    }

    #[derive(Debug)]
    enum Action {
        Insert,
        Remove,
        CanisterUpgrade,
    }

    struct Fuzzer {
        set: Option<SBTreeSet<SBox<String>>>,
        example: BTreeSet<String>,
        keys: Vec<String>,
        rng: ThreadRng,
        log: Vec<Action>,
    }

    impl Fuzzer {
        fn new() -> Fuzzer {
            Fuzzer {
                set: Some(SBTreeSet::new()),
                example: BTreeSet::new(),
                keys: Vec::new(),
                rng: thread_rng(),
                log: Vec::new(),
            }
        }

        fn set(&mut self) -> &mut SBTreeSet<SBox<String>> {
            self.set.as_mut().unwrap()
        }

        fn next(&mut self) {
            let action = self.rng.gen_range(0..100);

            match action {
                // INSERT ~60%
                0..=59 => {
                    let key = generate_random_string(&mut self.rng);
                    if let Ok(data) = SBox::new(key.clone()) {
                        if self.set().insert(data).is_err() {
                            return;
                        }
                        self.example.insert(key.clone());

                        match self.keys.binary_search(&key) {
                            Ok(idx) => {}
                            Err(idx) => {
                                self.keys.insert(idx, key);
                            }
                        };

                        self.log.push(Action::Insert);
                    }
                }
                // REMOVE
                60..=89 => {
                    let len = self.set().len();

                    if len == 0 {
                        return self.next();
                    }

                    let idx: u64 = self.rng.gen_range(0..len);
                    let key = self.keys.remove(idx as usize);

                    self.set().remove(&key);
                    self.example.remove(&key);

                    self.log.push(Action::Remove);
                }
                // CANISTER UPGRADE
                _ => match SBox::new(self.set.take().unwrap()) {
                    Ok(data) => {
                        store_custom_data(1, data);

                        if stable_memory_pre_upgrade().is_ok() {
                            stable_memory_post_upgrade();
                        }

                        self.set = retrieve_custom_data::<SBTreeSet<SBox<String>>>(1)
                            .map(|it| it.into_inner());

                        self.log.push(Action::CanisterUpgrade);
                    }
                    Err(set) => {
                        self.set = Some(set);
                    }
                },
            }

            _debug_validate_allocator();
            assert_eq!(self.set().len() as usize, self.example.len());

            for key in self.keys.clone() {
                assert!(self.set().contains(&key));
                assert!(self.example.contains(&key));
            }
        }
    }

    #[test]
    fn fuzzer_works_fine() {
        stable::clear();
        init_allocator(0);

        {
            let mut fuzzer = Fuzzer::new();

            for _ in 0..10_000 {
                fuzzer.next();
            }
        }

        assert_eq!(get_allocated_size(), 0);
    }

    #[test]
    fn fuzzer_works_fine_limited_memory() {
        stable::clear();
        init_allocator(10);

        {
            let mut fuzzer = Fuzzer::new();

            for _ in 0..10_000 {
                fuzzer.next();
            }
        }

        assert_eq!(get_allocated_size(), 0);
    }
}
