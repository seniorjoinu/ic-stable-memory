use crate::collections::hash_map::SHashMap;
use crate::{SUnsafeCell, _get_custom_data_ptr, _set_custom_data_ptr};
use candid::CandidType;
use serde::de::DeserializeOwned;

type StableVars = SHashMap<String, u64>;

pub fn init_stable_vars() {
    let storage = StableVars::new();
    let storage_box = SUnsafeCell::new(&storage).expect("Unable to init stable vars storage");

    _set_custom_data_ptr(0, unsafe { storage_box.as_ptr() });
}

// TODO: return to cellbox for this one

unsafe fn get_stable_vars_storage() -> SUnsafeCell<StableVars> {
    let vec_box_ptr = _get_custom_data_ptr(0);

    SUnsafeCell::from_ptr(vec_box_ptr)
}

pub fn declare_stable_var<T: CandidType + DeserializeOwned>(
    name: String,
    data: T,
) -> SUnsafeCell<T> {
    let var_box = SUnsafeCell::new(&data).expect("Unable to declare stable var");

    persist_stable_var(name, &var_box);

    var_box
}

pub fn persist_stable_var<T: CandidType + DeserializeOwned>(
    name: String,
    var_box: &SUnsafeCell<T>,
) {
    let mut storage_box = unsafe { get_stable_vars_storage() };
    let mut storage = storage_box.get_cloned();

    storage
        .insert(name, unsafe { var_box.as_ptr() })
        .expect("Unable to declare stable var");
    unsafe {
        storage_box
            .set(&storage)
            .expect("Unable to declare stable var")
    };
}

#[macro_export]
macro_rules! s_declare {
    ($name:ident = $expr:expr) => {
        let $name = $crate::utils::vars::declare_stable_var(String::from(stringify!($name)), $expr);
    };
}

#[macro_export]
macro_rules! s_persist {
    ($name:ident) => {
        $crate::utils::vars::persist_stable_var(String::from(stringify!($name), $name));
    };
}
