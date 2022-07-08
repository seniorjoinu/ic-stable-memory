#[macro_export]
macro_rules! s {
    ( $name:ident ) => {
        ic_stable_memory::utils::vars::get_var::<$name>(stringify!($name))
    };
    ( $name:ident = $val:expr ) => {
        ic_stable_memory::utils::vars::set_var::<$name>(stringify!($name), &$val)
    };
}
