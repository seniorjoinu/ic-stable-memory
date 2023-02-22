use proc_macro2::{self, TokenStream};
use quote::quote;
use syn::{Generics, Ident};

pub fn derive_fixed_size_as_dyn_size_bytes_impl(ident: &Ident, generics: &Generics) -> TokenStream {
    if !generics.params.is_empty() {
        panic!("Generics not supported");
    }

    quote! {
        impl ic_stable_memory::AsDynSizeBytes for #ident {
            #[inline]
            fn as_dyn_size_bytes(&self) -> Vec<u8> {
                ic_stable_memory::AsFixedSizeBytes::as_new_fixed_size_bytes(self).to_vec()
            }

            #[inline]
            fn from_dyn_size_bytes(arr: &[u8]) -> Self {
                ic_stable_memory::AsFixedSizeBytes::from_fixed_size_bytes(arr)
            }
        }
    }
}
