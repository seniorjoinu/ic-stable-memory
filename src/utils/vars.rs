use crate::collections::hash_map::SHashMap;
use crate::primitive::s_unsafe_cell::SUnsafeCell;
use crate::{OutOfMemory, _get_custom_data_ptr, _set_custom_data_ptr};
use candid::CandidType;
use serde::de::DeserializeOwned;

static mut VARS: Option<SHashMap<String, u64>> = None;

pub fn init_vars() {
    unsafe { VARS = Some(SHashMap::new()) }
}

pub fn store_vars() {
    let vars = unsafe { VARS.take() };
    let vars_box = SUnsafeCell::new(&vars.unwrap()).expect("Unable to store vars");

    _set_custom_data_ptr(0, unsafe { vars_box.as_ptr() });
}

pub fn reinit_vars() {
    let vars_box_ptr = _get_custom_data_ptr(0);
    let vars_box = unsafe { SUnsafeCell::from_ptr(vars_box_ptr) };

    unsafe { VARS = Some(vars_box.get_cloned()) }
}

pub fn set_var<T: CandidType + DeserializeOwned>(name: &str, value: &T) -> Result<(), OutOfMemory> {
    let val_box = SUnsafeCell::new(value)?;
    unsafe {
        VARS.as_mut()
            .unwrap()
            .insert(String::from(name), val_box.as_ptr())
            .expect("Unable to set var")
    };

    Ok(())
}

pub fn get_var<T: CandidType + DeserializeOwned>(name: &str) -> T {
    unsafe {
        SUnsafeCell::from_ptr(
            VARS.as_ref()
                .unwrap()
                .get_cloned(&String::from(name))
                .unwrap(),
        )
        .get_cloned()
    }
}
