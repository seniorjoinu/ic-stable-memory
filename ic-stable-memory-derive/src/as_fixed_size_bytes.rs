use proc_macro2::{self, TokenStream};
use quote::{format_ident, quote};
use syn::{Data, Fields, Generics, Ident, Index};

pub fn derive_as_fixed_size_bytes_impl(
    ident: &Ident,
    data: &Data,
    generics: &Generics,
) -> TokenStream {
    if !generics.params.is_empty() {
        panic!("Generics not supported");
    }

    let (as_fixed_size_body, from_fixed_size_body, size) = match data {
        Data::Struct(d) => {
            let mut before = quote! { 0 };
            let mut after = quote! { 0 };

            let mut as_fixed_size_body = quote! {};
            let mut from_fixed_size_body = quote! {};

            for (idx, f) in d.fields.iter().enumerate() {
                let t = &f.ty;

                after = quote! { #after + <#t as ic_stable_memory::AsFixedSizeBytes>::SIZE };

                if let Some(i) = f.ident.clone() {
                    as_fixed_size_body = quote! { #as_fixed_size_body ic_stable_memory::AsFixedSizeBytes::as_fixed_size_bytes(&self.#i, &mut buf[(#before)..(#after)]); };
                    from_fixed_size_body = quote! { #from_fixed_size_body #i: ic_stable_memory::AsFixedSizeBytes::from_fixed_size_bytes(&buf[(#before)..(#after)]), };
                } else {
                    let idx = Index::from(idx);

                    as_fixed_size_body = quote! { #as_fixed_size_body ic_stable_memory::AsFixedSizeBytes::as_fixed_size_bytes(&self.#idx, &mut buf[(#before)..(#after)]); };
                    from_fixed_size_body = quote! { #from_fixed_size_body ic_stable_memory::AsFixedSizeBytes::from_fixed_size_bytes(&buf[(#before)..(#after)]), };
                };

                before = quote! { #after };
            }

            from_fixed_size_body = match d.fields {
                Fields::Unit => quote! {
                    Self
                },
                Fields::Named(_) => quote! {
                    Self { #from_fixed_size_body }
                },
                Fields::Unnamed(_) => quote! {
                    Self ( #from_fixed_size_body )
                },
            };

            let mut sizes = Vec::new();

            for f in &d.fields {
                let t = &f.ty;

                sizes.push(quote! { <#t as ic_stable_memory::AsFixedSizeBytes>::SIZE });
            }

            let size = if sizes.is_empty() {
                quote! { 0 }
            } else {
                quote! { #(#sizes)+* }
            };

            (as_fixed_size_body, from_fixed_size_body, size)
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

                let mut enum_header = quote! {};

                for (idx, f) in v.fields.iter().enumerate() {
                    let t = &f.ty;

                    after = quote! { #after + <#t as ic_stable_memory::AsFixedSizeBytes>::SIZE };

                    if let Some(i) = f.ident.clone() {
                        enum_header = quote! { #enum_header #i, };

                        as_fixed_size_body = quote! { #as_fixed_size_body #i.as_fixed_size_bytes(&mut buf[(#before)..(#after)]); };
                        from_fixed_size_body = quote! { #from_fixed_size_body #i: <#t as ic_stable_memory::AsFixedSizeBytes>::from_fixed_size_bytes(&buf[(#before)..(#after)]), };
                    } else {
                        let idx = Index::from(idx);

                        let val_i = format_ident!("val_{}", idx);
                        enum_header = quote! { #enum_header #val_i, };

                        as_fixed_size_body = quote! { #as_fixed_size_body #val_i.as_fixed_size_bytes(&mut buf[(#before)..(#after)]); };
                        from_fixed_size_body = quote! { #from_fixed_size_body <#t as ic_stable_memory::AsFixedSizeBytes>::from_fixed_size_bytes(&buf[(#before)..(#after)]), };
                    };

                    before = quote! { #after };
                }

                (from_fixed_size_body_total, as_fixed_size_body_total) = match &v.fields {
                    Fields::Unit => {
                        let from = quote! {
                            #from_fixed_size_body_total
                            #v_idx => {
                                Self::#v_name
                            }
                        };

                        let to = quote! {
                            #as_fixed_size_body_total
                            Self::#v_name => {
                                buf[0] = #v_idx;
                                #as_fixed_size_body
                            }
                        };

                        (from, to)
                    }
                    Fields::Named(_) => {
                        let from = quote! {
                            #from_fixed_size_body_total
                            #v_idx => {
                                Self::#v_name { #from_fixed_size_body }
                            }
                        };

                        let to = quote! {
                            #as_fixed_size_body_total
                            Self::#v_name { #enum_header } => {
                                buf[0] = #v_idx;
                                #as_fixed_size_body
                            }
                        };

                        (from, to)
                    }
                    Fields::Unnamed(_) => {
                        let from = quote! {
                            #from_fixed_size_body_total
                            #v_idx => {
                                Self::#v_name( #from_fixed_size_body )
                            }
                        };

                        let to = quote! {
                            #as_fixed_size_body_total
                            Self::#v_name(#enum_header) => {
                                buf[0] = #v_idx;
                                #as_fixed_size_body
                            }
                        };

                        (from, to)
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

            let mut sums = Vec::new();

            for v in &d.variants {
                let mut sizes = Vec::new();

                for f in &v.fields {
                    let t = &f.ty;

                    sizes.push(quote! { <#t as ic_stable_memory::AsFixedSizeBytes>::SIZE });
                }

                if sizes.is_empty() {
                    sums.push(quote! { 0 })
                } else {
                    sums.push(quote! { #(#sizes)+* });
                }
            }

            let size = if sums.is_empty() {
                quote! { <u8 as ic_stable_memory::AsFixedSizeBytes>::SIZE }
            } else if sums.len() == 1 {
                let s = sums.get(0).unwrap();
                quote! { <u8 as ic_stable_memory::AsFixedSizeBytes>::SIZE + #s }
            } else {
                let s1 = sums.get(0).unwrap();
                let mut q = quote! { #s1 };

                for i in 1..sums.len() {
                    let s = sums.get(i).unwrap();
                    q = quote! { ic_stable_memory::utils::math::max_usize(#s, #q) };
                }

                quote! { <u8 as ic_stable_memory::AsFixedSizeBytes>::SIZE + #q }
            };

            (as_fixed_size_body_total, from_fixed_size_body_total, size)
        }
        _ => panic!("Unions not supported!"),
    };

    quote! {
        impl ic_stable_memory::AsFixedSizeBytes for #ident {
            const SIZE: usize = #size;
            type Buf = [u8; Self::SIZE];

            fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
                use ic_stable_memory::AsFixedSizeBytes;

                #as_fixed_size_body
            }

            fn from_fixed_size_bytes(buf: &[u8]) -> Self {
                use ic_stable_memory::AsFixedSizeBytes;

                #from_fixed_size_body
            }
        }
    }
}
