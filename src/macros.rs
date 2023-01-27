#[macro_export]
macro_rules! s {
    ( $name:ident ) => {
        $crate::utils::vars::get_var::<$name>(stringify!($name).as_bytes())
    };
}

#[macro_export]
macro_rules! define {
    ( $name:ident = $val:expr ) => {
        $crate::utils::vars::set_var::<$name>(stringify!($name).as_bytes(), $val)
    };
}

#[macro_export]
macro_rules! undefine {
    ( $name:ident ) => {
        $crate::utils::vars::remove_var::<$name>(stringify!($name).as_bytes())
    };
}
