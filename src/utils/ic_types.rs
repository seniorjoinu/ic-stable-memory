use candid::types::{Serializer, Type};
use candid::{CandidType, Deserialize, Principal};
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

#[cfg(test)]
mod tests {
    use crate::utils::ic_types::SPrincipal;
    use candid::Principal;
    use speedy::{Readable, Writable};

    #[test]
    fn works_fine() {
        let p = SPrincipal(Principal::management_canister());
        let p_s = p.write_to_vec().expect("unable to write");

        let p1 = SPrincipal::read_from_buffer(&p_s).expect("unable to read");

        assert_eq!(p, p1);
    }
}
