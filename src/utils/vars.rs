use crate::collections::hash_map::SHashMap;
use crate::primitive::s_box::SBox;
use crate::primitive::StableAllocated;
use crate::{_get_custom_data_ptr, _set_custom_data_ptr};
use ic_cdk::trap;
use speedy::{LittleEndian, Readable, Writable};
use std::cell::RefCell;

const MAX_VAR_NAME_LEN: usize = 128;
type Variables = SHashMap<[u8; MAX_VAR_NAME_LEN], u64>;

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
        let vars = VARS.take();
        let mut vars_box = SBox::new(vars.expect("Stable vars are not initialized yet"));
        vars_box.move_to_stable();

        _set_custom_data_ptr(0, vars_box.as_ptr());
    } else {
        unreachable!("Stable variables are not initialized");
    }
}

pub fn reinit_vars() {
    if VARS.borrow().is_none() {
        let vars_box_ptr = _get_custom_data_ptr(0);
        let vars_box: SBox<Variables> = unsafe { SBox::from_ptr(vars_box_ptr) };
        let vars = vars_box.get_cloned();

        unsafe { vars_box.stable_drop() };

        *VARS.borrow_mut() = Some(vars);
    } else {
        unreachable!("Stable variables are already initialized");
    }
}

fn name_to_arr(name: &str) -> [u8; MAX_VAR_NAME_LEN] {
    assert!(
        name.len() <= MAX_VAR_NAME_LEN,
        "Stable variable name is too long (max 128 chars)"
    );

    let mut bytes = [0u8; MAX_VAR_NAME_LEN];
    bytes[0..name.len()].copy_from_slice(name.as_bytes());

    bytes
}

pub fn set_var<'a, T: Readable<'a, LittleEndian> + Writable<LittleEndian>>(name: &str, value: T) {
    let bytes = name_to_arr(name);
    let mut val_box = SBox::new(value);
    val_box.move_to_stable();

    let val_box_ptr = val_box.as_ptr();

    if let Some(m) = &mut *VARS.borrow_mut() {
        m.insert(bytes, val_box_ptr);
    } else {
        unreachable!("Stable variables are not initialized");
    }
}

pub fn get_var<'a, T: Readable<'a, LittleEndian> + Writable<LittleEndian>>(name: &str) -> T {
    let ptr = if let Some(m) = &*VARS.borrow() {
        let bytes = name_to_arr(name);

        m.get_copy(&bytes)
            .unwrap_or_else(|| trap(format!("Invalid stable var name {}", name).as_str()))
    } else {
        unreachable!("Stable variables are not initialized");
    };

    unsafe { SBox::<T>::from_ptr(ptr).get_cloned() }
}

pub fn remove_var<'a, T: Readable<'a, LittleEndian> + Writable<LittleEndian>>(name: &str) -> T {
    if let Some(vars) = &mut *VARS.borrow_mut() {
        let bytes = name_to_arr(name);

        let sbox_ptr = vars
            .remove(&bytes)
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
    use crate::{s, s_remove, stable, stable_memory_init};

    type Var = Vec<u8>;

    #[test]
    fn basic_flow_works_fine() {
        stable::clear();
        stable_memory_init(true, 0);

        s! { Var = vec![1u8, 2, 3, 4] };
        let v = s!(Var);

        assert_eq!(v, vec![1u8, 2, 3, 4]);

        let v1 = s_remove!(Var);

        assert_eq!(v1, v);
    }
}
