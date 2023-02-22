//! Sized data encoding algorithms that power this crate.
//!
//! The main idea is to make sized data encoding as fast as possible, taking as little space as possible.
//! Each type implementing [AsFixedSizeBytes] trait is aware of its encoded size and of data type of
//! the byte buffer that is used to encode it. Constant generics enable us to encode such fixed size
//! types in `[u8; N]`, which is very good, since arrays are stack-based data structures and don't
//! involve expensive heap allocations, so most types encode themselves into such generic arrays.
//!
//! Generic types (such as [Option]) are not yet compatible with constant generics and therefore they
//! are encoded to [Vec] of [u8].
//!
//! [AsFixedSizeBytes] trait encapusaltes these differences providing a simple API.

use candid::{Int, Nat, Principal};
use num_bigint::{BigInt, BigUint, Sign};

/// Allows fast and space-efficient fixed size data encoding.
///
/// This trait can be implemented by using [derive::AsFixedSizeBytes] macro.
/// By default it is implemented for the following types:
/// 1. All primitive types: [i8], [u8], [i16], [u16], [i32], [u32], [i64], [u64], [i128], [u128], [f32], [f64], [bool], [()]
/// 2. Primitive type generic arrays: [i8; N], [u8; N], [i16; N], [u16; N], [i32; N], [u32; N], [i64: N], [u64; N], [i128; N], [u128; N], [f32; N], [f64; N], [bool; N], [(); N]
/// 3. Tuples up to 6 elements, where each element implements [AsFixedSizeBytes]
/// 4. [Option] of `T`, where `T`: [AsFixedSizeBytes]
/// 5. IC native types: [candid::Principal], [candid::Nat], [candid::Int]
pub trait AsFixedSizeBytes {
    /// Size of self when encoded
    const SIZE: usize;

    /// [Buffer] that is used to encode this value into
    type Buf: Buffer;

    /// Encodes itself into a slice of bytes.
    ///
    /// # Panics
    /// Will panic if out of bounds.
    fn as_fixed_size_bytes(&self, buf: &mut [u8]);

    /// Decodes itself from a slice of bytes.
    ///
    /// # Panics
    /// Will panic if out of bounds.
    fn from_fixed_size_bytes(buf: &[u8]) -> Self;

    /// Encodes itself into a new [Self::Buf] of size == [Self::SIZE]
    fn as_new_fixed_size_bytes(&self) -> Self::Buf {
        let mut buf = Self::Buf::new(Self::SIZE);
        self.as_fixed_size_bytes(buf._deref_mut());

        buf
    }
}

macro_rules! impl_for_number {
    ($ty:ty) => {
        impl AsFixedSizeBytes for $ty {
            const SIZE: usize = std::mem::size_of::<$ty>();
            type Buf = [u8; Self::SIZE];

            #[inline]
            fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
                buf.copy_from_slice(&self.to_le_bytes())
            }

            #[inline]
            fn from_fixed_size_bytes(buf: &[u8]) -> Self {
                let mut b = Self::Buf::new(Self::SIZE);
                b.copy_from_slice(buf);

                Self::from_le_bytes(b)
            }
        }
    };
}

impl_for_number!(i8);
impl_for_number!(u8);
impl_for_number!(i16);
impl_for_number!(u16);
impl_for_number!(i32);
impl_for_number!(u32);
impl_for_number!(i64);
impl_for_number!(u64);
impl_for_number!(i128);
impl_for_number!(u128);
impl_for_number!(isize);
impl_for_number!(usize);
impl_for_number!(f32);
impl_for_number!(f64);

impl AsFixedSizeBytes for char {
    const SIZE: usize = u32::SIZE;
    type Buf = [u8; Self::SIZE];

    fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
        u32::from(*self).as_fixed_size_bytes(buf)
    }

    fn from_fixed_size_bytes(buf: &[u8]) -> Self {
        char::try_from(u32::from_fixed_size_bytes(buf)).unwrap()
    }
}

impl AsFixedSizeBytes for () {
    const SIZE: usize = 0;
    type Buf = [u8; 0];

    #[inline]
    fn as_fixed_size_bytes(&self, _: &mut [u8]) {}

    #[inline]
    fn from_fixed_size_bytes(_: &[u8]) -> Self {}
}

impl AsFixedSizeBytes for bool {
    const SIZE: usize = 1;
    type Buf = [u8; 1];

    #[inline]
    fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
        if *self {
            buf[0] = 1;
        } else {
            buf[0] = 0;
        }
    }

    #[inline]
    fn from_fixed_size_bytes(buf: &[u8]) -> Self {
        assert!(buf[0] < 2);

        buf[0] == 1
    }
}

impl<T: AsFixedSizeBytes> AsFixedSizeBytes for Option<T> {
    const SIZE: usize = T::SIZE + 1;
    type Buf = Vec<u8>;

    #[inline]
    fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
        if let Some(it) = self {
            buf[0] = 1;
            it.as_fixed_size_bytes(&mut buf[1..Self::SIZE]);
        } else {
            buf[0] = 0;
        }
    }

    #[inline]
    fn from_fixed_size_bytes(buf: &[u8]) -> Self {
        if buf[0] == 1 {
            Some(T::from_fixed_size_bytes(&buf[1..Self::SIZE]))
        } else {
            None
        }
    }
}

impl<const N: usize> AsFixedSizeBytes for [(); N] {
    const SIZE: usize = 0;
    type Buf = [u8; 0];

    #[inline]
    fn as_fixed_size_bytes(&self, _: &mut [u8]) {}

    #[inline]
    fn from_fixed_size_bytes(_: &[u8]) -> Self {
        [(); N]
    }
}

macro_rules! impl_for_single_byte_type_arr {
    ($ty:ty, $zero:expr) => {
        impl<const N: usize> AsFixedSizeBytes for [$ty; N] {
            const SIZE: usize = N;
            type Buf = [u8; N];

            fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
                for i in 0..N {
                    let from = i * <$ty>::SIZE;
                    let to = from + <$ty>::SIZE;

                    self[i].as_fixed_size_bytes(&mut buf[from..to]);
                }
            }

            fn from_fixed_size_bytes(buf: &[u8]) -> Self {
                let mut it = [$zero; N];

                for i in 0..N {
                    let from = i * <$ty>::SIZE;
                    let to = from + <$ty>::SIZE;

                    it[i] = <$ty>::from_fixed_size_bytes(&buf[from..to]);
                }

                it
            }
        }
    };
}

impl_for_single_byte_type_arr!(bool, false);
impl_for_single_byte_type_arr!(i8, 0);
impl_for_single_byte_type_arr!(u8, 0);

macro_rules! impl_for_number_arr {
    ($ty:ty, $zero:expr) => {
        impl<const N: usize> AsFixedSizeBytes for [$ty; N] {
            const SIZE: usize = N;
            type Buf = Vec<u8>;

            fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
                for i in 0..N {
                    let from = i * <$ty>::SIZE;
                    let to = from + <$ty>::SIZE;

                    self[i].as_fixed_size_bytes(&mut buf[from..to]);
                }
            }

            fn from_fixed_size_bytes(buf: &[u8]) -> Self {
                let mut it = [$zero; N];

                for i in 0..N {
                    let from = i * <$ty>::SIZE;
                    let to = from + <$ty>::SIZE;

                    it[i] = <$ty>::from_fixed_size_bytes(&buf[from..to]);
                }

                it
            }
        }
    };
}

impl_for_number_arr!(i16, 0);
impl_for_number_arr!(u16, 0);
impl_for_number_arr!(i32, 0);
impl_for_number_arr!(u32, 0);
impl_for_number_arr!(i64, 0);
impl_for_number_arr!(u64, 0);
impl_for_number_arr!(i128, 0);
impl_for_number_arr!(u128, 0);
impl_for_number_arr!(isize, 0);
impl_for_number_arr!(usize, 0);
impl_for_number_arr!(f32, 0.0);
impl_for_number_arr!(f64, 0.0);

impl<const N: usize> AsFixedSizeBytes for [char; N] {
    const SIZE: usize = N * char::SIZE;
    type Buf = Vec<u8>;

    fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
        let mut from = 0;

        for c in self {
            c.as_fixed_size_bytes(&mut buf[from..(from + u32::SIZE)]);
            from += u32::SIZE;
        }
    }

    fn from_fixed_size_bytes(buf: &[u8]) -> Self {
        let mut s = [char::default(); N];

        for from in 0..N {
            s[from] =
                char::from_fixed_size_bytes(&buf[(from * u32::SIZE)..((from + 1) * u32::SIZE)]);
        }

        s
    }
}

impl<A: AsFixedSizeBytes> AsFixedSizeBytes for (A,) {
    const SIZE: usize = A::SIZE;
    type Buf = Vec<u8>;

    fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
        self.0.as_fixed_size_bytes(buf)
    }

    fn from_fixed_size_bytes(buf: &[u8]) -> Self {
        (A::from_fixed_size_bytes(buf),)
    }
}
impl<A: AsFixedSizeBytes, B: AsFixedSizeBytes> AsFixedSizeBytes for (A, B) {
    const SIZE: usize = A::SIZE + B::SIZE;
    type Buf = Vec<u8>;

    fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
        self.0.as_fixed_size_bytes(&mut buf[0..A::SIZE]);
        self.1
            .as_fixed_size_bytes(&mut buf[A::SIZE..(A::SIZE + B::SIZE)]);
    }

    fn from_fixed_size_bytes(buf: &[u8]) -> Self {
        (
            A::from_fixed_size_bytes(&buf[0..A::SIZE]),
            B::from_fixed_size_bytes(&buf[A::SIZE..(A::SIZE + B::SIZE)]),
        )
    }
}
impl<A: AsFixedSizeBytes, B: AsFixedSizeBytes, C: AsFixedSizeBytes> AsFixedSizeBytes for (A, B, C) {
    const SIZE: usize = A::SIZE + B::SIZE + C::SIZE;
    type Buf = Vec<u8>;

    fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
        self.0.as_fixed_size_bytes(&mut buf[0..A::SIZE]);
        self.1
            .as_fixed_size_bytes(&mut buf[A::SIZE..(A::SIZE + B::SIZE)]);
        self.2
            .as_fixed_size_bytes(&mut buf[(A::SIZE + B::SIZE)..(A::SIZE + B::SIZE + C::SIZE)]);
    }

    fn from_fixed_size_bytes(buf: &[u8]) -> Self {
        (
            A::from_fixed_size_bytes(&buf[0..A::SIZE]),
            B::from_fixed_size_bytes(&buf[A::SIZE..(A::SIZE + B::SIZE)]),
            C::from_fixed_size_bytes(&buf[(A::SIZE + B::SIZE)..(A::SIZE + B::SIZE + C::SIZE)]),
        )
    }
}
impl<A: AsFixedSizeBytes, B: AsFixedSizeBytes, C: AsFixedSizeBytes, D: AsFixedSizeBytes>
    AsFixedSizeBytes for (A, B, C, D)
{
    const SIZE: usize = A::SIZE + B::SIZE + C::SIZE + D::SIZE;
    type Buf = Vec<u8>;

    fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
        self.0.as_fixed_size_bytes(&mut buf[0..A::SIZE]);
        self.1
            .as_fixed_size_bytes(&mut buf[A::SIZE..(A::SIZE + B::SIZE)]);
        self.2
            .as_fixed_size_bytes(&mut buf[(A::SIZE + B::SIZE)..(A::SIZE + B::SIZE + C::SIZE)]);
        self.3.as_fixed_size_bytes(
            &mut buf[(A::SIZE + B::SIZE + C::SIZE)..(A::SIZE + B::SIZE + C::SIZE + D::SIZE)],
        );
    }

    fn from_fixed_size_bytes(buf: &[u8]) -> Self {
        (
            A::from_fixed_size_bytes(&buf[0..A::SIZE]),
            B::from_fixed_size_bytes(&buf[A::SIZE..(A::SIZE + B::SIZE)]),
            C::from_fixed_size_bytes(&buf[(A::SIZE + B::SIZE)..(A::SIZE + B::SIZE + C::SIZE)]),
            D::from_fixed_size_bytes(
                &buf[(A::SIZE + B::SIZE + C::SIZE)..(A::SIZE + B::SIZE + C::SIZE + D::SIZE)],
            ),
        )
    }
}
impl<
        A: AsFixedSizeBytes,
        B: AsFixedSizeBytes,
        C: AsFixedSizeBytes,
        D: AsFixedSizeBytes,
        E: AsFixedSizeBytes,
    > AsFixedSizeBytes for (A, B, C, D, E)
{
    const SIZE: usize = A::SIZE + B::SIZE + C::SIZE + D::SIZE + E::SIZE;
    type Buf = Vec<u8>;

    fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
        self.0.as_fixed_size_bytes(&mut buf[0..A::SIZE]);
        self.1
            .as_fixed_size_bytes(&mut buf[A::SIZE..(A::SIZE + B::SIZE)]);
        self.2
            .as_fixed_size_bytes(&mut buf[(A::SIZE + B::SIZE)..(A::SIZE + B::SIZE + C::SIZE)]);
        self.3.as_fixed_size_bytes(
            &mut buf[(A::SIZE + B::SIZE + C::SIZE)..(A::SIZE + B::SIZE + C::SIZE + D::SIZE)],
        );
        self.4.as_fixed_size_bytes(
            &mut buf[(A::SIZE + B::SIZE + C::SIZE + D::SIZE)
                ..(A::SIZE + B::SIZE + C::SIZE + D::SIZE + E::SIZE)],
        );
    }

    fn from_fixed_size_bytes(buf: &[u8]) -> Self {
        (
            A::from_fixed_size_bytes(&buf[0..A::SIZE]),
            B::from_fixed_size_bytes(&buf[A::SIZE..(A::SIZE + B::SIZE)]),
            C::from_fixed_size_bytes(&buf[(A::SIZE + B::SIZE)..(A::SIZE + B::SIZE + C::SIZE)]),
            D::from_fixed_size_bytes(
                &buf[(A::SIZE + B::SIZE + C::SIZE)..(A::SIZE + B::SIZE + C::SIZE + D::SIZE)],
            ),
            E::from_fixed_size_bytes(
                &buf[(A::SIZE + B::SIZE + C::SIZE + D::SIZE)
                    ..(A::SIZE + B::SIZE + C::SIZE + D::SIZE + E::SIZE)],
            ),
        )
    }
}
impl<
        A: AsFixedSizeBytes,
        B: AsFixedSizeBytes,
        C: AsFixedSizeBytes,
        D: AsFixedSizeBytes,
        E: AsFixedSizeBytes,
        F: AsFixedSizeBytes,
    > AsFixedSizeBytes for (A, B, C, D, E, F)
{
    const SIZE: usize = A::SIZE + B::SIZE + C::SIZE + D::SIZE + E::SIZE + F::SIZE;
    type Buf = Vec<u8>;

    fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
        self.0.as_fixed_size_bytes(&mut buf[0..A::SIZE]);
        self.1
            .as_fixed_size_bytes(&mut buf[A::SIZE..(A::SIZE + B::SIZE)]);
        self.2
            .as_fixed_size_bytes(&mut buf[(A::SIZE + B::SIZE)..(A::SIZE + B::SIZE + C::SIZE)]);
        self.3.as_fixed_size_bytes(
            &mut buf[(A::SIZE + B::SIZE + C::SIZE)..(A::SIZE + B::SIZE + C::SIZE + D::SIZE)],
        );
        self.4.as_fixed_size_bytes(
            &mut buf[(A::SIZE + B::SIZE + C::SIZE + D::SIZE)
                ..(A::SIZE + B::SIZE + C::SIZE + D::SIZE + E::SIZE)],
        );
        self.5.as_fixed_size_bytes(
            &mut buf[(A::SIZE + B::SIZE + C::SIZE + D::SIZE + E::SIZE)
                ..(A::SIZE + B::SIZE + C::SIZE + D::SIZE + E::SIZE + F::SIZE)],
        );
    }

    fn from_fixed_size_bytes(buf: &[u8]) -> Self {
        (
            A::from_fixed_size_bytes(&buf[0..A::SIZE]),
            B::from_fixed_size_bytes(&buf[A::SIZE..(A::SIZE + B::SIZE)]),
            C::from_fixed_size_bytes(&buf[(A::SIZE + B::SIZE)..(A::SIZE + B::SIZE + C::SIZE)]),
            D::from_fixed_size_bytes(
                &buf[(A::SIZE + B::SIZE + C::SIZE)..(A::SIZE + B::SIZE + C::SIZE + D::SIZE)],
            ),
            E::from_fixed_size_bytes(
                &buf[(A::SIZE + B::SIZE + C::SIZE + D::SIZE)
                    ..(A::SIZE + B::SIZE + C::SIZE + D::SIZE + E::SIZE)],
            ),
            F::from_fixed_size_bytes(
                &buf[(A::SIZE + B::SIZE + C::SIZE + D::SIZE + E::SIZE)
                    ..(A::SIZE + B::SIZE + C::SIZE + D::SIZE + E::SIZE + F::SIZE)],
            ),
        )
    }
}

impl AsFixedSizeBytes for Principal {
    const SIZE: usize = 30;
    type Buf = [u8; Self::SIZE];

    fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
        let slice = self.as_slice();

        buf[0] = slice.len() as u8;
        buf[1..(1 + slice.len())].copy_from_slice(slice);
    }

    fn from_fixed_size_bytes(buf: &[u8]) -> Self {
        let len = buf[0] as usize;

        Principal::from_slice(&buf[1..(1 + len)])
    }
}

impl AsFixedSizeBytes for Nat {
    const SIZE: usize = 32;
    type Buf = [u8; Self::SIZE];

    fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
        let vec = self.0.to_bytes_le();
        buf[0..vec.len()].copy_from_slice(&vec);
    }

    fn from_fixed_size_bytes(buf: &[u8]) -> Self {
        let it = BigUint::from_bytes_le(buf);

        Nat(it)
    }
}

impl AsFixedSizeBytes for Int {
    const SIZE: usize = 32;
    type Buf = [u8; Self::SIZE];

    fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
        let (sign, bytes) = self.0.to_bytes_le();

        buf[0] = match sign {
            Sign::Plus => 0u8,
            Sign::Minus => 1u8,
            Sign::NoSign => 2u8,
        };

        buf[1..(1 + bytes.len())].copy_from_slice(&bytes);
    }

    fn from_fixed_size_bytes(buf: &[u8]) -> Self {
        let sign = match buf[0] {
            0 => Sign::Plus,
            1 => Sign::Minus,
            2 => Sign::NoSign,
            _ => unreachable!(),
        };

        let it = BigInt::from_bytes_le(sign, &buf[1..]);

        Int(it)
    }
}

/// Either [u8; N] or [Vec] of [u8]
///
/// You can't implement this trait for any other type than these two.
pub trait Buffer: private::Sealed {
    #[doc(hidden)]
    fn new(size: usize) -> Self;
    #[doc(hidden)]
    fn _deref(&self) -> &[u8];
    #[doc(hidden)]
    fn _deref_mut(&mut self) -> &mut [u8];
}

impl<const N: usize> Buffer for [u8; N] {
    #[inline]
    fn new(_: usize) -> Self {
        [0u8; N]
    }

    #[inline]
    fn _deref(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self as *const u8, self.len()) }
    }

    #[inline]
    fn _deref_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self as *mut u8, self.len()) }
    }
}

impl Buffer for Vec<u8> {
    #[inline]
    fn new(size: usize) -> Self {
        vec![0u8; size]
    }

    #[inline]
    fn _deref(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.as_ptr(), self.len()) }
    }

    #[inline]
    fn _deref_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.as_mut_ptr(), self.len()) }
    }
}

mod private {
    pub trait Sealed {}

    impl<const N: usize> Sealed for [u8; N] {}
    impl Sealed for Vec<u8> {}
}
