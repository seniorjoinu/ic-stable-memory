use speedy::{Context, Readable, Reader, Writable, Writer};
use std::marker::PhantomData;

pub struct SPhantomData<T> {
    _marker: PhantomData<T>,
}

impl<T> Default for SPhantomData<T> {
    fn default() -> Self {
        Self {
            _marker: PhantomData::default(),
        }
    }
}

impl<T> SPhantomData<T> {
    pub(crate) const fn new() -> Self {
        Self {
            _marker: PhantomData {},
        }
    }
}

impl<'a, C: Context, T> Readable<'a, C> for SPhantomData<T> {
    fn read_from<R: Reader<'a, C>>(_: &mut R) -> Result<Self, <C as speedy::Context>::Error> {
        Ok(SPhantomData::new())
    }
}

impl<T, C: Context> Writable<C> for SPhantomData<T> {
    fn write_to<W: ?Sized + Writer<C>>(
        &self,
        _: &mut W,
    ) -> Result<(), <C as speedy::Context>::Error> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use speedy::{Writable, Readable};
    use crate::utils::phantom_data::SPhantomData;

    #[test]
    fn ser_works_fine() {
        let d = SPhantomData::<i32>::default();
        let vec = d.write_to_vec().unwrap();
        let d1 = SPhantomData::<i32>::read_from_buffer_copying_data(&vec).unwrap();
    }
}