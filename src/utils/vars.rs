use crate::collections::hash_map::SHashMap;
use crate::primitive::s_box::SBox;
use crate::primitive::s_box_mut::SBoxMut;
use crate::{_get_custom_data_ptr, _set_custom_data_ptr};
use fixedstr::fstr;
use ic_cdk::trap;
use speedy::{LittleEndian, Readable, Writable};
use std::cell::RefCell;

type Variables = SHashMap<[u8; 100], u64, [u8; 100], [u8; 8]>;

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
        let vars_box = SBox::new(vars.expect("Stable vars are not initialized yet"));

        _set_custom_data_ptr(0, vars_box.as_ptr());
    } else {
        unreachable!("Stable variables are not initialized");
    }
}

pub fn reinit_vars() {
    if VARS.borrow().is_none() {
        let vars_box_ptr = _get_custom_data_ptr(0);
        let vars_box: SBox<Variables> = unsafe { SBox::from_ptr(vars_box_ptr) };

        *VARS.borrow_mut() = Some(unsafe { vars_box.drop() });
    } else {
        unreachable!("Stable variables are already initialized");
    }
}

pub fn set_var<'a, T: Readable<'a, LittleEndian> + Writable<LittleEndian>>(name: &str, value: &T) {
    let val_box_ptr = SBoxMut::new(value).as_ptr();

    if let Some(m) = &mut *VARS.borrow_mut() {
        m.insert(&fstr::from(name).as_u8(), &val_box_ptr);
    } else {
        unreachable!("Stable variables are not initialized");
    }
}

pub fn get_var<'a, T: Readable<'a, LittleEndian> + Writable<LittleEndian>>(name: &str) -> T {
    let ptr = if let Some(m) = &*VARS.borrow() {
        m.get_copy(&fstr::from(name).as_u8())
            .unwrap_or_else(|| trap(format!("Invalid stable var name {}", name).as_str()))
    } else {
        unreachable!("Stable variables are not initialized");
    };

    unsafe { SBoxMut::from_ptr(ptr).get_cloned() }
}

#[cfg(test)]
mod tests {
    use crate::utils::vars::{get_var, set_var};
    use crate::{stable, stable_memory_init};

    #[test]
    fn basic_flow_works_fine() {
        stable::clear();
        stable_memory_init(true, 0);

        set_var("var", &vec![1u8, 2, 3, 4]);
        let v = get_var::<Vec<u8>>("var");

        assert_eq!(v, vec![1u8, 2, 3, 4]);
    }
}
