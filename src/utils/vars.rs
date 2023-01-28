use crate::collections::hash_map::SHashMap;
use crate::mem::s_slice::Side;
use crate::primitive::s_box::SBox;
use crate::primitive::StableAllocated;
use crate::utils::encoding::{AsDynSizeBytes, AsFixedSizeBytes, FixedSize};
use crate::{_get_custom_data_ptr, _set_custom_data_ptr, allocate, deallocate, SSlice};
use ic_cdk::trap;
use std::cell::RefCell;

type Variables = SHashMap<[u8; 128], u64>;

#[thread_local]
static VARS: RefCell<Option<Variables>> = RefCell::new(None);

pub fn init_vars() {
    if VARS.borrow().is_none() {
        *VARS.borrow_mut() = Some(SHashMap::new());
    } else {
        unreachable!("Stable variables are already initialized");
    }
}

pub fn deinit_vars() {
    if VARS.borrow().is_some() {
        let vars = VARS.take().unwrap();
        let vars_buf = vars.as_fixed_size_bytes();
        let slice = allocate(vars_buf.len());

        slice.write_bytes(0, &vars_buf);

        _set_custom_data_ptr(0, slice.get_ptr());
    } else {
        unreachable!("Stable variables are not initialized");
    }
}

pub fn reinit_vars() {
    if VARS.borrow().is_none() {
        let slice_ptr = _get_custom_data_ptr(0);
        let slice = SSlice::from_ptr(slice_ptr, Side::Start).unwrap();

        let mut vars_buf = Variables::_u8_arr_of_size();
        slice.read_bytes(0, &mut vars_buf);

        let vars = Variables::from_fixed_size_bytes(&vars_buf);
        deallocate(slice);

        *VARS.borrow_mut() = Some(vars);
    } else {
        unreachable!("Stable variables are already initialized");
    }
}

fn format_name(name: &[u8]) -> [u8; 128] {
    let mut name = name.to_vec();
    assert!(name.len() <= 128, "Var name too long");
    name.resize(128, 0u8);

    name.try_into().unwrap()
}

pub fn set_var<T: AsDynSizeBytes>(name: &[u8], value: T) {
    if let Some(m) = &mut *VARS.borrow_mut() {
        let name = format_name(name);

        if m.contains_key(&name) {
            panic!("Stable variable is already defined!");
        }

        let mut val_box = SBox::new(value);

        val_box.move_to_stable();
        let val_box_ptr = val_box.as_ptr();

        // returns None, since we've checked with contains_key
        m.insert(name, val_box_ptr);
    } else {
        unreachable!("Stable variables are not initialized");
    }
}

pub fn get_var<T: AsDynSizeBytes>(name: &[u8]) -> T {
    let ptr = if let Some(m) = &*VARS.borrow() {
        let name = format_name(name);

        *m.get(&name).expect("Stable variable not found").read()
    } else {
        unreachable!("Stable variables are not initialized");
    };

    unsafe { SBox::<T>::from_ptr(ptr).get_cloned() }
}

pub fn remove_var<T: AsDynSizeBytes>(name: &[u8]) -> T {
    if let Some(vars) = &mut *VARS.borrow_mut() {
        let name = format_name(name);

        let sbox_ptr = vars.remove(&name).expect("Stable variable not found");

        let mut sbox = unsafe { SBox::<T>::from_ptr(sbox_ptr) };
        let copy = unsafe { sbox.get_cloned() };
        sbox.remove_from_stable();

        copy
    } else {
        unreachable!("Stable variables are not initialized");
    }
}

#[cfg(test)]
mod tests {
    use crate::utils::encoding::{AsDynSizeBytes, AsFixedSizeBytes, FixedSize};
    use crate::utils::vars::{get_var, remove_var, set_var};
    use crate::{
        define, deinit_vars, init_vars, reinit_vars, s, stable, stable_memory_init, undefine,
    };

    type Var = Vec<u8>;

    impl AsDynSizeBytes for Var {
        fn from_dyn_size_bytes(buf: &[u8]) -> Self {
            let mut len_buf = usize::_u8_arr_of_size();
            len_buf.copy_from_slice(&buf[..usize::SIZE]);
            let len = usize::from_fixed_size_bytes(&len_buf);

            let mut var = vec![0u8; len];
            var.copy_from_slice(&buf[usize::SIZE..(usize::SIZE + len)]);

            let i = [0u8; <u64 as crate::utils::encoding::FixedSize>::SIZE];

            var
        }

        fn as_dyn_size_bytes(&self) -> Vec<u8> {
            let mut result = vec![0u8; self.len() + usize::SIZE];
            let len_buf = self.len().as_fixed_size_bytes();

            result[..usize::SIZE].copy_from_slice(&len_buf);
            result[usize::SIZE..].copy_from_slice(self);

            result
        }
    }

    #[test]
    fn basic_flow_works_fine() {
        stable::clear();
        stable_memory_init(true, 0);

        define! { Var = vec![1u8, 2, 3, 4] };

        let v = s!(Var);
        assert_eq!(v, vec![1u8, 2, 3, 4]);

        undefine!(Var);

        define! { Var = vec![4u8, 3, 2, 1] };

        let v = s!(Var);
        assert_eq!(v, vec![4u8, 3, 2, 1]);

        let v1 = undefine!(Var);

        assert_eq!(v1, v);
    }

    #[test]
    #[should_panic]
    fn init_vars_should_panic_when_called_twice() {
        stable_memory_init(true, 0);

        init_vars();
    }

    #[test]
    #[should_panic]
    fn deinit_vars_should_panic_when_called_without_init() {
        deinit_vars();
    }

    #[test]
    #[should_panic]
    fn reinit_vars_should_panic_when_called_twice() {
        stable_memory_init(true, 0);

        reinit_vars();
    }

    #[test]
    #[should_panic]
    fn set_var_should_panic_when_vars_are_not_initted() {
        set_var(b"abc", vec![1u8, 2, 3]);
    }

    #[test]
    #[should_panic]
    fn get_var_should_panic_when_vars_are_not_initted() {
        get_var::<Var>(b"abc");
    }

    #[test]
    #[should_panic]
    fn remove_var_should_panic_when_vars_are_not_initted() {
        remove_var::<Var>(b"abc");
    }
}
