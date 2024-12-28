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
                let value_conversion = match field_ty {
                    syn::Type::Path(path) => {
                        let type_ident = &path.path.segments.last().unwrap().ident;
                        match type_ident.to_string().as_str() {
                            "String" => quote! { Value::String(self.#field_name.clone()) },
                            "i64"|"u32"|"u16"|"u8"|"i32"|"i16"|"i8" => quote! { Value::Number(Number::I64(self.#field_name)) },
                            "f64"|"f32" => quote! { Value::Number(Number::F64(self.#field_name)) },
                            "bool" => quote! { Value::Boolean(self.#field_name) },
                            _ => quote! { self.#field_name.to_json_value() },
                        }
                    }
                    _ => quote! { self.#field_name.to_json_value() },
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
pub fn de_json_derive(input:TokenStream) -> TokenStream {
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
                    #field_name: obj.get(stringify!(#field_name))
                        .ok_or(())?
                        .try_into()
                        .map_err(|_| ())?,
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
            }  
        }
        _ => unimplemented!("DeJson is only implemented for struct and enums")
    
    };

    TokenStream::from(gen)
}