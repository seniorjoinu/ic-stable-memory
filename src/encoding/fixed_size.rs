use candid::{Int, Nat, Principal};
use num_bigint::{BigInt, BigUint, Sign};

pub trait AsFixedSizeBytes: Sized {
    const SIZE: usize;
    type Buf: Buffer;

    fn as_fixed_size_bytes(&self, buf: &mut [u8]);
    fn from_fixed_size_bytes(buf: &[u8]) -> Self;

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

pub trait Buffer: private::Sealed {
    fn new(size: usize) -> Self;
    fn _deref(&self) -> &[u8];
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
