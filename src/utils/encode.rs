use candid::de::IDLDeserialize;
use candid::utils::ArgumentDecoder;
use candid::{encode_one, CandidType, Result};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::mem::size_of;
use std::slice::from_raw_parts;

pub fn decode_args_allow_trailing<'a, Tuple>(bytes: &'a [u8]) -> Result<Tuple>
where
    Tuple: ArgumentDecoder<'a>,
{
    let mut de = IDLDeserialize::new(bytes)?;
    let res = ArgumentDecoder::decode(&mut de)?;
    Ok(res)
}

pub fn decode_one_allow_trailing<'de, T: CandidType + Deserialize<'de>>(
    bytes: &'de [u8],
) -> Result<T> {
    let (res,) = decode_args_allow_trailing(bytes)?;
    Ok(res)
}

pub trait AsBytes {
    fn as_bytes(&self) -> Vec<u8>;
    fn from_bytes(bytes: &[u8]) -> Self;
}

impl<T: Copy> AsBytes for T {
    fn as_bytes(&self) -> Vec<u8> {
        unsafe {
            Vec::from(from_raw_parts(
                (self as *const T) as *const u8,
                size_of::<T>(),
            ))
        }
    }

    fn from_bytes(bytes: &[u8]) -> Self {
        unsafe { (bytes.as_ptr() as *const T).read() }
    }
}
