use crate::{
    codec::{
        decodable::{
            Decodable, DecodableField, DecodablePrimitive, FieldMarker, GetMarker, PrimitiveMarker,
        },
        encodable::{EncodableField, EncodablePrimitive},
    },
    datatypes::*,
    Error,
};
use alloc::vec::Vec;
use core::convert::TryInto;

macro_rules! impl_get_marker {
    ($(($ty:ty, $marker:ident)),+ $(,)?) => {
        $(
            impl GetMarker for $ty {
                fn get_marker() -> FieldMarker {
                    FieldMarker::Primitive(PrimitiveMarker::$marker)
                }
            }
        )+
    };
}

macro_rules! impl_decodable {
    ($(($ty:ty, $marker:ident)),+ $(,)?) => {
        $(
            impl<'a> Decodable<'a> for $ty {
                fn get_structure(_: &[u8]) -> Result<Vec<FieldMarker>, Error> {
                    Ok(vec![PrimitiveMarker::$marker.into()])
                }

                fn from_decoded_fields(
                    mut data: Vec<DecodableField<'a>>,
                ) -> Result<Self, Error> {
                    data.pop().ok_or(Error::NoDecodableFieldPassed)?.try_into()
                }
            }
        )+
    };
}

macro_rules! impl_try_from_decodable_primitive {
    ($(($ty:ty, $variant:ident)),+ $(,)?) => {
        $(
            impl<'a> TryFrom<DecodablePrimitive<'a>> for $ty {
                type Error = Error;

                fn try_from(value: DecodablePrimitive<'a>) -> Result<Self, Self::Error> {
                    match value {
                        DecodablePrimitive::$variant(val) => Ok(val),
                        _ => Err(Error::PrimitiveConversionError),
                    }
                }
            }
        )+
    };
}

macro_rules! impl_try_from_decodable_field {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl<'a> TryFrom<DecodableField<'a>> for $ty {
                type Error = Error;

                fn try_from(value: DecodableField<'a>) -> Result<Self, Self::Error> {
                    match value {
                        DecodableField::Primitive(p) => p.try_into(),
                        _ => Err(Error::DecodableConversionError),
                    }
                }
            }
        )+
    };
}

macro_rules! impl_encodable_field_conversion {
    ($(($ty:ty, $variant:ident)),+ $(,)?) => {
        $(
            impl<'a> From<$ty> for EncodableField<'a> {
                fn from(v: $ty) -> Self {
                    EncodableField::Primitive(EncodablePrimitive::$variant(v))
                }
            }
        )+
    };
}

macro_rules! impl_field_marker_from_owned {
    ($(($ty:ty, $marker:ident)),+ $(,)?) => {
        $(
            impl From<$ty> for FieldMarker {
                fn from(_: $ty) -> Self {
                    FieldMarker::Primitive(PrimitiveMarker::$marker)
                }
            }
        )+
    };
}

macro_rules! impl_field_marker_from_borrowed {
    ($(($ty:ty, $marker:ident)),+ $(,)?) => {
        $(
            impl<'a> From<$ty> for FieldMarker {
                fn from(_: $ty) -> Self {
                    FieldMarker::Primitive(PrimitiveMarker::$marker)
                }
            }
        )+
    };
}

impl_get_marker!(
    (bool, Bool),
    (u8, U8),
    (u16, U16),
    (U24, U24),
    (u32, U32),
    (f32, F32),
    (u64, U64),
    (U256<'_>, U256),
    (Mac<'_>, Mac),
    (Signature<'_>, Signature),
    (B032<'_>, B032),
    (B0255<'_>, B0255),
    (B064K<'_>, B064K),
    (B016M<'_>, B016M),
);

impl_decodable!(
    (u8, U8),
    (u16, U16),
    (u32, U32),
    (f32, F32),
    (u64, U64),
    (bool, Bool),
    (U24, U24),
    (U256<'a>, U256),
    (Mac<'a>, Mac),
    (Signature<'a>, Signature),
    (B032<'a>, B032),
    (B0255<'a>, B0255),
    (B064K<'a>, B064K),
    (B016M<'a>, B016M),
);

impl_try_from_decodable_primitive!(
    (u8, U8),
    (u16, U16),
    (u32, U32),
    (f32, F32),
    (u64, U64),
    (bool, Bool),
    (U24, U24),
    (U256<'a>, U256),
    (Mac<'a>, Mac),
    (Signature<'a>, Signature),
    (B032<'a>, B032),
    (B0255<'a>, B0255),
    (B064K<'a>, B064K),
    (B016M<'a>, B016M),
);

impl_try_from_decodable_field!(
    u8,
    u16,
    u32,
    f32,
    u64,
    bool,
    U24,
    U256<'a>,
    Mac<'a>,
    Signature<'a>,
    B032<'a>,
    B0255<'a>,
    B064K<'a>,
    B016M<'a>,
);

impl_encodable_field_conversion!(
    (bool, Bool),
    (u8, U8),
    (u16, U16),
    (U24, U24),
    (u32, U32),
    (f32, F32),
    (u64, U64),
    (U256<'a>, U256),
    (Mac<'a>, Mac),
    (Signature<'a>, Signature),
    (B032<'a>, B032),
    (B0255<'a>, B0255),
    (B064K<'a>, B064K),
    (B016M<'a>, B016M),
);

impl_field_marker_from_owned!(
    (bool, Bool),
    (u8, U8),
    (u16, U16),
    (u32, U32),
    (f32, F32),
    (u64, U64),
    (U24, U24),
);

impl_field_marker_from_borrowed!(
    (Inner<'a, true, 16, 0, 0>, Mac),
    (Inner<'a, true, 32, 0, 0>, U256),
    (Inner<'a, true, 64, 0, 0>, Signature),
    (B032<'a>, B032),
    (Inner<'a, false, 1, 1, 255>, B0255),
    (Inner<'a, false, 1, 2, { 2_usize.pow(16) - 1 }>, B064K),
    (Inner<'a, false, 1, 3, { 2_usize.pow(24) - 1 }>, B016M),
);
