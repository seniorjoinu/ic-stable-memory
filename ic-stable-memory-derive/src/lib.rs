use crate::as_fixed_size_bytes::derive_as_fixed_size_bytes_impl;
use crate::candid_as_dyn_size_bytes::derive_candid_as_dyn_size_bytes_impl;
use crate::fixed_size_as_dyn_size_bytes::derive_fixed_size_as_dyn_size_bytes_impl;
use crate::stable_type::derive_stable_type_impl;
use proc_macro::TokenStream as Tokens;
use syn::{parse_macro_input, DeriveInput};

mod as_fixed_size_bytes;
mod candid_as_dyn_size_bytes;
mod fixed_size_as_dyn_size_bytes;
mod stable_type;

/// Derives [ic_stable_memory::StableType] proxying flag toggling calls
#[proc_macro_derive(StableType)]
pub fn derive_stable_type(input: Tokens) -> Tokens {
    let DeriveInput {
        ident,
        data,
        generics,
        ..
    } = parse_macro_input!(input);

    derive_stable_type_impl(&ident, &data, &generics).into()
}

/// Derives [ic_stable_memory::AsFixedSizeBytes]. Does not support generics at the moment.
#[proc_macro_derive(AsFixedSizeBytes)]
pub fn derive_as_fixed_size_bytes(input: Tokens) -> Tokens {
    let DeriveInput {
        ident,
        data,
        generics,
        ..
    } = parse_macro_input!(input);

    derive_as_fixed_size_bytes_impl(&ident, &data, &generics).into()
}

/// Derives [ic_stable_memory::AsDynSizeBytes] for a type that already implements [candid::CandidType] and [candid::Deserialize].
#[proc_macro_derive(CandidAsDynSizeBytes)]
pub fn derive_candid_as_dyn_size_bytes(input: Tokens) -> Tokens {
    let DeriveInput {
        ident, generics, ..
    } = parse_macro_input!(input);

    derive_candid_as_dyn_size_bytes_impl(&ident, &generics).into()
}

/// Derives [ic_stable_memory::AsDynSizeBytes] for a type that already implements [ic_stable_memory::AsFixedSizeBytes].
#[proc_macro_derive(FixedSizeAsDynSizeBytes)]
pub fn derive_fixed_size_as_dyn_size_bytes(input: Tokens) -> Tokens {
    let DeriveInput {
        ident, generics, ..
    } = parse_macro_input!(input);

    derive_fixed_size_as_dyn_size_bytes_impl(&ident, &generics).into()
}
