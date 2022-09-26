use speedy::{Context, Readable, Reader, Writable, Writer};
use std::marker::PhantomData;

pub struct SPhantomData<T> {
    _marker: PhantomData<T>,
}

impl<T> SPhantomData<T> {
    pub(crate) const fn default() -> Self {
        Self {
            _marker: PhantomData {},
        }
    }
}

impl<'a, C: Context, T> Readable<'a, C> for SPhantomData<T> {
    fn read_from<R: Reader<'a, C>>(reader: &mut R) -> Result<Self, <C as speedy::Context>::Error> {
        Ok(SPhantomData::default())
    }
}

impl<T, C: Context> Writable<C> for SPhantomData<T> {
    fn write_to<W: ?Sized + Writer<C>>(
        &self,
        writer: &mut W,
    ) -> Result<(), <C as speedy::Context>::Error> {
        Ok(())
    }
}
