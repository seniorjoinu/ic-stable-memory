use crate::collections::hash_map::SHashMap;
use crate::primitive::s_unsafe_cell::SUnsafeCell;
use crate::{_get_custom_data_ptr, _set_custom_data_ptr};
use candid::CandidType;
use ic_cdk::trap;
use serde::de::DeserializeOwned;

static mut VARS: Option<SHashMap<String, u64>> = None;

pub fn init_vars() {
    unsafe { VARS = Some(SHashMap::new_with_capacity(101)) }
}

pub fn store_vars() {
    let vars = unsafe { VARS.take() };
    let vars_box = SUnsafeCell::new(&vars.expect("Stable vars are not initialized yet"));

    _set_custom_data_ptr(0, unsafe { vars_box.as_ptr() });
}

pub fn reinit_vars() {
    let vars_box_ptr = _get_custom_data_ptr(0);
    let vars_box = unsafe { SUnsafeCell::from_ptr(vars_box_ptr) };

    unsafe { VARS = Some(vars_box.get_cloned()) }
}

pub fn set_var<T: CandidType + DeserializeOwned>(name: &str, value: &T) {
    let val_box = SUnsafeCell::new(value);

    unsafe {
        VARS.as_mut()
            .expect("Stable vars are not initialized yet")
            .insert(String::from(name), val_box.as_ptr())
    };
}

pub fn get_var<T: CandidType + DeserializeOwned>(name: &str) -> T {
    unsafe {
        SUnsafeCell::from_ptr(
            VARS.as_ref()
                .expect("Stable vars are not initialized yet")
                .get_cloned(&String::from(name))
                .unwrap_or_else(|| trap(format!("Invalid stable var name {}", name).as_str())),
        )
        .get_cloned()
    }
}
