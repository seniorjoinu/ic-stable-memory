use proc_macro2::{self, TokenStream};
use quote::{format_ident, quote};
use syn::{Data, Fields, Generics, Ident, Index};

pub fn derive_stable_type_impl(ident: &Ident, data: &Data, generics: &Generics) -> TokenStream {
    if !generics.params.is_empty() {
        panic!("Generics not supported");
    }

    let (assume_owned_body, assume_not_owned_body) = match data {
        Data::Struct(d) => {
            let mut assume_owned_body = quote! {};
            let mut assume_not_owned_body = quote! {};

            for (idx, f) in d.fields.iter().enumerate() {
                let t = &f.ty;

                if let Some(i) = f.ident.clone() {
                    assume_owned_body = quote! { #assume_owned_body <#t as ic_stable_memory::StableType>::assume_owned_by_stable_memory(&mut self.#i); };
                    assume_not_owned_body = quote! { #assume_not_owned_body <#t as ic_stable_memory::StableType>::assume_not_owned_by_stable_memory(&mut self.#i); };
                } else {
                    let idx = Index::from(idx);

                    assume_owned_body = quote! { #assume_owned_body <#t as ic_stable_memory::StableType>::assume_owned_by_stable_memory(&mut self.#idx); };
                    assume_not_owned_body = quote! { #assume_not_owned_body <#t as ic_stable_memory::StableType>::assume_not_owned_by_stable_memory(&mut self.#idx); };
                };
            }

            (assume_owned_body, assume_not_owned_body)
        }
        Data::Enum(d) => {
            let mut assume_owned_body_total = quote! {};
            let mut assume_not_owned_body_total = quote! {};

            for v in d.variants.iter() {
                let v_name = &v.ident;

                let mut assume_owned_body = quote! {};
                let mut assume_not_owned_body = quote! {};

                let mut enum_header = quote! {};

                for (idx, f) in v.fields.iter().enumerate() {
                    let t = &f.ty;

                    if let Some(i) = f.ident.clone() {
                        enum_header = quote! { #enum_header #i, };

                        assume_owned_body = quote! { #assume_owned_body <#t as ic_stable_memory::StableType>::assume_owned_by_stable_memory(#i); };
                        assume_not_owned_body = quote! { #assume_not_owned_body <#t as ic_stable_memory::StableType>::assume_not_owned_by_stable_memory(#i); };
                    } else {
                        let val_i = format_ident!("val_{}", idx);

                        enum_header = quote! { #enum_header #val_i, };

                        assume_owned_body = quote! { #assume_owned_body <#t as ic_stable_memory::StableType>::assume_owned_by_stable_memory(#val_i); };
                        assume_not_owned_body = quote! { #assume_not_owned_body <#t as ic_stable_memory::StableType>::assume_not_owned_by_stable_memory(#val_i); };
                    };
                }

                (assume_owned_body_total, assume_not_owned_body_total) = match &v.fields {
                    Fields::Unit => {
                        let owned = quote! {
                            #assume_owned_body_total
                            Self::#v_name => {}
                        };

                        let not_owned = quote! {
                            #assume_not_owned_body_total
                            Self::#v_name => {}
                        };

                        (owned, not_owned)
                    }
                    Fields::Named(_) => {
                        let owned = quote! {
                            #assume_owned_body_total
                            Self::#v_name { #enum_header } => {
                                #assume_owned_body
                            }
                        };

                        let not_owned = quote! {
                            #assume_not_owned_body_total
                            Self::#v_name { #enum_header } => {
                                #assume_not_owned_body
                            }
                        };

                        (owned, not_owned)
                    }
                    Fields::Unnamed(_) => {
                        let owned = quote! {
                            #assume_owned_body_total
                            Self::#v_name(#enum_header) => {
                                #assume_owned_body
                            }
                        };

                        let not_owned = quote! {
                            #assume_not_owned_body_total
                            Self::#v_name(#enum_header) => {
                                #assume_not_owned_body
                            }
                        };

                        (owned, not_owned)
                    }
                };
            }

            assume_owned_body_total = quote! {
                unsafe {
                    match self {
                        #assume_owned_body_total
                    }
                }
            };

            assume_not_owned_body_total = quote! {
                unsafe {
                    match self {
                        #assume_not_owned_body_total
                    }
                }
            };

            (assume_owned_body_total, assume_not_owned_body_total)
        }
        _ => panic!("Unions not supported!"),
    };

    quote! {
        impl ic_stable_memory::StableType for #ident {
            unsafe fn assume_owned_by_stable_memory(&mut self) {
                #assume_owned_body
            }

            unsafe fn assume_not_owned_by_stable_memory(&mut self) {
                #assume_not_owned_body
            }
        }
    }
}
