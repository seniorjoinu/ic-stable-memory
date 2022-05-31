use candid::de::IDLDeserialize;
use candid::utils::ArgumentDecoder;
use candid::{CandidType, Result};
use serde::Deserialize;

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
