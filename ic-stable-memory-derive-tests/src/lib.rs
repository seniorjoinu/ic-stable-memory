#[cfg(test)]
mod derive_tests {
    use candid::{CandidType, Deserialize, Principal};
    use ic_stable_memory_derive::{CandidAsDynSizeBytes, StableDrop, StableType};

    #[derive(StableType, StableDrop, PartialEq, Eq, Debug)]
    struct A1 {
        x: u64,
        y: u32,
        z: usize,
    }

    #[derive(StableType, StableDrop, PartialEq, Eq, Debug)]
    struct A2(u64, u32, usize);

    #[derive(StableType, StableDrop, PartialEq, Eq, Debug)]
    struct A3;

    #[derive(StableType, StableDrop, PartialEq, Eq, Debug)]
    enum B {
        X,
        Y(u32),
        Z { a: u64, b: u16 },
    }

    #[derive(CandidType, Deserialize, CandidAsDynSizeBytes, PartialEq, Eq, Debug)]
    struct C {
        x: u32,
        y: u32,
        p: Principal,
    }

    #[test]
    fn works_fine() {
        use ic_stable_memory::utils::encoding::{AsDynSizeBytes, AsFixedSizeBytes, FixedSize};

        assert_eq!(A1::SIZE, u64::SIZE + u32::SIZE + usize::SIZE);
        assert_eq!(A2::SIZE, u64::SIZE + u32::SIZE + usize::SIZE);
        assert_eq!(A3::SIZE, 0);

        assert_eq!(B::SIZE, u8::SIZE + u64::SIZE + u16::SIZE);

        let a_1 = A1 { x: 1, y: 2, z: 3 };
        let a_1_buf = a_1.as_fixed_size_bytes();
        let a_1_copy = A1::from_fixed_size_bytes(&a_1_buf);

        assert_eq!(a_1, a_1_copy);

        let a_2 = A2(1, 2, 3);
        let a_2_buf = a_2.as_fixed_size_bytes();
        let a_2_copy = A2::from_fixed_size_bytes(&a_2_buf);

        assert_eq!(a_2, a_2_copy);

        let a_3 = A3;
        let a_3_buf = a_3.as_fixed_size_bytes();
        let a_3_copy = A3::from_fixed_size_bytes(&a_3_buf);

        assert_eq!(a_3, a_3_copy);

        let b_1 = B::X;
        let b_1_buf = b_1.as_fixed_size_bytes();
        let b_1_copy = B::from_fixed_size_bytes(&b_1_buf);

        assert_eq!(b_1, b_1_copy);

        let b_2 = B::Y(10);
        let b_2_buf = b_2.as_fixed_size_bytes();
        let b_2_copy = B::from_fixed_size_bytes(&b_2_buf);

        assert_eq!(b_2, b_2_copy);

        let b_3 = B::Z { a: 1, b: 2 };
        let b_3_buf = b_3.as_fixed_size_bytes();
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
