use proc_macro::TokenStream as Tokens;
use proc_macro2::{self, Ident as IdentPM, TokenStream};
use quote::{format_ident, quote};
use stable_drop::derive_stable_drop_impl;
use stable_type::{derive_as_fixed_size_bytes, derive_fixed_size, derive_stable_allocated};
use syn::{parse_macro_input, Data, DeriveInput, Fields, Ident, Index};

mod stable_drop;
mod stable_type;

#[proc_macro_derive(StableType)]
pub fn derive_stable_type(input: Tokens) -> Tokens {
    let DeriveInput {
        ident,
        data,
        generics,
        ..
    } = parse_macro_input!(input);
    let fixed_size = derive_fixed_size(&ident, &data, &generics);
    let as_fixed_size_bytes = derive_as_fixed_size_bytes(&ident, &data, &generics);
    let stable_allocated = derive_stable_allocated(&ident, &generics);

    let out = quote! {
        #fixed_size
        #as_fixed_size_bytes
        #stable_allocated
    };

    out.into()
}

#[proc_macro_derive(StableDrop)]
pub fn derive_stable_drop(input: Tokens) -> Tokens {
    let DeriveInput {
        ident,
        data,
        generics,
        ..
    } = parse_macro_input!(input);

    derive_stable_drop_impl(&ident, &generics).into()
}
