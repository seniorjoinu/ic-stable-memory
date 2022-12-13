use crate::collections::hash_map::SHashMap;
use crate::primitive::s_box::SBox;
use crate::primitive::StableAllocated;
use crate::utils::encoding::AsDynSizeBytes;
use crate::{_get_custom_data_ptr, _set_custom_data_ptr};
use ic_cdk::trap;
use std::cell::RefCell;

type Variables = SHashMap<SBox<String>, u64>;

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

pub fn set_var<'a, T: AsDynSizeBytes<Vec<u8>>>(name: &str, value: T) {
    let mut name_box = SBox::new(String::from(name));
    let mut val_box = SBox::new(value);

    val_box.move_to_stable();
    let mut val_box_ptr = val_box.as_ptr();

    if let Some(m) = &mut *VARS.borrow_mut() {
        val_box_ptr = m.insert(name_box, val_box_ptr);

        let mut prev_val_box = unsafe { SBox::from_ptr(val_box_ptr) };
        prev_val_box.remove_from_stable();
    } else {
        unreachable!("Stable variables are not initialized");
    }
}

pub fn get_var<'a, T: AsDynSizeBytes<Vec<u8>>>(name: &str) -> T {
    let ptr = if let Some(m) = &*VARS.borrow() {
        let name_box = SBox::new(String::from(name));

        m.get_copy(&name_box)
            .unwrap_or_else(|| trap(format!("Invalid stable var name {}", name).as_str()))
    } else {
        unreachable!("Stable variables are not initialized");
    };

    unsafe { SBox::<T>::from_ptr(ptr).get_cloned() }
}

pub fn remove_var<'a, T: AsDynSizeBytes<Vec<u8>>>(name: &str) -> T {
    if let Some(vars) = &mut *VARS.borrow_mut() {
        let name_box = SBox::new(String::from(name));

        let sbox_ptr = vars
            .remove(&name_box)
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
