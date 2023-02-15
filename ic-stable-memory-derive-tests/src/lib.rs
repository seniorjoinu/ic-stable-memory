#[cfg(test)]
mod derive_tests {
    use candid::{CandidType, Deserialize, Principal};
    use ic_stable_memory::derive::{AsFixedSizeBytes, CandidAsDynSizeBytes, StableType};

    #[derive(StableType, AsFixedSizeBytes, PartialEq, Eq, Debug)]
    struct A1 {
        x: u64,
        y: u32,
        z: usize,
    }

    #[derive(StableType, AsFixedSizeBytes, PartialEq, Eq, Debug)]
    struct A2(u64, u32, usize);

    #[derive(StableType, AsFixedSizeBytes, PartialEq, Eq, Debug)]
    struct A3;

    #[derive(StableType, AsFixedSizeBytes, PartialEq, Eq, Debug)]
    enum B {
        X,
        Y(u32),
        Z { a: u64, b: u16 },
    }

    #[derive(StableType, CandidType, Deserialize, CandidAsDynSizeBytes, PartialEq, Eq, Debug)]
    struct C {
        x: u32,
        y: u32,
        p: Principal,
    }

    #[test]
    fn works_fine() {
        use ic_stable_memory::{AsDynSizeBytes, AsFixedSizeBytes};

        assert_eq!(A1::SIZE, u64::SIZE + u32::SIZE + usize::SIZE);

        let a_1 = A1 { x: 1, y: 2, z: 3 };
        let a_1_buf = a_1.as_new_fixed_size_bytes();
        let a_1_copy = A1::from_fixed_size_bytes(&a_1_buf);

        assert_eq!(a_1, a_1_copy);

        assert_eq!(A2::SIZE, u64::SIZE + u32::SIZE + usize::SIZE);

        let a_2 = A2(1, 2, 3);
        let a_2_buf = a_2.as_new_fixed_size_bytes();
        let a_2_copy = A2::from_fixed_size_bytes(&a_2_buf);

        assert_eq!(a_2, a_2_copy);

        assert_eq!(A3::SIZE, 0);

        let a_3 = A3;
        let a_3_buf = a_3.as_new_fixed_size_bytes();
        let a_3_copy = A3::from_fixed_size_bytes(&a_3_buf);

        assert_eq!(a_3, a_3_copy);

        assert_eq!(B::SIZE, u8::SIZE + u64::SIZE + u16::SIZE);

        let b_1 = B::X;
        let b_1_buf = b_1.as_new_fixed_size_bytes();
        let b_1_copy = B::from_fixed_size_bytes(&b_1_buf);

        assert_eq!(b_1, b_1_copy);

        let b_2 = B::Y(10);
        let b_2_buf = b_2.as_new_fixed_size_bytes();
        let b_2_copy = B::from_fixed_size_bytes(&b_2_buf);

        assert_eq!(b_2, b_2_copy);

        let b_3 = B::Z { a: 1, b: 2 };
        let b_3_buf = b_3.as_new_fixed_size_bytes();
        let b_3_copy = B::from_fixed_size_bytes(&b_3_buf);

        assert_eq!(b_3, b_3_copy);

        let c = C {
            x: 10,
            y: 20,
            p: Principal::management_canister(),
        };
        let mut c_buf = c.as_dyn_size_bytes();
        c_buf.extend(vec![0u8; 10]);

        let c_copy = C::from_dyn_size_bytes(&c_buf);

        assert_eq!(c, c_copy);
    }
}

#[cfg(test)]
mod tests {
    use candid::{CandidType, Deserialize};
    use ic_stable_memory::collections::{
        SBTreeMap, SBTreeSet, SCertifiedBTreeMap, SHashMap, SHashSet, SLog, SVec,
    };
    use ic_stable_memory::derive::{AsFixedSizeBytes, CandidAsDynSizeBytes, StableType};
    use ic_stable_memory::utils::certification::{
        leaf, leaf_hash, AsHashTree, AsHashableBytes, Hash,
    };
    use ic_stable_memory::utils::DebuglessUnwrap;
    use ic_stable_memory::{
        get_allocated_size, retrieve_custom_data, stable_memory_init, store_custom_data, SBox,
    };
    use rand::rngs::ThreadRng;
    use rand::{thread_rng, Rng};
    use std::borrow::Borrow;

    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                            abcdefghijklmnopqrstuvwxyz\
                            0123456789)(*&^%$#@!~";

    pub fn generate_random_string(rng: &mut ThreadRng) -> String {
        let len = rng.gen_range(10..1000usize);

        (0..len)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }

    #[derive(
        Clone,
        Debug,
        Ord,
        PartialOrd,
        Eq,
        PartialEq,
        Default,
        CandidType,
        Deserialize,
        CandidAsDynSizeBytes,
        StableType,
    )]
    struct WrappedString(pub String);

    impl Borrow<String> for WrappedString {
        fn borrow(&self) -> &String {
            self.0.borrow()
        }
    }

    impl AsHashTree for WrappedString {
        fn root_hash(&self) -> Hash {
            leaf_hash(self.0.as_bytes())
        }
    }

    impl AsHashableBytes for WrappedString {
        fn as_hashable_bytes(&self) -> Vec<u8> {
            self.0.as_bytes().to_vec()
        }
    }

    #[derive(AsFixedSizeBytes, StableType, Default)]
    struct State {
        vec: SVec<SBox<String>>,
        log: SLog<SBox<String>>,
        hash_map: SHashMap<SBox<String>, SBox<String>>,
        hash_set: SHashSet<SBox<String>>,
        btree_map: SBTreeMap<SBox<String>, SBox<String>>,
        btree_set: SBTreeSet<SBox<String>>,
        certified_btree_map: SCertifiedBTreeMap<SBox<WrappedString>, SBox<WrappedString>>,
    }

    #[test]
    fn big_state_works_fine() {
        ic_stable_memory::stable::clear();
        stable_memory_init();

        {
            let mut rng = thread_rng();
            let mut state = State::default();

            for _ in 0..100 {
                let str = generate_random_string(&mut rng);

                state.vec.push(SBox::new(str.clone()).unwrap()).unwrap();
                state.log.push(SBox::new(str.clone()).unwrap()).unwrap();
                state
                    .hash_map
                    .insert(
                        SBox::new(str.clone()).unwrap(),
                        SBox::new(str.clone()).unwrap(),
                    )
                    .unwrap();
                state
                    .hash_set
                    .insert(SBox::new(str.clone()).unwrap())
                    .unwrap();
                state
                    .btree_map
                    .insert(
                        SBox::new(str.clone()).unwrap(),
                        SBox::new(str.clone()).unwrap(),
                    )
                    .unwrap();
                state
                    .btree_set
                    .insert(SBox::new(str.clone()).unwrap())
                    .unwrap();
                state
                    .certified_btree_map
                    .insert_and_commit(
                        SBox::new(WrappedString(str.clone())).unwrap(),
                        SBox::new(WrappedString(str)).unwrap(),
                    )
                    .unwrap();

                for i in 0..state.vec.len() {
                    let val = state.vec.get(i).unwrap();
                    assert_eq!(*state.log.get(i as u64).unwrap(), *val);
                    assert_eq!(*state.hash_map.get(&*val).unwrap(), *val);
                    assert!(state.hash_set.contains(&*val));
                    assert_eq!(*state.btree_map.get(&*val).unwrap(), *val);
                    assert!(state.btree_set.contains(&*val));
                    assert_eq!(
                        *state
                            .certified_btree_map
                            .get(&WrappedString(val.get().clone()))
                            .unwrap()
                            .get(),
                        WrappedString(val.get().clone())
                    );

                    let w = state
                        .certified_btree_map
                        .witness_with(&WrappedString(val.get().clone()), |v| {
                            leaf(v.get().as_hashable_bytes())
                        });
                    assert_eq!(w.reconstruct(), state.certified_btree_map.root_hash());

                    store_custom_data(1, SBox::new(state).debugless_unwrap());
                    state = retrieve_custom_data(1).unwrap().into_inner();
                }
            }
        }

        assert_eq!(get_allocated_size(), 0);
    }
}
