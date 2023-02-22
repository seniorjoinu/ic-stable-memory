use proc_macro2::{self, TokenStream};
use quote::quote;
use syn::{Generics, Ident, Index};

pub fn derive_candid_as_dyn_size_bytes_impl(ident: &Ident, generics: &Generics) -> TokenStream {
    if !generics.params.is_empty() {
        panic!("Generics not supported");
    }

    quote! {
        impl ic_stable_memory::AsDynSizeBytes for #ident {
            #[inline]
            fn as_dyn_size_bytes(&self) -> Vec<u8> {
                candid::encode_one(self).unwrap()
            }

            #[inline]
            fn from_dyn_size_bytes(arr: &[u8]) -> Self {
                ic_stable_memory::encoding::dyn_size::candid_decode_one_allow_trailing(arr).unwrap()
            }
        }
    }
}
