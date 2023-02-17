use crate::collections::hash_map::SHashMap;
use crate::collections::hash_set::iter::SHashSetIter;
use crate::encoding::AsFixedSizeBytes;
use crate::primitive::StableType;
use crate::OutOfMemory;
use std::borrow::Borrow;
use std::fmt::{Debug, Formatter};
use std::hash::Hash;

pub mod iter;

pub struct SHashSet<T: StableType + AsFixedSizeBytes + Hash + Eq> {
    map: SHashMap<T, ()>,
}

impl<T: StableType + AsFixedSizeBytes + Hash + Eq> SHashSet<T> {
    #[inline]
    pub fn new() -> Self {
        Self {
            map: SHashMap::new(),
        }
    }

    #[inline]
    pub fn new_with_capacity(capacity: usize) -> Result<Self, OutOfMemory> {
        Ok(Self {
            map: SHashMap::new_with_capacity(capacity)?,
        })
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
        Q: Hash + Eq,
    {
        self.map.remove(value).is_some()
    }

    #[inline]
    pub fn contains<Q>(&self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.map.contains_key(value)
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.map.capacity()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        self.map.is_full()
    }

    #[inline]
    pub fn iter(&self) -> SHashSetIter<T> {
        SHashSetIter::new(self)
    }

    #[inline]
    pub fn clear(&mut self) {
        self.map.clear();
    }
}

impl<T: StableType + AsFixedSizeBytes + Hash + Eq> Default for SHashSet<T> {
    #[inline]
    fn default() -> Self {
        SHashSet::new()
    }
}

impl<T: StableType + AsFixedSizeBytes + Hash + Eq> AsFixedSizeBytes for SHashSet<T> {
    const SIZE: usize = SHashMap::<T, ()>::SIZE;
    type Buf = <SHashMap<T, ()> as AsFixedSizeBytes>::Buf;

    #[inline]
    fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
        self.map.as_fixed_size_bytes(buf)
    }

    #[inline]
    fn from_fixed_size_bytes(arr: &[u8]) -> Self {
        let map = SHashMap::<T, ()>::from_fixed_size_bytes(arr);
        Self { map }
    }
}

impl<T: StableType + AsFixedSizeBytes + Hash + Eq> StableType for SHashSet<T> {
    #[inline]
    unsafe fn stable_drop_flag_off(&mut self) {
        self.map.stable_drop_flag_off();
    }

    #[inline]
    unsafe fn stable_drop_flag_on(&mut self) {
        self.map.stable_drop_flag_on();
    }
}

impl<T: StableType + AsFixedSizeBytes + Hash + Eq + Debug> Debug for SHashSet<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("(")?;
        for (idx, elem) in self.iter().enumerate() {
            elem.fmt(f)?;

            if idx < self.len() - 1 {
                f.write_str(", ")?;
            }
        }
        f.write_str(")")
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::hash_set::SHashSet;
    use crate::encoding::{AsFixedSizeBytes, Buffer};
    use crate::utils::test::generate_random_string;
    use crate::utils::DebuglessUnwrap;
    use crate::{
        _debug_validate_allocator, get_allocated_size, init_allocator, retrieve_custom_data,
        stable, stable_memory_init, stable_memory_post_upgrade, stable_memory_pre_upgrade,
        store_custom_data, SBox,
    };
    use rand::rngs::ThreadRng;
    use rand::{thread_rng, Rng};
    use std::collections::HashSet;

    #[test]
    fn basic_flow_works_fine() {
        stable::clear();
        stable_memory_init();

        {
            let mut set = SHashSet::default();

            assert!(set.is_empty());

            assert!(!set.insert(10).unwrap());
            assert!(!set.insert(20).unwrap());
            assert!(set.insert(10).unwrap());

            assert!(set.contains(&10));
            assert!(!set.contains(&100));

            assert_eq!(set.len(), 2);

            assert!(!set.remove(&100));
            assert!(set.remove(&10));

            SHashSet::<u64>::new_with_capacity(10).debugless_unwrap();
        }

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
    }

    #[test]
    fn iter_works_fine() {
        stable::clear();
        stable_memory_init();

        {
            let mut set = SHashSet::default();

            for i in 0..100 {
                set.insert(i);
            }

            let mut c = 0;
            for _ in set.iter() {
                c += 1;
            }

            assert_eq!(c, 100);
        }

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
    }

    #[test]
    fn serialization_works_fine() {
        stable::clear();
        stable_memory_init();

        {
            let set = SHashSet::<u32>::default();

            let len = set.len();
            let cap = set.capacity();

            let buf = set.as_new_fixed_size_bytes();
            let set1 = SHashSet::<u32>::from_fixed_size_bytes(buf._deref());

            assert_eq!(len, set1.len());
            assert_eq!(cap, set1.capacity());
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
        set: Option<SHashSet<SBox<String>>>,
        example: HashSet<String>,
        keys: Vec<String>,
        rng: ThreadRng,
        log: Vec<Action>,
    }

    impl Fuzzer {
        fn new() -> Fuzzer {
            Fuzzer {
                set: Some(SHashSet::new()),
                example: HashSet::new(),
                keys: Vec::new(),
                rng: thread_rng(),
                log: Vec::new(),
            }
        }

        fn set(&mut self) -> &mut SHashSet<SBox<String>> {
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

                    let idx = self.rng.gen_range(0..len);
                    let key = self.keys.remove(idx);

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

                        self.set = retrieve_custom_data::<SHashSet<SBox<String>>>(1)
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
