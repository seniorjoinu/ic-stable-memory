use crate::mem::s_slice::{Side, PTR_SIZE};
use crate::mem::Anyway;
use crate::utils::phantom_data::SPhantomData;
use crate::{allocate, deallocate, reallocate, SSlice};
use speedy::{LittleEndian, Readable, Writable};

#[derive(Readable, Writable)]
enum SRefCellState {
    Default,
    Ref(u16),
    Mut,
}

#[derive(Readable, Writable)]
pub struct SRefCell<T> {
    inner_ptr: u64,
    // TODO: state is ignored by now
    state: SRefCellState,
    _marker: SPhantomData<T>,
}

impl<'a, T: Readable<'a, LittleEndian> + Writable<LittleEndian>> SRefCell<T> {
    pub fn new(it: &T) -> Self {
        let buf = it.write_to_vec().unwrap();

        let slice_of_data = allocate(buf.len());
        slice_of_data.write_bytes(0, &buf);

        let slice_of_inner_ptr = allocate(PTR_SIZE);
        slice_of_inner_ptr.write_word(0, slice_of_data.ptr);

        Self {
            inner_ptr: slice_of_inner_ptr.ptr,
            state: SRefCellState::Default,
            _marker: SPhantomData::new(),
        }
    }

    // TODO: transform into borrow
    pub fn get_cloned(&self) -> T {
        let ptr = SSlice::_read_word(self.inner_ptr, 0);
        let slice_of_data = SSlice::from_ptr(ptr, Side::Start).unwrap();

        let mut buf = vec![0u8; slice_of_data.get_size_bytes()];
        slice_of_data.read_bytes(0, &mut buf);

        T::read_from_buffer_copying_data(&buf).unwrap()
    }

    // TODO: transform into borrow_mut
    pub fn set(&self, it: &T) {
        let buf = it.write_to_vec().unwrap();

        let ptr = SSlice::_read_word(self.inner_ptr, 0);
        let slice_of_data = SSlice::from_ptr(ptr, Side::Start).unwrap();

        let slice_of_data = if buf.len() > slice_of_data.get_size_bytes() {
            let it = reallocate(slice_of_data, buf.len()).anyway();
            SSlice::_write_word(self.inner_ptr, 0, it.ptr);

            it
        } else {
            slice_of_data
        };

        slice_of_data.write_bytes(0, &buf);
    }

    pub fn drop(self) {
        let slice_of_inner_ptr = SSlice::from_ptr(self.inner_ptr, Side::Start).unwrap();
        let ptr = slice_of_inner_ptr.read_word(0);
        let slice_of_data = SSlice::from_ptr(ptr, Side::Start).unwrap();

        deallocate(slice_of_data);
        deallocate(slice_of_inner_ptr);
    }
}

#[cfg(test)]
mod tests {
    use crate::primitive::s_ref_cell::SRefCell;
    use crate::{init_allocator, stable};

    #[test]
    fn basic_flow_works_fine() {
        stable::clear();
        stable::grow(1).unwrap();
        init_allocator(0);

        let refcell = SRefCell::new(&String::from("one"));
        assert_eq!(refcell.get_cloned(), String::from("one"));

        refcell.set(&String::from("two"));

        refcell.set(&String::from(
            "two three four five six seven eight nine ten",
        ));
        assert_eq!(
            refcell.get_cloned(),
            String::from("two three four five six seven eight nine ten")
        );

        refcell.drop();
    }
}
