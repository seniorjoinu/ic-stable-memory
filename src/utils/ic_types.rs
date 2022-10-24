use crate::primitive::StableAllocated;
use candid::types::{Serializer, Type};
use candid::{CandidType, Deserialize, Int, Nat, Principal};
use copy_as_bytes::traits::{AsBytes, SuperSized};
use num_bigint::{BigInt, BigUint, Sign};
use serde::Deserializer;
use speedy::{Context, Readable, Reader, Writable, Writer};
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SPrincipal(pub Principal);

impl CandidType for SPrincipal {
    fn _ty() -> Type {
        Principal::_ty()
    }

    fn idl_serialize<S>(&self, serializer: S) -> Result<(), S::Error>
    where
        S: Serializer,
    {
        self.0.idl_serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SPrincipal {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(SPrincipal(Principal::deserialize(deserializer)?))
    }
}

impl<'a, C: Context> Readable<'a, C> for SPrincipal {
    fn read_from<R: Reader<'a, C>>(reader: &mut R) -> Result<Self, <C as speedy::Context>::Error> {
        let len = reader.read_u32()?;
        let mut buf = vec![0u8; len as usize];
        reader.read_bytes(&mut buf)?;

        Ok(SPrincipal(Principal::from_slice(&buf)))
    }
}

impl<C: Context> Writable<C> for SPrincipal {
    fn write_to<T: ?Sized + Writer<C>>(
        &self,
        writer: &mut T,
    ) -> Result<(), <C as speedy::Context>::Error> {
        let slice = self.0.as_slice();

        writer.write_u32(slice.len() as u32)?;
        writer.write_bytes(slice)
    }
}

impl Display for SPrincipal {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl SuperSized for SPrincipal {
    const SIZE: usize = 30;
}

impl AsBytes for SPrincipal {
    fn to_bytes(self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        let slice = self.0.as_slice();

        buf[0] = slice.len() as u8;
        buf[1..(1 + slice.len())].copy_from_slice(slice);

        buf
    }

    fn from_bytes(arr: [u8; Self::SIZE]) -> Self {
        let len = arr[0] as usize;
        let mut buf = vec![0u8; len];
        buf.copy_from_slice(&arr[1..(1 + len)]);

        SPrincipal(Principal::from_slice(&buf))
    }
}

impl StableAllocated for SPrincipal {
    #[inline]
    fn move_to_stable(&mut self) {}

    #[inline]
    fn remove_from_stable(&mut self) {}

    #[inline]
    unsafe fn stable_drop(self) {}
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SNat(pub Nat);

impl CandidType for SNat {
    fn _ty() -> Type {
        Nat::_ty()
    }

    fn idl_serialize<S>(&self, serializer: S) -> Result<(), S::Error>
    where
        S: Serializer,
    {
        self.0.idl_serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SNat {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(SNat(Nat::deserialize(deserializer)?))
    }
}

impl<'a, C: Context> Readable<'a, C> for SNat {
    fn read_from<R: Reader<'a, C>>(reader: &mut R) -> Result<Self, <C as speedy::Context>::Error> {
        let len = reader.read_u32()?;
        let mut buf = vec![0u8; len as usize];
        reader.read_bytes(&mut buf)?;

        Ok(SNat(Nat::from(BigUint::from_bytes_le(&buf))))
    }
}

impl<C: Context> Writable<C> for SNat {
    fn write_to<T: ?Sized + Writer<C>>(
        &self,
        writer: &mut T,
    ) -> Result<(), <C as speedy::Context>::Error> {
        let slice = self.0 .0.to_bytes_le();

        writer.write_u32(slice.len() as u32)?;
        writer.write_bytes(&slice)
    }
}

impl Display for SNat {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl SuperSized for SNat {
    const SIZE: usize = 32;
}

impl AsBytes for SNat {
    fn to_bytes(self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        let vec = self.0 .0.to_bytes_le();
        buf[..vec.len()].copy_from_slice(&vec);

        buf
    }

    fn from_bytes(arr: [u8; Self::SIZE]) -> Self {
        let it = BigUint::from_bytes_le(&arr);

        SNat(Nat(it))
    }
}

impl StableAllocated for SNat {
    #[inline]
    fn move_to_stable(&mut self) {}

    #[inline]
    fn remove_from_stable(&mut self) {}

    #[inline]
    unsafe fn stable_drop(self) {}
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SInt(pub Int);

impl CandidType for SInt {
    fn _ty() -> Type {
        Int::_ty()
    }

    fn idl_serialize<S>(&self, serializer: S) -> Result<(), S::Error>
    where
        S: Serializer,
    {
        self.0.idl_serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SInt {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(SInt(Int::deserialize(deserializer)?))
    }
}

impl<'a, C: Context> Readable<'a, C> for SInt {
    fn read_from<R: Reader<'a, C>>(reader: &mut R) -> Result<Self, <C as speedy::Context>::Error> {
        let sign_byte = reader.read_u8()?;
        let sign = match sign_byte {
            0 => Sign::Plus,
            1 => Sign::Minus,
            2 => Sign::NoSign,
            _ => unreachable!(""),
        };

        let len = reader.read_u32()?;
        let mut buf = vec![0u8; len as usize];
        reader.read_bytes(&mut buf)?;

        Ok(SInt(Int::from(BigInt::from_bytes_le(sign, &buf))))
    }
}

impl<C: Context> Writable<C> for SInt {
    fn write_to<T: ?Sized + Writer<C>>(
        &self,
        writer: &mut T,
    ) -> Result<(), <C as speedy::Context>::Error> {
        let (sign, slice) = self.0 .0.to_bytes_le();

        let sign_byte = match sign {
            Sign::Plus => 0u8,
            Sign::Minus => 1u8,
            Sign::NoSign => 2u8,
        };

        writer.write_u8(sign_byte)?;
        writer.write_u32(slice.len() as u32)?;
        writer.write_bytes(&slice)
    }
}

impl Display for SInt {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl SuperSized for SInt {
    const SIZE: usize = 32;
}

impl AsBytes for SInt {
    fn to_bytes(self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        let (sign, bytes) = self.0 .0.to_bytes_le();

        buf[0] = match sign {
            Sign::Plus => 0u8,
            Sign::Minus => 1u8,
            Sign::NoSign => 2u8,
        };

        buf[1..(1 + bytes.len())].copy_from_slice(&bytes);

        buf
    }

    fn from_bytes(arr: [u8; Self::SIZE]) -> Self {
        let sign = match arr[0] {
            0 => Sign::Plus,
            1 => Sign::Minus,
            2 => Sign::NoSign,
            _ => unreachable!(),
        };

        let it = BigInt::from_bytes_le(sign, &arr[1..]);

        SInt(Int(it))
    }
}

impl StableAllocated for SInt {
    #[inline]
    fn move_to_stable(&mut self) {}

    #[inline]
    fn remove_from_stable(&mut self) {}

    #[inline]
    unsafe fn stable_drop(self) {}
}

#[cfg(test)]
mod tests {
    use crate::primitive::StableAllocated;
    use crate::utils::ic_types::{SInt, SNat, SPrincipal};
    use candid::{decode_one, encode_one, Int, Nat, Principal};
    use copy_as_bytes::traits::AsBytes;
    use speedy::{Readable, Writable};

    #[test]
    fn sprincipal_works_fine() {
        let p = SPrincipal(Principal::management_canister());
        let p_s = p.write_to_vec().expect("unable to write");

        let p1 = SPrincipal::read_from_buffer(&p_s).expect("unable to read");

        assert_eq!(p, p1);

        let p_c = encode_one(&p).unwrap();
        let p2 = decode_one::<SPrincipal>(&p_c).unwrap();

        assert_eq!(p, p2);

        let buf = p1.to_bytes();
        let mut p2 = SPrincipal::from_bytes(buf);

        assert_eq!(p, p2);

        p2.move_to_stable();
        p2.remove_from_stable();
        unsafe { p2.stable_drop() };

        println!("{}", p);
    }

    #[test]
    fn snat_works_fine() {
        let n = SNat(Nat::from(10));
        let n_s = n.write_to_vec().unwrap();

        let n1 = SNat::read_from_buffer(&n_s).unwrap();

        assert_eq!(n, n1);

        let n_c = encode_one(&n).unwrap();
        let n2 = decode_one::<SNat>(&n_c).unwrap();

        assert_eq!(n, n2);

        let buf = n1.to_bytes();
        let mut n2 = SNat::from_bytes(buf);

        assert_eq!(n, n2);

        n2.move_to_stable();
        n2.remove_from_stable();
        unsafe { n2.stable_drop() };

        println!("{}", n);
    }

    #[test]
    fn sint_works_fine() {
        let n = SInt(Int::from(10));
        let n_s = n.write_to_vec().unwrap();

        let n1 = SInt::read_from_buffer(&n_s).unwrap();

        assert_eq!(n, n1);

        let n_c = encode_one(&n).unwrap();
        let n2 = decode_one::<SInt>(&n_c).unwrap();

        assert_eq!(n, n2);

        let buf = n1.to_bytes();
        let mut n2 = SInt::from_bytes(buf);

        assert_eq!(n, n2);

        n2.move_to_stable();
        n2.remove_from_stable();
        unsafe { n2.stable_drop() };

        println!("{}", n);
    }
}
