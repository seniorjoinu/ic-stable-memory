use candid::de::IDLDeserialize;
use candid::utils::ArgumentDecoder;
use candid::{CandidType, Deserialize, Result};

pub trait AsDynSizeBytes {
    fn as_dyn_size_bytes(&self) -> Vec<u8>;
    fn from_dyn_size_bytes(buf: &[u8]) -> Self;
}

#[cfg(not(feature = "custom_dyn_encoding"))]
use crate::encoding::AsFixedSizeBytes;

#[cfg(not(feature = "custom_dyn_encoding"))]
use crate::primitive::s_box::SBox;

#[cfg(not(feature = "custom_dyn_encoding"))]
impl<T: AsFixedSizeBytes> AsDynSizeBytes for T {
    #[inline]
    fn as_dyn_size_bytes(&self) -> Vec<u8> {
        let mut v = vec![0u8; T::SIZE];
        self.as_fixed_size_bytes(&mut v);

        v
    }

    #[inline]
    fn from_dyn_size_bytes(buf: &[u8]) -> Self {
        Self::from_fixed_size_bytes(&buf[0..T::SIZE])
    }
}

#[cfg(not(feature = "custom_dyn_encoding"))]
impl AsDynSizeBytes for Vec<u8> {
    #[inline]
    fn as_dyn_size_bytes(&self) -> Vec<u8> {
        let mut v = vec![0u8; usize::SIZE + self.len()];

        self.len().as_fixed_size_bytes(&mut v[0..usize::SIZE]);
        v[usize::SIZE..(usize::SIZE + self.len())].copy_from_slice(&self);

        v
    }

    #[inline]
    fn from_dyn_size_bytes(buf: &[u8]) -> Self {
        let len = usize::from_fixed_size_bytes(&buf[0..usize::SIZE]);
        let mut v = vec![0u8; len];

        v.copy_from_slice(&buf[usize::SIZE..(usize::SIZE + len)]);

        v
    }
}

#[cfg(not(feature = "custom_dyn_encoding"))]
impl AsDynSizeBytes for String {
    #[inline]
    fn as_dyn_size_bytes(&self) -> Vec<u8> {
        let mut v = vec![0u8; usize::SIZE + self.len()];

        self.len().as_fixed_size_bytes(&mut v[0..usize::SIZE]);
        v[usize::SIZE..(usize::SIZE + self.len())].copy_from_slice(self.as_bytes());

        v
    }

    #[inline]
    fn from_dyn_size_bytes(buf: &[u8]) -> Self {
        let len = usize::from_fixed_size_bytes(&buf[0..usize::SIZE]);
        let mut v = vec![0u8; len];

        v.copy_from_slice(&buf[usize::SIZE..(usize::SIZE + len)]);

        String::from_utf8(v).unwrap()
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
