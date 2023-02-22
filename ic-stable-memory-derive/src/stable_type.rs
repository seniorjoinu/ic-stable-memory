use proc_macro2::{self, TokenStream};
use quote::{format_ident, quote};
use syn::{Data, Fields, Generics, Ident, Index};

pub fn derive_stable_type_impl(ident: &Ident, data: &Data, generics: &Generics) -> TokenStream {
    if !generics.params.is_empty() {
        panic!("Generics not supported");
    }

    let (flag_off_body, flag_on_body) = match data {
        Data::Struct(d) => {
            let mut flag_off_body = quote! {};
            let mut flag_on_body = quote! {};

            for (idx, f) in d.fields.iter().enumerate() {
                let t = &f.ty;

                if let Some(i) = f.ident.clone() {
                    flag_off_body = quote! { #flag_off_body <#t as ic_stable_memory::StableType>::stable_drop_flag_off(&mut self.#i); };
                    flag_on_body = quote! { #flag_on_body <#t as ic_stable_memory::StableType>::stable_drop_flag_on(&mut self.#i); };
                } else {
                    let idx = Index::from(idx);

                    flag_off_body = quote! { #flag_off_body <#t as ic_stable_memory::StableType>::stable_drop_flag_off(&mut self.#idx); };
                    flag_on_body = quote! { #flag_on_body <#t as ic_stable_memory::StableType>::stable_drop_flag_on(&mut self.#idx); };
                };
            }

            (flag_off_body, flag_on_body)
        }
        Data::Enum(d) => {
            let mut flag_off_body_total = quote! {};
            let mut flag_on_body_total = quote! {};

            for v in d.variants.iter() {
                let v_name = &v.ident;

                let mut flag_off_body = quote! {};
                let mut flag_on_body = quote! {};

                let mut enum_header = quote! {};

                for (idx, f) in v.fields.iter().enumerate() {
                    let t = &f.ty;

                    if let Some(i) = f.ident.clone() {
                        enum_header = quote! { #enum_header #i, };

                        flag_off_body = quote! { #flag_off_body <#t as ic_stable_memory::StableType>::stable_drop_flag_off(#i); };
                        flag_on_body = quote! { #flag_on_body <#t as ic_stable_memory::StableType>::stable_drop_flag_on(#i); };
                    } else {
                        let val_i = format_ident!("val_{}", idx);

                        enum_header = quote! { #enum_header #val_i, };

                        flag_off_body = quote! { #flag_off_body <#t as ic_stable_memory::StableType>::stable_drop_flag_off(#val_i); };
                        flag_on_body = quote! { #flag_on_body <#t as ic_stable_memory::StableType>::stable_drop_flag_on(#val_i); };
                    };
                }

                (flag_off_body_total, flag_on_body_total) = match &v.fields {
                    Fields::Unit => {
                        let owned = quote! {
                            #flag_off_body_total
                            Self::#v_name => {}
                        };

                        let not_owned = quote! {
                            #flag_on_body_total
                            Self::#v_name => {}
                        };

                        (owned, not_owned)
                    }
                    Fields::Named(_) => {
                        let owned = quote! {
                            #flag_off_body_total
                            Self::#v_name { #enum_header } => {
                                #flag_off_body
                            }
                        };

                        let not_owned = quote! {
                            #flag_on_body_total
                            Self::#v_name { #enum_header } => {
                                #flag_on_body
                            }
                        };

                        (owned, not_owned)
                    }
                    Fields::Unnamed(_) => {
                        let owned = quote! {
                            #flag_off_body_total
                            Self::#v_name(#enum_header) => {
                                #flag_off_body
                            }
                        };

                        let not_owned = quote! {
                            #flag_on_body_total
                            Self::#v_name(#enum_header) => {
                                #flag_on_body
                            }
                        };

                        (owned, not_owned)
                    }
                };
            }

            flag_off_body_total = quote! {
                unsafe {
                    match self {
                        #flag_off_body_total
                    }
                }
            };

            flag_on_body_total = quote! {
                unsafe {
                    match self {
                        #flag_on_body_total
                    }
                }
            };

            (flag_off_body_total, flag_on_body_total)
        }
        _ => panic!("Unions not supported!"),
    };

    quote! {
        impl ic_stable_memory::StableType for #ident {
            #[inline]
            unsafe fn stable_drop_flag_off(&mut self) {
                #flag_off_body
            }

            #[inline]
            unsafe fn stable_drop_flag_on(&mut self) {
                #flag_on_body
            }
        }
    }
}
