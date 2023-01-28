use proc_macro::TokenStream as Tokens;
use proc_macro2::{self, Ident as IdentPM, TokenStream};
use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DeriveInput, Fields, GenericParam, Generics, Ident, Index};

pub fn derive_stable_drop_impl(ident: &Ident, generics: &Generics) -> TokenStream {
    let mut gen = quote! {};
    let mut gen_orig = quote! {};
    let mut wher = &generics.where_clause;

    for g in &generics.params {
        match g {
            GenericParam::Lifetime(l) => {
                gen = quote! { #gen #l, };
                gen_orig = quote! { #gen_orig #l, };
            }
            GenericParam::Const(c) => {
                gen = quote! { #gen #c, };

                let i = &c.ident;
                gen_orig = quote! { #gen_orig #i, };
            }
            GenericParam::Type(t) => {
                gen = quote! { #gen #t: ic_stable_memory::primitive::StableDrop, };
                gen_orig = quote! { #gen_orig #t, };
            }
        }
    }

    quote! {
        impl<#gen> ic_stable_memory::primitive::StableDrop for #ident<#gen_orig> #wher {
            type Output = ();

            #[inline]
            unsafe fn stable_drop(self) {}
        }
    }
}
