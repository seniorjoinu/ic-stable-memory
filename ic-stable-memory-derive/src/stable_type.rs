use proc_macro::TokenStream as Tokens;
use proc_macro2::{self, Ident as IdentPM, TokenStream};
use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DeriveInput, Fields, GenericParam, Generics, Ident, Index};

pub fn derive_stable_allocated(ident: &Ident, generics: &Generics) -> TokenStream {
    if !generics.params.is_empty() {
        panic!("Generics not supported");
    }

    quote! {
        impl ic_stable_memory::primitive::StableAllocated for #ident {
            #[inline]
            fn move_to_stable(&mut self) {}

            #[inline]
            fn remove_from_stable(&mut self) {}
        }
    }
}

pub fn derive_as_fixed_size_bytes(ident: &Ident, data: &Data, generics: &Generics) -> TokenStream {
    let (as_fixed_size_body, from_fixed_size_body) = match data {
        Data::Struct(d) => {
            let mut before = quote! { 0 };
            let mut after = quote! { 0 };

            let mut as_fixed_size_body = quote! {};
            let mut from_fixed_size_body = quote! {};
            let mut from_fixed_size_init_body = quote! {};

            for (idx, f) in d.fields.iter().enumerate() {
                let t = &f.ty;

                after =
                    quote! { #after + <#t as ic_stable_memory::utils::encoding::FixedSize>::SIZE };

                let (buf_ident, val_ident) = if let Some(i) = f.ident.clone() {
                    as_fixed_size_body = quote! { #as_fixed_size_body buf[(#before)..(#after)].copy_from_slice(&ic_stable_memory::utils::encoding::AsFixedSizeBytes::as_fixed_size_bytes(&self.#i)); };

                    let val_i = format_ident!("val_{}", i);
                    from_fixed_size_init_body = quote! { #from_fixed_size_init_body #i: #val_i, };

                    (format_ident!("buf_{}", i), val_i)
                } else {
                    let idx = Index::from(idx);
                    as_fixed_size_body = quote! { #as_fixed_size_body buf[(#before)..(#after)].copy_from_slice(&ic_stable_memory::utils::encoding::AsFixedSizeBytes::as_fixed_size_bytes(&self.#idx)); };

                    let val_i = format_ident!("val_{}", idx);
                    from_fixed_size_init_body = quote! { #from_fixed_size_init_body #val_i, };

                    (format_ident!("buf_{}", idx), val_i)
                };

                from_fixed_size_body = quote! { #from_fixed_size_body let mut #buf_ident = [0u8; <#t as ic_stable_memory::utils::encoding::FixedSize>::SIZE]; };
                from_fixed_size_body = quote! { #from_fixed_size_body #buf_ident.copy_from_slice(&buf[(#before)..(#after)]); };
                from_fixed_size_body = quote! { #from_fixed_size_body let #val_ident = <#t as ic_stable_memory::utils::encoding::AsFixedSizeBytes>::from_fixed_size_bytes(&#buf_ident); };

                before = quote! { #after };
            }

            from_fixed_size_body = match d.fields {
                Fields::Unit => quote! {
                    #from_fixed_size_body
                    Self
                },
                Fields::Named(_) => quote! {
                    #from_fixed_size_body
                    Self { #from_fixed_size_init_body }
                },
                Fields::Unnamed(_) => quote! {
                    #from_fixed_size_body
                    Self( #from_fixed_size_init_body )
                },
            };

            (as_fixed_size_body, from_fixed_size_body)
        }
        Data::Enum(d) => {
            let mut as_fixed_size_body_total = quote! {};
            let mut from_fixed_size_body_total = quote! {};

            for (v_idx, v) in d.variants.iter().enumerate() {
                let v_name = &v.ident;
                let v_idx = v_idx as u8;

                let mut before = quote! { 1 };
                let mut after = quote! { 1 };

                let mut as_fixed_size_body = quote! {};
                let mut from_fixed_size_body = quote! {};
                let mut from_fixed_size_init_body = quote! {};

                let mut enum_header = quote! {};

                for (idx, f) in v.fields.iter().enumerate() {
                    let t = &f.ty;

                    after = quote! { #after + <#t as ic_stable_memory::utils::encoding::FixedSize>::SIZE };

                    let (buf_ident, val_ident) = if let Some(i) = f.ident.clone() {
                        enum_header = quote! { #enum_header #i, };
                        as_fixed_size_body = quote! { #as_fixed_size_body buf[(#before)..(#after)].copy_from_slice(&ic_stable_memory::utils::encoding::AsFixedSizeBytes::as_fixed_size_bytes(#i)); };

                        let val_i = format_ident!("val_{}", i);
                        from_fixed_size_init_body =
                            quote! { #from_fixed_size_init_body #i: #val_i, };

                        (format_ident!("buf_{}", i), val_i)
                    } else {
                        let val_i = format_ident!("val_{}", idx);

                        enum_header = quote! { #enum_header #val_i, };

                        as_fixed_size_body = quote! { #as_fixed_size_body buf[(#before)..(#after)].copy_from_slice(&ic_stable_memory::utils::encoding::AsFixedSizeBytes::as_fixed_size_bytes(#val_i)); };
                        from_fixed_size_init_body = quote! { #from_fixed_size_init_body #val_i, };

                        (format_ident!("buf_{}", idx), val_i)
                    };

                    from_fixed_size_body = quote! { #from_fixed_size_body let mut #buf_ident = [0u8; <#t as ic_stable_memory::utils::encoding::FixedSize>::SIZE]; };
                    from_fixed_size_body = quote! { #from_fixed_size_body #buf_ident.copy_from_slice(&buf[(#before)..(#after)]); };
                    from_fixed_size_body = quote! { #from_fixed_size_body let #val_ident = <#t as ic_stable_memory::utils::encoding::AsFixedSizeBytes>::from_fixed_size_bytes(&#buf_ident); };

                    before = quote! { #after };
                }

                (from_fixed_size_body, as_fixed_size_body_total) = match &v.fields {
                    Fields::Unit => {
                        let from = quote! {
                            #from_fixed_size_body
                            Self::#v_name
                        };

                        let to = quote! {
                            #as_fixed_size_body_total
                            Self::#v_name => {
                                buf[0] = #v_idx;
                                #as_fixed_size_body
                            },
                        };

                        (from, to)
                    }
                    Fields::Named(_) => {
                        let from = quote! {
                            #from_fixed_size_body
                            Self::#v_name { #from_fixed_size_init_body }
                        };

                        let to = quote! {
                            #as_fixed_size_body_total
                            Self::#v_name { #enum_header } => {
                                buf[0] = #v_idx;
                                #as_fixed_size_body
                            },
                        };

                        (from, to)
                    }
                    Fields::Unnamed(_) => {
                        let from = quote! {
                            #from_fixed_size_body
                            Self::#v_name( #from_fixed_size_init_body )
                        };

                        let to = quote! {
                            #as_fixed_size_body_total
                            Self::#v_name(#enum_header) => {
                                buf[0] = #v_idx;
                                #as_fixed_size_body
                            },
                        };

                        (from, to)
                    }
                };

                from_fixed_size_body_total = quote! {
                    #from_fixed_size_body_total
                    #v_idx => {
                        #from_fixed_size_body
                    }
                };
            }

            as_fixed_size_body_total = quote! {
                match self {
                    #as_fixed_size_body_total
                }
            };

            from_fixed_size_body_total = quote! {
                let f = buf[0];
                match f {
                    #from_fixed_size_body_total,
                    _ => unreachable!(),
                }
            };

            (as_fixed_size_body_total, from_fixed_size_body_total)
        }
        _ => panic!("Unions not supported!"),
    };

    if !generics.params.is_empty() {
        panic!("Generics not supported");
    }

    quote! {
        impl ic_stable_memory::utils::encoding::AsFixedSizeBytes for #ident {
            fn as_fixed_size_bytes(&self) -> [u8; <Self as ic_stable_memory::utils::encoding::FixedSize>::SIZE] {
                let mut buf = [0u8; <Self as ic_stable_memory::utils::encoding::FixedSize>::SIZE];

                #as_fixed_size_body

                buf
            }

            fn from_fixed_size_bytes(buf: &[u8; <Self as ic_stable_memory::utils::encoding::FixedSize>::SIZE]) -> Self {
                #from_fixed_size_body
            }
        }
    }
}

pub fn derive_fixed_size(ident: &Ident, data: &Data, generics: &Generics) -> TokenStream {
    let size = match data {
        Data::Struct(d) => {
            let mut sizes = Vec::new();

            for f in &d.fields {
                let t = &f.ty;

                sizes.push(quote! { <#t as ic_stable_memory::utils::encoding::FixedSize>::SIZE });
            }

            if sizes.is_empty() {
                quote! { 0 }
            } else {
                quote! { #(#sizes)+* }
            }
        }
        Data::Enum(d) => {
            let mut sums = Vec::new();

            for v in &d.variants {
                let mut sizes = Vec::new();

                for f in &v.fields {
                    let t = &f.ty;

                    sizes.push(
                        quote! { <#t as ic_stable_memory::utils::encoding::FixedSize>::SIZE },
                    );
                }

                if sizes.is_empty() {
                    sums.push(quote! { 0 })
                } else {
                    sums.push(quote! { #(#sizes)+* });
                }
            }

            if sums.is_empty() {
                quote! { <u8 as ic_stable_memory::utils::encoding::FixedSize>::SIZE }
            } else if sums.len() == 1 {
                let s = sums.get(0).unwrap();
                quote! { <u8 as ic_stable_memory::utils::encoding::FixedSize>::SIZE + #s }
            } else {
                let s1 = sums.get(0).unwrap();
                let mut q = quote! { #s1 };

                for i in 1..sums.len() {
                    let s = sums.get(i).unwrap();
                    q = quote! { ic_stable_memory::utils::math::max_usize(#s, #q) };
                }

                quote! { <u8 as ic_stable_memory::utils::encoding::FixedSize>::SIZE + #q }
            }
        }
        _ => panic!("Unions not supported"),
    };

    if !generics.params.is_empty() {
        panic!("Generics not supported");
    }

    quote! {
        impl ic_stable_memory::utils::encoding::FixedSize for #ident {
            const SIZE: usize = #size;
        }
    }
}
