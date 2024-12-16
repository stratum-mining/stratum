pub use binary_codec_sv2::{self, Decodable as Deserialize, Encodable as Serialize, *};
use core::convert::TryInto;
pub use derive_codec_sv2::{Decodable as Deserialize, Encodable as Serialize};

// The `Test` struct is expanded using the `Deserialize` and `Serialize` procedural macros.
// These macros provide the necessary methods for serializing and deserializing the struct.
//
// mod impl_parse_decodable_test {
//     use super::binary_codec_sv2::{
//         decodable::DecodableField, decodable::FieldMarker, Decodable, Error, SizeHint,
//     };
//     use super::*;
//     impl<'decoder> Decodable<'decoder> for Test {
//         fn get_structure(data: &[u8]) -> Result<Vec<FieldMarker>, Error> {
//             let mut fields = Vec::new();
//             let mut offset = 0;
//             let a: Vec<FieldMarker> = u32::get_structure(&data[offset..])?;
//             offset += a.size_hint_(&data, offset)?;
//             let a = a.try_into()?;
//             fields.push(a);
//             let b: Vec<FieldMarker> = u8::get_structure(&data[offset..])?;
//             offset += b.size_hint_(&data, offset)?;
//             let b = b.try_into()?;
//             fields.push(b);
//             let c: Vec<FieldMarker> = U24::get_structure(&data[offset..])?;
//             offset += c.size_hint_(&data, offset)?;
//             let c = c.try_into()?;
//             fields.push(c);
//             Ok(fields)
//         }
//         fn from_decoded_fields(
//             mut data: Vec<DecodableField<'decoder>>,
//         ) -> Result<Self, Error> {
//             Ok(Self {
//                 c: U24::from_decoded_fields(
//                     data.pop().ok_or(Error::NoDecodableFieldPassed)?.into(),
//                 )?,
//                 b: u8::from_decoded_fields(
//                     data.pop().ok_or(Error::NoDecodableFieldPassed)?.into(),
//                 )?,
//                 a: u32::from_decoded_fields(
//                     data.pop().ok_or(Error::NoDecodableFieldPassed)?.into(),
//                 )?,
//             })
//         }
//     }
//     impl<'decoder> Test {
//         pub fn into_static(self) -> Test {
//             Test {
//                 a: self.a.clone(),
//                 b: self.b.clone(),
//                 c: self.c.clone(),
//             }
//         }
//     }
//     impl<'decoder> Test {
//         pub fn as_static(&self) -> Test {
//             Test {
//                 a: self.a.clone(),
//                 b: self.b.clone(),
//                 c: self.c.clone(),
//             }
//         }
//     }
// }
// mod impl_parse_encodable_test {
//     use super::binary_codec_sv2::{encodable::EncodableField, GetSize};
//     use super::Test;
//     extern crate alloc;
//     use alloc::vec::Vec;
//     impl<'decoder> From<Test> for EncodableField<'decoder> {
//         fn from(v: Test) -> Self {
//             let mut fields: Vec<EncodableField> = Vec::new();
//             let val = v.a;
//             fields.push(val.into());
//             let val = v.b;
//             fields.push(val.into());
//             let val = v.c;
//             fields.push(val.into());
//             Self::Struct(fields)
//         }
//     }
//     impl<'decoder> GetSize for Test {
//         fn get_size(&self) -> usize {
//             let mut size = 0;
//             size += self.a.get_size();
//             size += self.b.get_size();
//             size += self.c.get_size();
//             size
//         }
//     }
// }
//

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
struct Test {
    a: u32,
    b: u8,
    c: U24,
}

fn main() {
    let expected = Test {
        a: 456,
        b: 9,
        c: 67_u32.try_into().unwrap(),
    };

    // `to_bytes` serves as the entry point to the `binary_sv2` crate. It acts as a serializer that
    // converts the struct into bytes.
    let mut bytes = to_bytes(expected.clone()).unwrap();

    // `from_bytes` is a deserializer that interprets the bytes and reconstructs the original
    // struct.
    let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

    assert_eq!(deserialized, expected);
}
