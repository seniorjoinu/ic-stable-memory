use candid::de::IDLDeserialize;
use candid::utils::ArgumentDecoder;
use candid::{CandidType, Deserialize, Int, Nat, Principal, Result};
use num_bigint::{BigInt, BigUint, Sign};
use std::mem::size_of;

pub trait FixedSize {
    const SIZE: usize;

    #[inline]
    fn _size() -> usize {
        Self::SIZE
    }

    #[inline]
    fn _u8_arr_of_size() -> [u8; Self::SIZE] {
        [0u8; Self::SIZE]
    }

    #[inline]
    fn _u8_vec_of_size() -> Vec<u8> {
        vec![0u8; Self::SIZE]
    }
}

macro_rules! impl_for_primitive {
    ($ty:ty) => {
        impl FixedSize for $ty {
            const SIZE: usize = size_of::<$ty>();
        }
    };
}

impl_for_primitive!(u8);
impl_for_primitive!(u16);
impl_for_primitive!(u32);
impl_for_primitive!(u64);
impl_for_primitive!(u128);
impl_for_primitive!(i8);
impl_for_primitive!(i16);
impl_for_primitive!(i32);
impl_for_primitive!(i64);
impl_for_primitive!(i128);
impl_for_primitive!(f32);
impl_for_primitive!(f64);
impl_for_primitive!(usize);
impl_for_primitive!(isize);
impl_for_primitive!(bool);
impl_for_primitive!(());

macro_rules! impl_for_primitive_arr {
    ($ty:ty) => {
        impl<const N: usize> FixedSize for [$ty; N] {
            const SIZE: usize = N * <$ty>::SIZE;
        }
    };
}

impl_for_primitive_arr!(u8);
impl_for_primitive_arr!(u16);
impl_for_primitive_arr!(u32);
impl_for_primitive_arr!(u64);
impl_for_primitive_arr!(u128);
impl_for_primitive_arr!(i8);
impl_for_primitive_arr!(i16);
impl_for_primitive_arr!(i32);
impl_for_primitive_arr!(i64);
impl_for_primitive_arr!(i128);
impl_for_primitive_arr!(f32);
impl_for_primitive_arr!(f64);
impl_for_primitive_arr!(usize);
impl_for_primitive_arr!(isize);
impl_for_primitive_arr!(bool);
impl_for_primitive_arr!(());

impl<T: FixedSize> FixedSize for Option<T> {
    const SIZE: usize = 1 + T::SIZE;
}

impl<A: FixedSize> FixedSize for (A,) {
    const SIZE: usize = A::SIZE;
}
impl<A: FixedSize, B: FixedSize> FixedSize for (A, B) {
    const SIZE: usize = A::SIZE + B::SIZE;
}
impl<A: FixedSize, B: FixedSize, C: FixedSize> FixedSize for (A, B, C) {
    const SIZE: usize = A::SIZE + B::SIZE + C::SIZE;
}
impl<A: FixedSize, B: FixedSize, C: FixedSize, D: FixedSize> FixedSize for (A, B, C, D) {
    const SIZE: usize = A::SIZE + B::SIZE + C::SIZE + D::SIZE;
}
impl<A: FixedSize, B: FixedSize, C: FixedSize, D: FixedSize, E: FixedSize> FixedSize
    for (A, B, C, D, E)
{
    const SIZE: usize = A::SIZE + B::SIZE + C::SIZE + D::SIZE + E::SIZE;
}
impl<A: FixedSize, B: FixedSize, C: FixedSize, D: FixedSize, E: FixedSize, F: FixedSize> FixedSize
    for (A, B, C, D, E, F)
{
    const SIZE: usize = A::SIZE + B::SIZE + C::SIZE + D::SIZE + E::SIZE + F::SIZE;
}
impl<
        A: FixedSize,
        B: FixedSize,
        C: FixedSize,
        D: FixedSize,
        E: FixedSize,
        F: FixedSize,
        G: FixedSize,
    > FixedSize for (A, B, C, D, E, F, G)
{
    const SIZE: usize = A::SIZE + B::SIZE + C::SIZE + D::SIZE + E::SIZE + F::SIZE + G::SIZE;
}
impl<
        A: FixedSize,
        B: FixedSize,
        C: FixedSize,
        D: FixedSize,
        E: FixedSize,
        F: FixedSize,
        G: FixedSize,
        H: FixedSize,
    > FixedSize for (A, B, C, D, E, F, G, H)
{
    const SIZE: usize =
        A::SIZE + B::SIZE + C::SIZE + D::SIZE + E::SIZE + F::SIZE + G::SIZE + H::SIZE;
}

impl FixedSize for Principal {
    const SIZE: usize = 30;
}

impl FixedSize for Nat {
    const SIZE: usize = 32;
}

impl FixedSize for Int {
    const SIZE: usize = 32;
}

pub trait AsFixedSizeBytes: FixedSize {
    fn as_fixed_size_bytes(&self) -> [u8; Self::SIZE];
    fn from_fixed_size_bytes(buf: &[u8; Self::SIZE]) -> Self;
}

impl AsFixedSizeBytes for u8 {
    #[inline]
    fn as_fixed_size_bytes(&self) -> [u8; u8::SIZE] {
        [*self]
    }

    #[inline]
    fn from_fixed_size_bytes(arr: &[u8; u8::SIZE]) -> Self {
        arr[0]
    }
}

impl AsFixedSizeBytes for i8 {
    #[inline]
    fn as_fixed_size_bytes(&self) -> [u8; i8::SIZE] {
        [*self as u8]
    }

    #[inline]
    fn from_fixed_size_bytes(arr: &[u8; i8::SIZE]) -> Self {
        arr[0] as i8
    }
}

impl AsFixedSizeBytes for bool {
    #[inline]
    fn as_fixed_size_bytes(&self) -> [u8; bool::SIZE] {
        [u8::from(*self)]
    }

    #[inline]
    fn from_fixed_size_bytes(arr: &[u8; bool::SIZE]) -> Self {
        debug_assert!(arr[0] < 2);

        arr[0] == 1
    }
}

macro_rules! impl_for_numbers {
    ($ty:ty) => {
        impl AsFixedSizeBytes for $ty {
            #[inline]
            fn as_fixed_size_bytes(&self) -> [u8; <$ty>::SIZE] {
                self.to_le_bytes()
            }

            #[inline]
            fn from_fixed_size_bytes(arr: &[u8; <$ty>::SIZE]) -> Self {
                Self::from_le_bytes(*arr)
            }
        }
    };
}

impl_for_numbers!(u16);
impl_for_numbers!(u32);
impl_for_numbers!(u64);
impl_for_numbers!(u128);
impl_for_numbers!(i16);
impl_for_numbers!(i32);
impl_for_numbers!(i64);
impl_for_numbers!(i128);
impl_for_numbers!(f32);
impl_for_numbers!(f64);
impl_for_numbers!(usize);
impl_for_numbers!(isize);

impl AsFixedSizeBytes for () {
    #[inline]
    fn as_fixed_size_bytes(&self) -> [u8; 0] {
        []
    }

    #[inline]
    fn from_fixed_size_bytes(_: &[u8; 0]) -> Self {}
}

impl<const N: usize> AsFixedSizeBytes for [u8; N] {
    #[inline]
    fn as_fixed_size_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b.copy_from_slice(self);

        b
    }

    #[inline]
    fn from_fixed_size_bytes(arr: &[u8; Self::SIZE]) -> Self {
        let mut b = [0u8; N];
        b.copy_from_slice(arr);

        b
    }
}

impl<T: AsFixedSizeBytes> AsFixedSizeBytes for Option<T>
where
    [(); T::SIZE]: Sized,
{
    fn as_fixed_size_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        if let Some(it) = self {
            buf[0] = 1;
            buf[1..].copy_from_slice(&it.as_fixed_size_bytes());
        }

        buf
    }

    fn from_fixed_size_bytes(arr: &[u8; Self::SIZE]) -> Self {
        if arr[0] == 0 {
            None
        } else {
            let mut buf = [0u8; T::SIZE];
            buf.copy_from_slice(&arr[1..]);

            Some(T::from_fixed_size_bytes(&buf))
        }
    }
}

impl AsFixedSizeBytes for Principal {
    fn as_fixed_size_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        let slice = self.as_slice();

        buf[0] = slice.len() as u8;
        buf[1..(1 + slice.len())].copy_from_slice(slice);

        buf
    }

    fn from_fixed_size_bytes(arr: &[u8; Self::SIZE]) -> Self {
        let len = arr[0] as usize;
        let mut buf = vec![0u8; len];
        buf.copy_from_slice(&arr[1..(1 + len)]);

        Principal::from_slice(&buf)
    }
}

impl AsFixedSizeBytes for Nat {
    fn as_fixed_size_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        let vec = self.0.to_bytes_le();
        buf[..vec.len()].copy_from_slice(&vec);

        buf
    }

    fn from_fixed_size_bytes(arr: &[u8; Self::SIZE]) -> Self {
        let it = BigUint::from_bytes_le(arr);

        Nat(it)
    }
}

impl AsFixedSizeBytes for Int {
    fn as_fixed_size_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        let (sign, bytes) = self.0.to_bytes_le();

        buf[0] = match sign {
            Sign::Plus => 0u8,
            Sign::Minus => 1u8,
            Sign::NoSign => 2u8,
        };

        buf[1..(1 + bytes.len())].copy_from_slice(&bytes);

        buf
    }

    fn from_fixed_size_bytes(arr: &[u8; Self::SIZE]) -> Self {
        let sign = match arr[0] {
            0 => Sign::Plus,
            1 => Sign::Minus,
            2 => Sign::NoSign,
            _ => unreachable!(),
        };

        let it = BigInt::from_bytes_le(sign, &arr[1..]);

        Int(it)
    }
}

pub trait AsDynSizeBytes {
    fn as_dyn_size_bytes(&self) -> Vec<u8>;
    fn from_dyn_size_bytes(buf: &[u8]) -> Self;
}

#[cfg(feature = "default_dyn_size_encoding")]
impl<T: AsFixedSizeBytes> AsDynSizeBytes for T
where
    [(); T::SIZE]: Sized,
{
    #[inline]
    fn as_dyn_size_bytes(&self) -> Vec<u8> {
        self.as_fixed_size_bytes().to_vec()
    }

    #[inline]
    fn from_dyn_size_bytes(buf: &[u8]) -> Self {
        let mut b = T::_u8_arr_of_size();
        b.copy_from_slice(&buf[0..T::SIZE]);

        Self::from_fixed_size_bytes(&b)
    }
}

pub fn candid_decode_args_allow_trailing<'a, Tuple>(bytes: &'a [u8]) -> Result<Tuple>
where
    Tuple: ArgumentDecoder<'a>,
{
    let mut de = IDLDeserialize::new(bytes)?;
    let res = ArgumentDecoder::decode(&mut de)?;

    Ok(res)
}

pub fn candid_decode_one_allow_trailing<'a, T>(bytes: &'a [u8]) -> Result<T>
where
    T: Deserialize<'a> + CandidType,
{
    let (res,) = candid_decode_args_allow_trailing(bytes)?;
    Ok(res)
}

#[cfg(test)]
mod benches {
    use crate::utils::encoding::{AsFixedSizeBytes, FixedSize};
    use candid::{Int, Nat, Principal};

    #[test]
    fn works_fine() {
        assert_eq!(u64::_size(), 8);
        assert_eq!(u64::_u8_vec_of_size().len(), 8);
        assert_eq!(u64::_u8_arr_of_size().len(), 8);

        assert_eq!(i8::from_fixed_size_bytes(&10i8.as_fixed_size_bytes()), 10);
        assert_eq!(u8::from_fixed_size_bytes(&10u8.as_fixed_size_bytes()), 10);
        assert!(bool::from_fixed_size_bytes(&true.as_fixed_size_bytes()));
        let c = Some(10).as_fixed_size_bytes();
        assert_eq!(Option::<u32>::from_fixed_size_bytes(&c), Some(10));
        assert_eq!(
            Option::<u32>::from_fixed_size_bytes(&Option::<u32>::as_fixed_size_bytes(&None)),
            None
        );
        assert_eq!(
            Principal::from_fixed_size_bytes(
                &Principal::management_canister().as_fixed_size_bytes()
            ),
            Principal::management_canister()
        );
        assert_eq!(
            Nat::from_fixed_size_bytes(&Nat::from(10).as_fixed_size_bytes()),
            Nat::from(10)
        );
        assert_eq!(
            Int::from_fixed_size_bytes(&Int::from(10).as_fixed_size_bytes()),
            Int::from(10)
        );
    }
}
