use crate::primitive::s_slice::{Side, PTR_SIZE};
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
    state: SRefCellState,
    _marker: SPhantomData<T>,
}

impl<'a, T: Readable<'a, LittleEndian> + Writable<LittleEndian>> SRefCell<T> {
    pub fn new(it: &T) -> Self {
        let buf = it.write_to_vec().unwrap();

        let slice_of_data = allocate::<T>(buf.len());
        slice_of_data._write_bytes(0, &buf);

        let slice_of_inner_ptr = allocate::<u64>(PTR_SIZE);
        slice_of_inner_ptr._write_word(0, slice_of_data.ptr);

        Self {
            inner_ptr: slice_of_inner_ptr.ptr,
            state: SRefCellState::Default,
            _marker: SPhantomData::default(),
        }
    }

    pub fn get_cloned(&self) -> T {
        let slice_of_inner_ptr =
            unsafe { SSlice::<u64>::from_ptr(self.inner_ptr, Side::Start).unwrap() };
        let ptr = slice_of_inner_ptr._read_word(0);
        let slice_of_data = unsafe { SSlice::<T>::from_ptr(ptr, Side::Start).unwrap() };

        let mut buf = vec![0u8; slice_of_data.get_size_bytes()];
        slice_of_data._read_bytes(0, &mut buf);

        T::read_from_buffer_copying_data(&buf).unwrap()
    }

    pub fn set(&mut self, it: &T) {
        let buf = it.write_to_vec().unwrap();

        let slice_of_inner_ptr =
            unsafe { SSlice::<u64>::from_ptr(self.inner_ptr, Side::Start).unwrap() };
        let ptr = slice_of_inner_ptr._read_word(0);
        let slice_of_data = unsafe { SSlice::<T>::from_ptr(ptr, Side::Start).unwrap() };

        let slice_of_data = if buf.len() > slice_of_data.get_size_bytes() {
            let it = reallocate(slice_of_data, buf.len());
            slice_of_inner_ptr._write_word(0, it.ptr);

            it
        } else {
            slice_of_data
        };

        slice_of_data._write_bytes(0, &buf);
    }
}
