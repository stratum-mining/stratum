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

                quote! {
                    map.insert(
                        stringify!(#field_name).to_string(),
                        self.#field_name.to_json_value(),
                    );
                }
            });

            quote! {
                impl #name {
                    pub fn to_json_value(&self) -> sv2_serde_json::value::Value {
                        let mut map = std::collections::HashMap::new();
                        #(#fields)*
                        sv2_serde_json::value::Value::Object(map)
                    }
                }

                impl ToJsonValue for #name {
                    fn to_json_value(&self) -> sv2_serde_json::value::Value {
                        self.to_json_value()
                    }
                }
            }
        }
        Data::Enum(data) => {
            let variants = data.variants.iter().map(|variant| {
                let var_name = &variant.ident;
                if variant.fields.is_empty() {
                    quote! {
                        #name::#var_name => {
                            let mut map = std::collections::HashMap::new();
                            map.insert(stringify!(#var_name).to_string(), sv2_serde_json::value::Value::Null);
                            sv2_serde_json::value::Value::Object(map)
                        },
                    }
                } else {
                    quote! {
                        #name::#var_name(inner) => {
                            let mut map = std::collections::HashMap::new();
                            map.insert(stringify!(#var_name).to_string(), inner.to_json_value());
                            sv2_serde_json::value::Value::Object(map)
                        },
                    }
                }
            });

            quote! {
                impl #name {
                    pub fn to_json_value(&self) -> sv2_serde_json::value::Value {
                        match self {
                            #(#variants)*
                        }
                    }
                }

                impl ToJsonValue for #name {
                    fn to_json_value(&self) -> sv2_serde_json::value::Value {
                        self.to_json_value()
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

                quote! {
                    #field_name: <#field_ty as FromJsonValue>::from_json_value(
                        &obj.get(stringify!(#field_name)).cloned().ok_or(())?
                    )?,
                }
            });

            quote! {
                impl #name {
                    pub fn from_json_value(value: &sv2_serde_json::value::Value) -> Result<Self, ()> {
                        if let sv2_serde_json::value::Value::Object(obj) = value {
                            Ok(Self {
                                #(#fields)*
                            })
                        } else {
                            Err(())
                        }
                    }
                }

                impl FromJsonValue for #name {
                    fn from_json_value(value: &sv2_serde_json::value::Value) -> Result<Self, ()> {
                        Self::from_json_value(value)
                    }
                }
            }
        }
        Data::Enum(data) => {
            let variants = data.variants.iter().map(|variant| {
                let var_name = &variant.ident;
                if variant.fields.is_empty() {
                    quote! {
                        stringify!(#var_name) => Ok(#name::#var_name),
                    }
                } else {
                    quote! {
                        stringify!(#var_name) => {
                            if let Some(inner_value) = obj.get(stringify!(#var_name)) {
                                let parsed = <_ as sv2_serde_json::value::FromJsonValue>::from_json_value(&inner_value.clone())?;
                                Ok(#name::#var_name(parsed))
                            } else {
                                Err(())
                            }
                        },
                    }
                }
            });

            quote! {
                impl #name {
                    pub fn from_json_value(value: &sv2_serde_json::value::Value) -> Result<Self, ()> {
                        if let sv2_serde_json::value::Value::Object(obj) = value {
                            let variant_name = obj.keys().next().ok_or(())?;
                            match variant_name.as_str() {
                                #(#variants)*
                                _ => Err(()),
                            }
                        } else {
                            Err(())
                        }
                    }
                }

                impl FromJsonValue for #name {
                    fn from_json_value(value: &sv2_serde_json::value::Value) -> Result<Self, ()> {
                        Self::from_json_value(value)
                    }
                }
            }
        }

        _ => unimplemented!("DeJson is only implemented for structs and enums"),
    };

    TokenStream::from(gen)
}
