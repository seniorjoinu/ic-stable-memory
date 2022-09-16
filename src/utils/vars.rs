use crate::collections::hash_map::SHashMap;
use crate::primitive::s_unsafe_cell::SUnsafeCell;
use crate::{_get_custom_data_ptr, _set_custom_data_ptr};
use ic_cdk::trap;
use speedy::{LittleEndian, Readable, Writable};
use std::cell::RefCell;

thread_local! {
    static VARS: RefCell<Option<SHashMap<String, u64>>> = RefCell::new(None);
}

pub fn init_vars() {
    VARS.with(|it| {
        if it.borrow().is_none() {
            *it.borrow_mut() = Some(SHashMap::new_with_capacity(101));
        } else {
            unreachable!("Stable variables are already initialized");
        }
    })
}

pub fn deinit_vars() {
    VARS.with(|it| {
        if it.borrow().is_some() {
            let vars = it.take();
            let vars_box = SUnsafeCell::new(&vars.expect("Stable vars are not initialized yet"));

            _set_custom_data_ptr(0, unsafe { vars_box.as_ptr() });

            *it.borrow_mut() = None;
        } else {
            unreachable!("Stable variables are not initialized");
        }
    })
}

pub fn reinit_vars() {
    VARS.with(|it| {
        if it.borrow().is_none() {
            let vars_box_ptr = _get_custom_data_ptr(0);
            let vars_box = unsafe { SUnsafeCell::from_ptr(vars_box_ptr) };

            *it.borrow_mut() = Some(vars_box.get_cloned());
        } else {
            unreachable!("Stable variables are already initialized");
        }
    })
}

pub fn set_var<'a, T: Readable<'a, LittleEndian> + Writable<LittleEndian>>(name: &str, value: &T) {
    let val_box_ptr = unsafe { SUnsafeCell::new(value).as_ptr() };

    VARS.with(|it| {
        if let Some(m) = &mut *it.borrow_mut() {
            m.insert(String::from(name), &val_box_ptr);
        } else {
            unreachable!("Stable variables are not initialized");
        }
    });
}

pub fn get_var<'a, T: Readable<'a, LittleEndian> + Writable<LittleEndian>>(name: &str) -> T {
    let ptr = VARS.with(|it| {
        if let Some(m) = &*it.borrow() {
            m.get_cloned(&String::from(name))
                .unwrap_or_else(|| trap(format!("Invalid stable var name {}", name).as_str()))
        } else {
            unreachable!("Stable variables are not initialized");
        }
    });

    unsafe { SUnsafeCell::from_ptr(ptr).get_cloned() }
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
