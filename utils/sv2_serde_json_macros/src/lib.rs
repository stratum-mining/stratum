extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput};

#[proc_macro_derive(SerJson)]
pub fn ser_json_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let gen = match &input.data {
        Data::Struct(data) => {
            let fields = data.fields.iter().map(|field| {
                let field_name = field
                    .ident
                    .as_ref()
                    .expect("Expected named fields in the struct");
                let field_ty = &field.ty;

                // Handle standard types explicitly
                let value_conversion = if let syn::Type::Path(path) = field_ty {
                    let type_ident = &path.path.segments.last().unwrap().ident;
                    if type_ident == "Vec" {
                        // Handle Vec types
                        if let syn::PathArguments::AngleBracketed(args) =
                            &path.path.segments.last().unwrap().arguments
                        {
                            if let Some(syn::GenericArgument::Type(syn::Type::Path(inner_path))) =
                                args.args.first()
                            {
                                let inner_ident =
                                    &inner_path.path.segments.last().unwrap().ident;
                                match inner_ident.to_string().as_str() {
                                    "String" => quote! {
                                        Value::Array(self.#field_name.iter().map(|v| Value::String(v.clone())).collect())
                                    },
                                    "i64" | "u32" | "u16" | "u8" | "i32" | "i16" | "i8" => quote! {
                                        Value::Array(self.#field_name.iter().map(|&v| Value::Number(Number::I64(v as i64))).collect())
                                    },
                                    "f64" | "f32" => quote! {
                                        Value::Array(self.#field_name.iter().map(|&v| Value::Number(Number::F64(v as f64))).collect())
                                    },
                                    "bool" => quote! {
                                        Value::Array(self.#field_name.iter().map(|&v| Value::Boolean(v)).collect())
                                    },
                                    _ => quote! {
                                        Value::Array(self.#field_name.iter().map(|v| v.to_json_value()).collect())
                                    },
                                }
                            } else {
                                quote! { Value::Array(vec![]) } // Default fallback
                            }
                        } else {
                            quote! { Value::Array(vec![]) } // Default fallback
                        }
                    } else {
                        // Handle primitive and other types
                        match type_ident.to_string().as_str() {
                            "String" => quote! { Value::String(self.#field_name.clone()) },
                            "i64" | "u32" | "u16" | "u8" | "i32" | "i16" | "i8" => {
                                quote! { Value::Number(Number::I64(self.#field_name as i64)) }
                            }
                            "f64" | "f32" => quote! { Value::Number(Number::F64(self.#field_name as f64)) },
                            "bool" => quote! { Value::Boolean(self.#field_name) },
                            _ => quote! { self.#field_name.to_json_value() },
                        }
                    }
                } else {
                    quote! { self.#field_name.to_json_value() }
                };

                quote! {
                    map.insert(
                        stringify!(#field_name).to_string(),
                        #value_conversion,
                    );
                }
            });

            quote! {
                impl #name {
                    pub fn to_json_value(&self) -> Value {
                        let mut map = std::collections::HashMap::new();
                        #(#fields)*
                        Value::Object(map)
                    }
                }
            }
        }
        Data::Enum(data) => {
            let variants = data.variants.iter().map(|variant| {
                let var_name = &variant.ident;

                // Handle enum variants as string representations
                quote! {
                    #name::#var_name => Value::String(stringify!(#var_name).to_string()),
                }
            });

            quote! {
                impl #name {
                    pub fn to_json_value(&self) -> Value {
                        match self {
                            #(#variants)*
                        }
                    }
                }
            }
        }
        _ => unimplemented!("SerJson is only implemented for structs and enums"),
    };

    TokenStream::from(gen)
}

#[proc_macro_derive(DeJson)]
pub fn de_json_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let gen = match &input.data {
        Data::Struct(data) => {
            let fields = data.fields.iter().map(|field| {
                let field_name = field
                    .ident
                    .as_ref()
                    .expect("Expected named fields in the struct");
                let field_ty = &field.ty;

                // Determine if the field is a primitive or custom type
                match field_ty {
                    syn::Type::Path(_) => quote! {
                        #field_name: <#field_ty as TryFrom<&Value>>::try_from(
                            obj.get(stringify!(#field_name)).ok_or(())?
                        ).map_err(|_| ())?,
                    },
                    _ => quote! {
                        #field_name: obj.get(stringify!(#field_name))
                            .ok_or(())?
                            .try_into()
                            .map_err(|_| ())?,
                    },
                }

            });

            quote! {
                impl #name {
                    pub fn from_json_value(value: Value) -> Result<Self, ()> {
                        if let Value::Object(obj) = value {
                            Ok(Self {
                                #(#fields)*
                            })
                        } else {
                            Err(())
                        }
                    }
                }

                impl TryFrom<&Value> for #name {
                    type Error = ();
                    fn try_from(value: &Value) -> Result<Self, Self::Error> {
                        if let Value::Object(obj) = value {
                            Self::from_json_value(value.clone())
                        } else {
                            Err(())
                        }
                    }
                }
            }
        }
        Data::Enum(data) => {
            let variants = data.variants.iter().map(|variant| {
                let var_name = &variant.ident;
                quote! {
                    stringify!(#var_name) => Ok(#name::#var_name),
                }
            });

            quote! {
                impl #name {
                    pub fn from_json_value(value: Value) -> Result<Self, ()> {
                        if let Value::String(s) = value {
                            match s.as_str() {
                                #(#variants)*
                                _ => Err(()),
                            }
                        } else {
                            Err(())
                        }
                    }
                }

                // impl TryFrom<&Value> for #name {
                //     type Error = ();
                //     fn try_from(value: &Value) -> Result<Self, Self::Error> {
                //         value.clone().try_into()
                //     }
                // }
            }
        }
        _ => unimplemented!("DeJson is only implemented for structs and enums"),
    };

    TokenStream::from(gen)
}
