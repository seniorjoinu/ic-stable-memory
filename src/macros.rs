#[macro_export]
macro_rules! s {
    ( $name:ident ) => {
        $crate::utils::vars::get_var::<$name>(stringify!($name))
    };
    ( $name:ident = $val:expr ) => {
        $crate::utils::vars::set_var::<$name>(stringify!($name), $val)
    };
}

#[macro_export]
macro_rules! s_remove {
    ( $name:ident ) => {
        $crate::utils::vars::remove_var::<$name>(stringify!($name))
    };
}
