use candid::{Int, Nat, Principal};
use num_bigint::{BigInt, BigUint, Sign};
use std::io::Write;
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

impl<const N: usize> FixedSize for [u8; N] {
    const SIZE: usize = N;
}

macro_rules! impl_for_primitive_arr {
    ($ty:ty) => {
        impl<const N: usize> FixedSize for [$ty; N] {
            const SIZE: usize = N * <$ty>::SIZE;
        }
    };
}

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

macro_rules! impl_for_u8_arr {
    ($size:expr) => {
        impl AsFixedSizeBytes for [u8; $size] {
            #[inline]
            fn as_fixed_size_bytes(&self) -> [u8; Self::SIZE] {
                *self
            }

            #[inline]
            fn from_fixed_size_bytes(arr: &[u8; Self::SIZE]) -> Self {
                *arr
            }
        }
    };
}

impl_for_u8_arr!(0);
impl_for_u8_arr!(1);
impl_for_u8_arr!(2);
impl_for_u8_arr!(3);
impl_for_u8_arr!(4);
impl_for_u8_arr!(5);
impl_for_u8_arr!(6);
impl_for_u8_arr!(7);
impl_for_u8_arr!(8);
impl_for_u8_arr!(9);
impl_for_u8_arr!(10);
impl_for_u8_arr!(11);
impl_for_u8_arr!(12);
impl_for_u8_arr!(13);
impl_for_u8_arr!(14);
impl_for_u8_arr!(15);
impl_for_u8_arr!(16);
impl_for_u8_arr!(17);
impl_for_u8_arr!(18);
impl_for_u8_arr!(19);
impl_for_u8_arr!(20);
impl_for_u8_arr!(21);
impl_for_u8_arr!(22);
impl_for_u8_arr!(23);
impl_for_u8_arr!(24);
impl_for_u8_arr!(25);
impl_for_u8_arr!(26);
impl_for_u8_arr!(27);
impl_for_u8_arr!(28);
impl_for_u8_arr!(29);
impl_for_u8_arr!(30);
impl_for_u8_arr!(31);
impl_for_u8_arr!(32);
impl_for_u8_arr!(33);
impl_for_u8_arr!(34);
impl_for_u8_arr!(35);
impl_for_u8_arr!(36);
impl_for_u8_arr!(37);
impl_for_u8_arr!(38);
impl_for_u8_arr!(39);
impl_for_u8_arr!(40);
impl_for_u8_arr!(41);
impl_for_u8_arr!(42);
impl_for_u8_arr!(43);
impl_for_u8_arr!(44);
impl_for_u8_arr!(45);
impl_for_u8_arr!(46);
impl_for_u8_arr!(47);
impl_for_u8_arr!(48);
impl_for_u8_arr!(49);
impl_for_u8_arr!(50);
impl_for_u8_arr!(51);
impl_for_u8_arr!(52);
impl_for_u8_arr!(53);
impl_for_u8_arr!(54);
impl_for_u8_arr!(55);
impl_for_u8_arr!(56);
impl_for_u8_arr!(57);
impl_for_u8_arr!(58);
impl_for_u8_arr!(59);
impl_for_u8_arr!(60);
impl_for_u8_arr!(61);
impl_for_u8_arr!(62);
impl_for_u8_arr!(63);
impl_for_u8_arr!(64);
impl_for_u8_arr!(128);
impl_for_u8_arr!(256);
impl_for_u8_arr!(512);
impl_for_u8_arr!(1024);
impl_for_u8_arr!(2048);
impl_for_u8_arr!(4096);

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
    fn as_dyn_size_bytes(&self, result: &mut Vec<u8>);
    fn from_dyn_size_bytes(buf: &[u8]) -> Self;

    fn as_new_dyn_size_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        self.as_dyn_size_bytes(&mut buf);

        buf
    }
}

impl AsDynSizeBytes for Vec<u8> {
    fn as_dyn_size_bytes(&self, result: &mut Vec<u8>) {
        result.extend_from_slice(self);
    }

    fn from_dyn_size_bytes(buf: &[u8]) -> Self {
        buf.to_vec()
    }
}

impl AsDynSizeBytes for String {
    fn as_dyn_size_bytes(&self, result: &mut Vec<u8>) {
        result.extend_from_slice(self.as_bytes());
    }

    fn from_dyn_size_bytes(buf: &[u8]) -> Self {
        Self::from_utf8(buf.to_vec()).unwrap()
    }
}
