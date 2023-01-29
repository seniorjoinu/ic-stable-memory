use proc_macro::TokenStream as Tokens;
use proc_macro2::{self, Ident as IdentPM, TokenStream};
use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DeriveInput, Fields, GenericParam, Generics, Ident, Index};

pub fn derive_stable_drop_impl(ident: &Ident, generics: &Generics) -> TokenStream {
    if !generics.params.is_empty() {
        panic!("Generics not supported");
    }

    quote! {
        impl ic_stable_memory::primitive::StableDrop for #ident {
            type Output = ();

            #[inline]
            unsafe fn stable_drop(self) {}
        }
    }
}
