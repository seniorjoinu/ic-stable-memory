use crate::collections::hash_map::SHashMap;
use crate::mem::s_slice::Side;
use crate::primitive::s_box::SBox;
use crate::primitive::StableAllocated;
use crate::utils::encoding::{AsDynSizeBytes, AsFixedSizeBytes, FixedSize};
use crate::{_get_custom_data_ptr, _set_custom_data_ptr, allocate, deallocate, SSlice};
use arrayvec::ArrayString;
use ic_cdk::trap;
use std::cell::RefCell;

type VarName = ArrayString<100>;
type Variables = SHashMap<VarName, u64>;

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

pub fn set_var<T: AsDynSizeBytes>(name: &str, value: T) {
    let name = VarName::from(name).unwrap();
    let mut val_box = SBox::new(value);

    val_box.move_to_stable();
    let val_box_ptr = val_box.as_ptr();

    if let Some(m) = &mut *VARS.borrow_mut() {
        if let Some(prev_val_box_ptr) = m.insert(name, val_box_ptr) {
            let mut prev_val_box = unsafe { SBox::<T>::from_ptr(prev_val_box_ptr) };
            prev_val_box.remove_from_stable();
        }
    } else {
        unreachable!("Stable variables are not initialized");
    }
}

pub fn get_var<T: AsDynSizeBytes>(name: &str) -> T {
    let ptr = if let Some(m) = &*VARS.borrow() {
        let name = VarName::from(name).unwrap();

        m.get_copy(&name)
            .unwrap_or_else(|| trap(format!("Invalid stable var name {}", name).as_str()))
    } else {
        unreachable!("Stable variables are not initialized");
    };

    unsafe { SBox::<T>::from_ptr(ptr).get_cloned() }
}

pub fn remove_var<T: AsDynSizeBytes>(name: &str) -> T {
    if let Some(vars) = &mut *VARS.borrow_mut() {
        let name = VarName::from(name).unwrap();

        let sbox_ptr = vars
            .remove(&name)
            .unwrap_or_else(|| trap(format!("Invalid stable var name {}", name).as_str()));

        let mut sbox = unsafe { SBox::<T>::from_ptr(sbox_ptr) };
        let copy = sbox.get_cloned();
        sbox.remove_from_stable();

        copy
    } else {
        unreachable!("Stable variables are not initialized");
    }
}

#[cfg(test)]
mod tests {
    use crate::utils::encoding::{AsDynSizeBytes, AsFixedSizeBytes, FixedSize};
    use crate::{s, s_remove, stable, stable_memory_init};

    type Var = Vec<u8>;

    // TODO: remove AsDynSizeBytes, make SBox-es only accept blobs

    #[test]
    fn basic_flow_works_fine() {
        stable::clear();
        stable_memory_init(true, 0);

        s! { Var = 10u64 };
        let v = s!(Var);

        assert_eq!(v, vec![1u8, 2, 3, 4]);

        let v1 = s_remove!(Var);

        assert_eq!(v1, v);
    }
}
