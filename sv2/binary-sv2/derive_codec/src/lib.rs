//! # Procedural Macros for Automatic Serialization and Deserialization
//!
//! Provides procedural macros for deriving serialization and deserialization
//! traits on structs used in binary protocol communication. The macros `Encodable` and `Decodable`
//! generate implementations of encoding and decoding behaviors, making it simpler to work with
//! binary data by automatically handling field parsing, sizing, and transformation.
//!
//! ## Overview
//!
//! These macros parse struct definitions to produce code that supports efficient and type-safe
//! serialization and deserialization. Each field within a struct is processed based on its type and
//! associated generics, allowing for custom encoding schemes and alignment with protocol
//! requirements. Additionally, the macros enable flexible handling of lifetimes and static
//! references to ensure compatibility across different use cases.
//!
//! ## Available Macros
//!
//! - **`Encodable`**: Automatically implements encoding logic, converting a struct's fields into a
//!   binary format.
//!   - **Attributes**: `#[already_sized]` (optional) to specify that a struct's size is fixed at
//!     compile-time.
//!   - **Generated Traits**: `EncodableField` (field-by-field encoding) and `GetSize` (size
//!     calculation).
//!
//! - **`Decodable`**: Automatically implements decoding logic, allowing a struct to be
//!   reconstructed from binary data.
//!   - **Generated Methods**: `get_structure` (defines field structure) and `from_decoded_fields`
//!     (builds the struct from decoded fields).
//!
//! ## Internal Structure
//!
//! ### `is_already_sized`
//! Checks if the `#[already_sized]` attribute is present on the struct, allowing certain
//! optimizations in generated code for fixed-size structs.
//!
//! ### `get_struct_properties`
//! Parses and captures a struct’s name, generics, and field data, enabling custom encoding and
//! decoding functionality.
//!
//! ### Custom Implementations
//! The `Encodable` macro generates an `EncodableField` implementation by serializing each field,
//! while `Decodable` constructs the struct from binary data. Both macros provide support for
//! structs with or without lifetimes, ensuring versatility in applications that require efficient,
//! protocol-level data handling.

#![no_std]

extern crate alloc;
extern crate proc_macro;

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::iter::FromIterator;
use proc_macro::{Group, TokenStream, TokenTree};

// Reserved field names to avoid conflicts
const RESERVED_FIELDS: [&str; 2] = ["__decodable_internal_data", "__decodable_internal_offset"];

// Checks if a `TokenStream` contains a group with a bracket delimiter (`[]`),
// and further examines if the group has an identifier called `already_sized`.
//
// Iterates through the `TokenStream`, searching for a group of tokens
// that is delimited by square brackets (`[]`). Once a group is found, it looks inside
// the group for an identifier named `already_sized`. If such an identifier is found,
// the function returns `true`. Otherwise, it returns `false`.
//
// # Example
//
// ```ignore
// use proc_macro2::{Delimiter, Group, Ident, Span, TokenStream, TokenTree};
// use quote::quote;
//
// let input: TokenStream = quote! {
//     [already_sized]
// };
//
// // Call the function to check if `already_sized` is present inside the bracket group.
// assert_eq!(is_already_sized(input), true);
//
// // Now let's try another TokenStream that doesn't contain the `already_sized` identifier.
// let input_without_already_sized: TokenStream = quote! {
//     [some_other_ident]
// };
//
// assert_eq!(is_already_sized(input_without_already_sized), false);
// ```
//
// In this example, the function successfully detects the presence of the `already_sized`
// identifier when it's wrapped inside brackets, and returns `true` accordingly.
fn is_already_sized(item: TokenStream) -> bool {
    let stream = item.into_iter();

    for next in stream {
        if let TokenTree::Group(g) = next.clone() {
            if g.delimiter() == proc_macro::Delimiter::Bracket {
                for t in g.stream().into_iter() {
                    if let TokenTree::Ident(i) = t {
                        if i.to_string() == "already_sized" {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

// Filters out attributes from a `TokenStream` that are prefixed with `#`.
//
// Removes all Rust attributes (e.g., `#[derive(...)]`, `#[cfg(...)]`)
// from the provided `TokenStream`, leaving behind only the core structure and
// its fields. This is useful in procedural macros when you want to manipulate
// the underlying data without the attributes.
//
// # Example
//
// ```ignore
// use proc_macro2::{Delimiter, Group, TokenStream, TokenTree};
// use quote::quote;
//
// let input: TokenStream = quote! {
//     #[derive(Debug, Clone)]
//     pub struct MyStruct {
//         pub field1: i32,
//         #[cfg(feature = "extra")]
//         pub field2: String,
//     }
// };
//
// let cleaned: TokenStream = remove_attributes(input);
//
// let expected_output: TokenStream = quote! {
//     pub struct MyStruct {
//         pub field1: i32,
//         pub field2: String,
//     }
// };
//
// assert_eq!(cleaned.to_string(), expected_output.to_string());
// ```
//
// In this example, the `#[derive(Debug, Clone)]` and `#[cfg(feature = "extra")]`
// attributes were removed, leaving just the plain `struct` with its fields.
fn remove_attributes(item: TokenStream) -> TokenStream {
    let stream = item.into_iter();
    let mut is_attribute = false;
    let mut result = Vec::new();

    for next in stream {
        match next.clone() {
            TokenTree::Punct(p) => {
                if p.to_string() == "#" {
                    is_attribute = true;
                } else {
                    result.push(next.clone());
                }
            }
            TokenTree::Group(g) => {
                if is_attribute {
                    continue;
                } else {
                    let delimiter = g.delimiter();
                    let cleaned_group = remove_attributes(g.stream());
                    let cleaned_group = TokenTree::Group(Group::new(delimiter, cleaned_group));
                    result.push(cleaned_group);
                }
            }
            _ => {
                is_attribute = false;
                result.push(next.clone());
            }
        }
    }

    TokenStream::from_iter(result)
}

// Represents the current state of the parser while processing a struct.
enum ParserState {
    // Indicates that the parser is processing the struct's name.
    Name,
    // Indicates that the parser is processing the struct's type.
    Type,
    // Indicates that the parser is inside the angle brackets for generics.
    //
    // The `usize` value represents the depth of nested generics
    Generics(usize),
}

// Represents a parsed struct, including its name, generics, and fields.
//
// # Examples
//
// ```ignore
// struct MyStruct<T> {
//     pub field1: i32,
//     pub field2: String,
// }
//
// let parsed = ParsedStruct {
//     name: "MyStruct".to_string(),
//     generics: "T".to_string(),
//     fields: vec![
//         ParsedField {
//             name: "field1".to_string(),
//             type_: "i32".to_string(),
//             generics: "".to_string(),
//         },
//         ParsedField {
//             name: "field2".to_string(),
//             type_: "String".to_string(),
//             generics: "".to_string(),
//         },
//     ],
// };
// ```
#[derive(Clone, Debug)]
struct ParsedStruct {
    // Name of the struct.
    pub name: String,
    // Generics associated with the struct, if any.
    pub generics: String,
    // List of fields within the struct.
    pub fields: Vec<ParsedField>,
}

// Represents a parsed field within a struct, including its name, type, and any associated generics.
//
// # Examples
//
// ```ignore
// // Given a struct field definition:
// // data: Option<T>,
//
// let field = ParsedField {
//     name: "data".to_string(),
//     type_: "Option".to_string(),
//     generics: "T".to_string(),
// };
// ```
#[derive(Clone, Debug)]
struct ParsedField {
    // Name of the field.
    name: String,
    // Type of the field.
    type_: String,
    // Generics associated with the field, if any.
    generics: String,
}

impl ParsedField {
    pub fn new() -> Self {
        ParsedField {
            name: "".to_string(),
            type_: "".to_string(),
            generics: "".to_string(),
        }
    }

    pub fn get_generics(&self) -> String {
        if self.generics == "<'decoder>" || self.generics.is_empty() {
            "".to_string()
        } else {
            format!("::{}", self.generics.clone())
        }
    }
    pub fn as_static(&self) -> String {
        if self.generics.is_empty() {
            "".to_string()
        } else {
            ".into_static()".to_string()
        }
    }
}

// Extracts properties of a struct, including its name, generics, and fields.
//
// Processes a token stream, filtering out attributes and extracting
// core components such as the struct's name, generics, and fields. It expects the
// token stream to represent a struct declaration.
//
// # Examples
//
// ```ignore
// use quote::quote;
//
// struct MyStruct<T> {
//     field1: i32,
// }
//
// let tokens = quote! {struct MyStream<T> { field1: i32 }};
// let parsed_struct = get_struct_properties(tokens);
//
// assert_eq!(parsed_struct.name, "MyStruct");
// assert_eq!(parsed_struct.generics, "T");
// assert_eq!(parsed_struct.fields.len(), 1);
// assert_eq!(parsed_struct.fields[0].name, "field1");
// assert_eq!(parsed_struct.fields[0].type_, "i32");
// ```
//
// This example demonstrates how `get_struct_properties` identifies the struct's
// name, any generic parameters, and its fields, including types and generic parameters.
fn get_struct_properties(item: TokenStream) -> ParsedStruct {
    let item = remove_attributes(item);
    let mut stream = item.into_iter();

    // Check if the stream is a struct
    loop {
        match stream.next().expect("Stream not a struct") {
            TokenTree::Ident(i) => {
                if i.to_string() == "struct" {
                    break;
                }
            }
            _ => continue,
        }
    }

    // Get the struct name
    let struct_name = match stream.next().expect("Struct has no name") {
        TokenTree::Ident(i) => i.to_string(),
        // Never executed at runtime it ok to panic
        _ => panic!("Strcut has no name"),
    };

    let mut struct_generics = "".to_string();
    let group: Vec<TokenTree>;

    // Get the struct generics if any
    loop {
        match stream
            .next()
            // Never executed at runtime it ok to panic
            .unwrap_or_else(|| panic!("Struct {} has no fields", struct_name))
        {
            TokenTree::Group(g) => {
                group = g.stream().into_iter().collect();
                break;
            }
            TokenTree::Punct(p) => {
                struct_generics = format!("{struct_generics}{p}");
            }
            TokenTree::Ident(i) => {
                struct_generics = format!("{struct_generics}{i}");
            }
            // Never executed at runtime it ok to panic
            _ => panic!("Struct {} has no fields", struct_name),
        };
    }

    let fields = parse_struct_fields(group);

    ParsedStruct {
        name: struct_name,
        generics: struct_generics,
        fields,
    }
}

// Parses the fields of a struct, scanning tokens to identify field names, types, and generics.
//
// Processes tokens for each field in a struct, managing parser states
// (`ParserState::Name`, `ParserState::Type`, and `ParserState::Generics`) to accurately parse
// complex types and nested generics within struct definitions.
//
// # Examples
//
// ```ignore
// struct MyStruct<T> {
//     field1: i32,
// }
//
// let tokens = vec![/* TokenTree representing `id: i32` */];
// let parsed_fields = parse_struct_fields(tokens);
//
// assert_eq!(parsed_fields.len(), 1);
// assert_eq!(parsed_fields[0].name, "field1");
// assert_eq!(parsed_fields[0].type_, "i32");
// assert_eq!(parsed_fields[0].generics, "");
// ```
//
// This example shows how `parse_struct_fields` handles both a primitive field (`id`) and a
// generic field (`data`) within a struct, including parsing of generic parameters.
fn parse_struct_fields(group: Vec<TokenTree>) -> Vec<ParsedField> {
    let mut fields = Vec::new();
    let mut field_ = ParsedField::new();
    let mut field_parser_state = ParserState::Name;
    for token in group {
        match (token, &field_parser_state) {
            (TokenTree::Ident(i), ParserState::Name) => {
                if i.to_string() == "pub" {
                    continue;
                } else {
                    field_.name = i.to_string();
                }
            }
            (TokenTree::Ident(i), ParserState::Type) => {
                field_.type_ = i.to_string();
            }
            (TokenTree::Ident(i), ParserState::Generics(_)) => {
                field_.generics = format!("{}{}", field_.generics, i);
            }
            (TokenTree::Punct(p), ParserState::Name) => {
                if p.to_string() == ":" {
                    field_parser_state = ParserState::Type
                } else {
                    // Never executed at runtime it ok to panic
                    panic!("Unexpected token '{}' in parsing {:#?}", p, field_);
                }
            }
            (TokenTree::Punct(p), ParserState::Type) => match p.to_string().as_ref() {
                "," => {
                    field_parser_state = ParserState::Name;
                    fields.push(field_.clone());
                    field_ = ParsedField::new();
                }
                "<" => {
                    field_.generics = "<".to_string();
                    field_parser_state = ParserState::Generics(0);
                }
                // Never executed at runtime it ok to panic
                _ => panic!("Unexpected token '{}' in parsing {:#?}", p, field_),
            },
            (TokenTree::Punct(p), ParserState::Generics(open_brackets)) => {
                match p.to_string().as_ref() {
                    "'" => {
                        field_.generics = format!("{}{}", field_.generics, p);
                    }
                    "<" => {
                        field_.generics = format!("{}{}", field_.generics, p);
                        field_parser_state = ParserState::Generics(open_brackets + 1);
                    }
                    ">" => {
                        field_.generics = format!("{}{}", field_.generics, p);
                        if open_brackets == &0 {
                            field_parser_state = ParserState::Type
                        } else {
                            field_parser_state = ParserState::Generics(open_brackets - 1);
                        }
                    }
                    _ => {
                        field_.generics = format!("{}{}", field_.generics, p);
                    }
                }
            }
            // Never executed at runtime it ok to panic
            _ => panic!("Unexpected token"),
        }
    }
    fields
}

/// Derives the `Decodable` trait, generating implementations for deserializing a struct from a
/// byte stream, including its structure, field decoding, and a method for creating a static
/// version.
///
/// This procedural macro generates the `Decodable` trait for a struct, which allows it to be
/// decoded from a binary stream, with support for handling fields of different types and
/// nested generics. The macro also includes implementations for `from_decoded_fields`,
/// `get_structure`, and methods to return a static version of the struct.
///
/// # Example
///
/// Given a struct:
///
/// ```ignore
/// struct Test {
///     a: u32,
///     b: u8,
///     c: U24,
/// }
/// ```
///
/// Using `#[derive(Decodable)]` on `Test` generates the following implementations:
///
/// ```ignore
/// mod impl_parse_decodable_test {
///     use super::{
///         binary_codec_sv2::{
///             decodable::{DecodableField, FieldMarker},
///             Decodable, Error, SizeHint,
///         },
///         *,
///     };
///
///     struct Test {
///         a: u32,
///         b: u8,
///         c: U24,
///     }
///
///     impl<'decoder> Decodable<'decoder> for Test {
///         fn get_structure(__decodable_internal_data: &[u8]) -> Result<Vec<FieldMarker>, Error> {
///             let mut fields = Vec::new();
///             let mut __decodable_internal_offset = 0;
///
///             let a: Vec<FieldMarker> = u32::get_structure(&__decodable_internal_data[__decodable_internal_offset..])?;
///             __decodable_internal_offset += a.size_hint_(&__decodable_internal_data, __decodable_internal_offset)?;
///             let a = a.try_into()?;
///             fields.push(a);
///
///             let b: Vec<FieldMarker> = u8::get_structure(&__decodable_internal_data[__decodable_internal_offset..])?;
///             __decodable_internal_offset += b.size_hint_(&__decodable_internal_data, __decodable_internal_offset)?;
///             let b = b.try_into()?;
///             fields.push(b);
///
///             let c: Vec<FieldMarker> = U24::get_structure(&__decodable_internal_data[__decodable_internal_offset..])?;
///             __decodable_internal_offset += c.size_hint_(&__decodable_internal_data, __decodable_internal_offset)?;
///             let c = c.try_into()?;
///             fields.push(c);
///
///             Ok(fields)
///         }
///
///         fn from_decoded_fields(mut __decodable_internal_data: Vec<DecodableField<'decoder>>) -> Result<Self, Error> {
///             Ok(Self {
///                 c: U24::from_decoded_fields(
///                     __decodable_internal_data.pop().ok_or(Error::NoDecodableFieldPassed)?.into(),
///                 )?,
///                 b: u8::from_decoded_fields(
///                     __decodable_internal_data.pop().ok_or(Error::NoDecodableFieldPassed)?.into(),
///                 )?,
///                 a: u32::from_decoded_fields(
///                     __decodable_internal_data.pop().ok_or(Error::NoDecodableFieldPassed)?.into(),
///                 )?,
///             })
///         }
///     }
///
///     impl Test {
///         pub fn into_static(self) -> Test {
///             Test {
///                 a: self.a.clone(),
///                 b: self.b.clone(),
///                 c: self.c.clone(),
///             }
///         }
///     }
///
///     impl Test {
///         pub fn as_static(&self) -> Test {
///             Test {
///                 a: self.a.clone(),
///                 b: self.b.clone(),
///                 c: self.c.clone(),
///             }
///         }
///     }
/// }
/// ```
///
/// This generated code enables `Test` to be decoded from a binary stream, defines how each
/// field should be parsed, and provides `into_static` and `as_static` methods to facilitate
/// ownership and lifetime management of decoded fields in the struct.
#[proc_macro_derive(Decodable)]
pub fn decodable(item: TokenStream) -> TokenStream {
    let parsed_struct = get_struct_properties(item);

    let data_ident = RESERVED_FIELDS[0];
    let offset_ident = RESERVED_FIELDS[1];

    for field in &parsed_struct.fields {
        if RESERVED_FIELDS.contains(&field.name.as_str()) {
            return format!(
                "compile_error!(\"Field name '{}' is reserved and cannot be used in struct '{}'. Rename it to avoid conflicts.\");",
                field.name, parsed_struct.name
            )
            .parse()
            .unwrap();
        }
    }

    let mut derive_fields = String::new();

    for f in parsed_struct.fields.clone() {
        let field = format!(
            "
            let {}: Vec<FieldMarker> = {}{}::get_structure(& {}[{}..])?;
            {} += {}.size_hint_(&{}, {})?;
            let {} =  {}.try_into()?;
            fields.push({});
            ",
            f.name,
            f.type_,
            f.get_generics(),
            data_ident,
            offset_ident,
            offset_ident,
            f.name,
            data_ident,
            offset_ident,
            f.name,
            f.name,
            f.name
        );
        derive_fields.push_str(&field)
    }

    let mut derive_static_fields = String::new();
    for f in parsed_struct.fields.clone() {
        let field = format!(
            "
            {}: self.{}.clone(){},
            ",
            f.name,
            f.name,
            f.as_static(),
        );
        derive_static_fields.push_str(&field)
    }

    let mut derive_decoded_fields = String::new();
    let mut fields = parsed_struct.fields.clone();

    // Reverse the fields as they are popped out from the end of the vector but we want to pop out
    // from the front
    fields.reverse();

    // Create Struct from fields
    for f in fields.clone() {
        let field = format!(
            "
            {}: {}{}::from_decoded_fields({}.pop().ok_or(Error::NoDecodableFieldPassed)?.into())?,
            ",
            f.name,
            f.type_,
            f.get_generics(),
            data_ident
        );
        derive_decoded_fields.push_str(&field)
    }

    let impl_generics = if !parsed_struct.generics.is_empty() {
        parsed_struct.clone().generics
    } else {
        "<'decoder>".to_string()
    };

    let result = format!(
        "mod impl_parse_decodable_{} {{

    use super::binary_codec_sv2::{{decodable::DecodableField, decodable::FieldMarker, Decodable, Error, SizeHint}};
    use super::*;

    impl{} Decodable<'decoder> for {}{} {{
        fn get_structure({}: &[u8]) -> Result<Vec<FieldMarker>, Error> {{
            let mut fields = Vec::new();
            let mut {} = 0;
            {}
            Ok(fields)
        }}

        fn from_decoded_fields(mut {}: Vec<DecodableField<'decoder>>) -> Result<Self, Error> {{
            Ok(Self {{
                {}
            }})
        }}
    }}

    impl{} {}{} {{
        pub fn into_static(self) -> {}{} {{
            {} {{
                {}
            }}
        }}
    }}
    impl{} {}{} {{
        pub fn as_static(&self) -> {}{} {{
            {} {{
                {}
            }}
        }}
    }}
    }}",
        // imports
        parsed_struct.name.to_lowercase(),
        // derive decodable
        impl_generics,
        parsed_struct.name,
        parsed_struct.generics,
        data_ident,
        offset_ident,
        derive_fields,
        data_ident,
        derive_decoded_fields,
        // impl into_static
        impl_generics,
        parsed_struct.name,
        parsed_struct.generics,
        parsed_struct.name,
        get_static_generics(&parsed_struct.generics),
        parsed_struct.name,
        derive_static_fields,
        // impl into_static
        impl_generics,
        parsed_struct.name,
        parsed_struct.generics,
        parsed_struct.name,
        get_static_generics(&parsed_struct.generics),
        parsed_struct.name,
        derive_static_fields,

    );

    // Never executed at runtime it ok to panic
    result.parse().unwrap()
}

fn get_static_generics(gen: &str) -> &str {
    if gen.is_empty() {
        gen
    } else {
        "<'static>"
    }
}

/// Derives the `Encodable` trait, generating implementations for serializing a struct into an
/// encoded format, including methods for field serialization, calculating the encoded size,
/// and handling cases where the struct is already sized.
///
/// This procedural macro generates the `Encodable` trait for a struct, allowing each field
/// to be converted into an `EncodableField`, with support for recursive field encoding.
/// The macro also includes an implementation of the `GetSize` trait to calculate the
/// encoded size of the struct, depending on the `already_sized` attribute.
///
/// # Example
///
/// Given a struct:
///
/// ```ignore
/// struct Test {
///     a: u32,
///     b: u8,
///     c: U24,
/// }
/// ```
///
/// Using `#[derive(Encodable)]` on `Test` generates the following implementations:
///
/// ```ignore
/// mod impl_parse_encodable_test {
///     use super::binary_codec_sv2::{encodable::EncodableField, GetSize};
///     extern crate alloc;
///     use alloc::vec::Vec;
///
///     struct Test {
///         a: u32,
///         b: u8,
///         c: U24,
///     }
///
///     impl<'decoder> From<Test> for EncodableField<'decoder> {
///         fn from(v: Test) -> Self {
///             let mut fields: Vec<EncodableField> = Vec::new();
///
///             let val = v.a;
///             fields.push(val.into());
///
///             let val = v.b;
///             fields.push(val.into());
///
///             let val = v.c;
///             fields.push(val.into());
///
///             Self::Struct(fields)
///         }
///     }
///
///     impl<'decoder> GetSize for Test {
///         fn get_size(&self) -> usize {
///             let mut size = 0;
///
///             size += self.a.get_size();
///             size += self.b.get_size();
///             size += self.c.get_size();
///
///             size
///         }
///     }
/// }
/// ```
///
/// This generated code enables `Test` to be serialized into an encoded format, defines
/// how each field should be converted, and calculates the total encoded size of the struct,
/// depending on whether it is marked as `already_sized`.
#[proc_macro_derive(Encodable, attributes(already_sized))]
pub fn encodable(item: TokenStream) -> TokenStream {
    let is_already_sized = is_already_sized(item.clone());
    let parsed_struct = get_struct_properties(item);
    let fields = parsed_struct.fields.clone();

    let mut field_into_decoded_field = String::new();

    // Create DecodableField from fields
    for f in fields.clone() {
        let field = format!(
            "
            let val = v.{};
            fields.push(val.into());
            ",
            f.name
        );
        field_into_decoded_field.push_str(&field)
    }

    let mut sizes = String::new();

    for f in fields {
        let field = format!(
            "
            size += self.{}.get_size();
            ",
            f.name
        );
        sizes.push_str(&field)
    }
    let impl_generics = if !parsed_struct.generics.is_empty() {
        parsed_struct.clone().generics
    } else {
        "<'decoder>".to_string()
    };

    let get_size = if is_already_sized {
        String::new()
    } else {
        format!(
            "
            impl{} GetSize for {}{} {{
                fn get_size(&self) -> usize {{
                    let mut size = 0;
                    {}
                    size
                }}
            }}
            ",
            impl_generics, parsed_struct.name, parsed_struct.generics, sizes
        )
    };

    let result = format!(
        "mod impl_parse_encodable_{} {{

    use super::binary_codec_sv2::{{encodable::EncodableField, GetSize}};
    use super::{};
    extern crate alloc;
    use alloc::vec::Vec;

    impl{} From<{}{}> for EncodableField<'decoder> {{
        fn from(v: {}{}) -> Self {{
            let mut fields: Vec<EncodableField> = Vec::new();
            {}
            Self::Struct(fields)
        }}
    }}

    {}

    }}",
        // imports
        parsed_struct.name.to_lowercase(),
        parsed_struct.name,
        // impl From<Struct> for DecodableField
        impl_generics,
        parsed_struct.name,
        parsed_struct.generics,
        parsed_struct.name,
        parsed_struct.generics,
        field_into_decoded_field,
        // impl get_size
        get_size,
    );
    //println!("{}", result);

    // Never executed at runtime it ok to panic
    result.parse().unwrap()
}
