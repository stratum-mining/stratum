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
use core::convert::{TryFrom, TryInto};

// IMPL GET MARKER FOR PRIMITIVES
impl GetMarker for bool {
    fn get_marker() -> FieldMarker {
        FieldMarker::Primitive(PrimitiveMarker::Bool)
    }
}
impl GetMarker for u8 {
    fn get_marker() -> FieldMarker {
        FieldMarker::Primitive(PrimitiveMarker::U8)
    }
}
impl GetMarker for u16 {
    fn get_marker() -> FieldMarker {
        FieldMarker::Primitive(PrimitiveMarker::U16)
    }
}
impl GetMarker for U24 {
    fn get_marker() -> FieldMarker {
        FieldMarker::Primitive(PrimitiveMarker::U24)
    }
}
impl GetMarker for u32 {
    fn get_marker() -> FieldMarker {
        FieldMarker::Primitive(PrimitiveMarker::U32)
    }
}
impl GetMarker for f32 {
    fn get_marker() -> FieldMarker {
        FieldMarker::Primitive(PrimitiveMarker::F32)
    }
}
impl GetMarker for u64 {
    fn get_marker() -> FieldMarker {
        FieldMarker::Primitive(PrimitiveMarker::U64)
    }
}
impl GetMarker for U256 {
    fn get_marker() -> FieldMarker {
        FieldMarker::Primitive(PrimitiveMarker::U256)
    }
}
impl GetMarker for Signature {
    fn get_marker() -> FieldMarker {
        FieldMarker::Primitive(PrimitiveMarker::Signature)
    }
}
impl GetMarker for B032 {
    fn get_marker() -> FieldMarker {
        FieldMarker::Primitive(PrimitiveMarker::B032)
    }
}
impl GetMarker for B0255 {
    fn get_marker() -> FieldMarker {
        FieldMarker::Primitive(PrimitiveMarker::B0255)
    }
}
impl GetMarker for B064K {
    fn get_marker() -> FieldMarker {
        FieldMarker::Primitive(PrimitiveMarker::B064K)
    }
}
impl GetMarker for B016M {
    fn get_marker() -> FieldMarker {
        FieldMarker::Primitive(PrimitiveMarker::B016M)
    }
}
impl GetMarker for U32AsRef {
    fn get_marker() -> FieldMarker {
        FieldMarker::Primitive(PrimitiveMarker::U32AsRef)
    }
}

// IMPL DECODABLE FOR PRIMITIVES

impl Decodable for u8 {
    fn get_structure(_: &[u8]) -> Result<Vec<FieldMarker>, Error> {
        Ok(vec![PrimitiveMarker::U8.into()])
    }

    fn from_decoded_fields(mut data: Vec<DecodableField>) -> Result<Self, Error> {
        data.pop().ok_or(Error::NoDecodableFieldPassed)?.try_into()
    }
}
impl Decodable for u16 {
    fn get_structure(_: &[u8]) -> Result<Vec<FieldMarker>, Error> {
        Ok(vec![PrimitiveMarker::U16.into()])
    }

    fn from_decoded_fields(mut data: Vec<DecodableField>) -> Result<Self, Error> {
        data.pop().ok_or(Error::NoDecodableFieldPassed)?.try_into()
    }
}
impl Decodable for u32 {
    fn get_structure(_: &[u8]) -> Result<Vec<FieldMarker>, Error> {
        Ok(vec![PrimitiveMarker::U32.into()])
    }

    fn from_decoded_fields(mut data: Vec<DecodableField>) -> Result<Self, Error> {
        data.pop().ok_or(Error::NoDecodableFieldPassed)?.try_into()
    }
}
impl Decodable for f32 {
    fn get_structure(_: &[u8]) -> Result<Vec<FieldMarker>, Error> {
        Ok(vec![PrimitiveMarker::F32.into()])
    }

    fn from_decoded_fields(mut data: Vec<DecodableField>) -> Result<Self, Error> {
        data.pop().ok_or(Error::NoDecodableFieldPassed)?.try_into()
    }
}
impl Decodable for u64 {
    fn get_structure(_: &[u8]) -> Result<Vec<FieldMarker>, Error> {
        Ok(vec![PrimitiveMarker::U64.into()])
    }

    fn from_decoded_fields(mut data: Vec<DecodableField>) -> Result<Self, Error> {
        data.pop().ok_or(Error::NoDecodableFieldPassed)?.try_into()
    }
}
impl Decodable for bool {
    fn get_structure(_: &[u8]) -> Result<Vec<FieldMarker>, Error> {
        Ok(vec![PrimitiveMarker::Bool.into()])
    }

    fn from_decoded_fields(mut data: Vec<DecodableField>) -> Result<Self, Error> {
        data.pop().ok_or(Error::NoDecodableFieldPassed)?.try_into()
    }
}
impl Decodable for U24 {
    fn get_structure(_: &[u8]) -> Result<Vec<FieldMarker>, Error> {
        Ok(vec![PrimitiveMarker::U24.into()])
    }

    fn from_decoded_fields(mut data: Vec<DecodableField>) -> Result<Self, Error> {
        data.pop().ok_or(Error::NoDecodableFieldPassed)?.try_into()
    }
}
impl Decodable for U256 {
    fn get_structure(_: &[u8]) -> Result<Vec<FieldMarker>, Error> {
        Ok(vec![PrimitiveMarker::U256.into()])
    }

    fn from_decoded_fields(mut data: Vec<DecodableField>) -> Result<Self, Error> {
        data.pop().ok_or(Error::NoDecodableFieldPassed)?.try_into()
    }
}
impl Decodable for Signature {
    fn get_structure(_: &[u8]) -> Result<Vec<FieldMarker>, Error> {
        Ok(vec![PrimitiveMarker::Signature.into()])
    }

    fn from_decoded_fields(mut data: Vec<DecodableField>) -> Result<Self, Error> {
        data.pop().ok_or(Error::NoDecodableFieldPassed)?.try_into()
    }
}
impl Decodable for B032 {
    fn get_structure(_: &[u8]) -> Result<Vec<FieldMarker>, Error> {
        Ok(vec![PrimitiveMarker::B032.into()])
    }

    fn from_decoded_fields(mut data: Vec<DecodableField>) -> Result<Self, Error> {
        data.pop().ok_or(Error::NoDecodableFieldPassed)?.try_into()
    }
}
impl Decodable for B0255 {
    fn get_structure(_: &[u8]) -> Result<Vec<FieldMarker>, Error> {
        Ok(vec![PrimitiveMarker::B0255.into()])
    }

    fn from_decoded_fields(mut data: Vec<DecodableField>) -> Result<Self, Error> {
        data.pop().ok_or(Error::NoDecodableFieldPassed)?.try_into()
    }
}
impl Decodable for B064K {
    fn get_structure(_: &[u8]) -> Result<Vec<FieldMarker>, Error> {
        Ok(vec![PrimitiveMarker::B064K.into()])
    }

    fn from_decoded_fields(mut data: Vec<DecodableField>) -> Result<Self, Error> {
        data.pop().ok_or(Error::NoDecodableFieldPassed)?.try_into()
    }
}
impl Decodable for B016M {
    fn get_structure(_: &[u8]) -> Result<Vec<FieldMarker>, Error> {
        Ok(vec![PrimitiveMarker::B016M.into()])
    }

    fn from_decoded_fields(mut data: Vec<DecodableField>) -> Result<Self, Error> {
        data.pop().ok_or(Error::NoDecodableFieldPassed)?.try_into()
    }
}

impl Decodable for U32AsRef {
    fn get_structure(_: &[u8]) -> Result<Vec<FieldMarker>, Error> {
        Ok(vec![PrimitiveMarker::U32AsRef.into()])
    }

    fn from_decoded_fields(mut data: Vec<DecodableField>) -> Result<Self, Error> {
        data.pop().ok_or(Error::NoDecodableFieldPassed)?.try_into()
    }
}

// IMPL TRY_FROM PRIMITIVE FOR PRIMITIVEs

impl TryFrom<DecodablePrimitive> for u8 {
    type Error = Error;

    fn try_from(value: DecodablePrimitive) -> Result<Self, Self::Error> {
        match value {
            DecodablePrimitive::U8(val) => Ok(val),
            _ => Err(Error::PrimitiveConversionError),
        }
    }
}
impl TryFrom<DecodablePrimitive> for u16 {
    type Error = Error;

    fn try_from(value: DecodablePrimitive) -> Result<Self, Self::Error> {
        match value {
            DecodablePrimitive::U16(val) => Ok(val),
            _ => Err(Error::PrimitiveConversionError),
        }
    }
}
impl TryFrom<DecodablePrimitive> for u32 {
    type Error = Error;

    fn try_from(value: DecodablePrimitive) -> Result<Self, Self::Error> {
        match value {
            DecodablePrimitive::U32(val) => Ok(val),
            _ => Err(Error::PrimitiveConversionError),
        }
    }
}
impl TryFrom<DecodablePrimitive> for f32 {
    type Error = Error;

    fn try_from(value: DecodablePrimitive) -> Result<Self, Self::Error> {
        match value {
            DecodablePrimitive::F32(val) => Ok(val),
            _ => Err(Error::PrimitiveConversionError),
        }
    }
}
impl TryFrom<DecodablePrimitive> for u64 {
    type Error = Error;

    fn try_from(value: DecodablePrimitive) -> Result<Self, Self::Error> {
        match value {
            DecodablePrimitive::U64(val) => Ok(val),
            _ => Err(Error::PrimitiveConversionError),
        }
    }
}
impl TryFrom<DecodablePrimitive> for bool {
    type Error = Error;

    fn try_from(value: DecodablePrimitive) -> Result<Self, Self::Error> {
        match value {
            DecodablePrimitive::Bool(val) => Ok(val),
            _ => Err(Error::PrimitiveConversionError),
        }
    }
}

impl TryFrom<DecodablePrimitive> for U24 {
    type Error = Error;

    fn try_from(value: DecodablePrimitive) -> Result<Self, Self::Error> {
        match value {
            DecodablePrimitive::U24(val) => Ok(val),
            _ => Err(Error::PrimitiveConversionError),
        }
    }
}
impl TryFrom<DecodablePrimitive> for U256 {
    type Error = Error;

    fn try_from(value: DecodablePrimitive) -> Result<Self, Self::Error> {
        match value {
            DecodablePrimitive::U256(val) => Ok(val),
            _ => Err(Error::PrimitiveConversionError),
        }
    }
}
impl TryFrom<DecodablePrimitive> for Signature {
    type Error = Error;

    fn try_from(value: DecodablePrimitive) -> Result<Self, Self::Error> {
        match value {
            DecodablePrimitive::Signature(val) => Ok(val),
            _ => Err(Error::PrimitiveConversionError),
        }
    }
}
impl TryFrom<DecodablePrimitive> for B032 {
    type Error = Error;

    fn try_from(value: DecodablePrimitive) -> Result<Self, Self::Error> {
        match value {
            DecodablePrimitive::B032(val) => Ok(val),
            _ => Err(Error::PrimitiveConversionError),
        }
    }
}
impl TryFrom<DecodablePrimitive> for B0255 {
    type Error = Error;

    fn try_from(value: DecodablePrimitive) -> Result<Self, Self::Error> {
        match value {
            DecodablePrimitive::B0255(val) => Ok(val),
            _ => Err(Error::PrimitiveConversionError),
        }
    }
}
impl TryFrom<DecodablePrimitive> for B064K {
    type Error = Error;

    fn try_from(value: DecodablePrimitive) -> Result<Self, Self::Error> {
        match value {
            DecodablePrimitive::B064K(val) => Ok(val),
            _ => Err(Error::PrimitiveConversionError),
        }
    }
}
impl TryFrom<DecodablePrimitive> for B016M {
    type Error = Error;

    fn try_from(value: DecodablePrimitive) -> Result<Self, Self::Error> {
        match value {
            DecodablePrimitive::B016M(val) => Ok(val),
            _ => Err(Error::PrimitiveConversionError),
        }
    }
}
impl TryFrom<DecodablePrimitive> for U32AsRef {
    type Error = Error;

    fn try_from(value: DecodablePrimitive) -> Result<Self, Self::Error> {
        match value {
            DecodablePrimitive::U32AsRef(val) => Ok(val),
            _ => Err(Error::PrimitiveConversionError),
        }
    }
}

// IMPL TRY_FROM DECODEC FIELD FOR PRIMITIVES

impl TryFrom<DecodableField> for u8 {
    type Error = Error;

    fn try_from(value: DecodableField) -> Result<Self, Self::Error> {
        match value {
            DecodableField::Primitive(p) => p.try_into(),
            _ => Err(Error::DecodableConversionError),
        }
    }
}
impl TryFrom<DecodableField> for u16 {
    type Error = Error;

    fn try_from(value: DecodableField) -> Result<Self, Self::Error> {
        match value {
            DecodableField::Primitive(p) => p.try_into(),
            _ => Err(Error::DecodableConversionError),
        }
    }
}
impl TryFrom<DecodableField> for u32 {
    type Error = Error;

    fn try_from(value: DecodableField) -> Result<Self, Self::Error> {
        match value {
            DecodableField::Primitive(p) => p.try_into(),
            _ => Err(Error::DecodableConversionError),
        }
    }
}
impl TryFrom<DecodableField> for f32 {
    type Error = Error;

    fn try_from(value: DecodableField) -> Result<Self, Self::Error> {
        match value {
            DecodableField::Primitive(p) => p.try_into(),
            _ => Err(Error::DecodableConversionError),
        }
    }
}
impl TryFrom<DecodableField> for u64 {
    type Error = Error;

    fn try_from(value: DecodableField) -> Result<Self, Self::Error> {
        match value {
            DecodableField::Primitive(p) => p.try_into(),
            _ => Err(Error::DecodableConversionError),
        }
    }
}
impl TryFrom<DecodableField> for bool {
    type Error = Error;

    fn try_from(value: DecodableField) -> Result<Self, Self::Error> {
        match value {
            DecodableField::Primitive(p) => p.try_into(),
            _ => Err(Error::DecodableConversionError),
        }
    }
}
impl TryFrom<DecodableField> for U24 {
    type Error = Error;

    fn try_from(value: DecodableField) -> Result<Self, Self::Error> {
        match value {
            DecodableField::Primitive(p) => p.try_into(),
            _ => Err(Error::DecodableConversionError),
        }
    }
}
impl TryFrom<DecodableField> for U256 {
    type Error = Error;

    fn try_from(value: DecodableField) -> Result<Self, Self::Error> {
        match value {
            DecodableField::Primitive(p) => p.try_into(),
            _ => Err(Error::DecodableConversionError),
        }
    }
}
impl TryFrom<DecodableField> for Signature {
    type Error = Error;

    fn try_from(value: DecodableField) -> Result<Self, Self::Error> {
        match value {
            DecodableField::Primitive(p) => p.try_into(),
            _ => Err(Error::DecodableConversionError),
        }
    }
}
impl TryFrom<DecodableField> for B032 {
    type Error = Error;

    fn try_from(value: DecodableField) -> Result<Self, Self::Error> {
        match value {
            DecodableField::Primitive(p) => p.try_into(),
            _ => Err(Error::DecodableConversionError),
        }
    }
}
impl TryFrom<DecodableField> for B0255 {
    type Error = Error;

    fn try_from(value: DecodableField) -> Result<Self, Self::Error> {
        match value {
            DecodableField::Primitive(p) => p.try_into(),
            _ => Err(Error::DecodableConversionError),
        }
    }
}
impl TryFrom<DecodableField> for B064K {
    type Error = Error;

    fn try_from(value: DecodableField) -> Result<Self, Self::Error> {
        match value {
            DecodableField::Primitive(p) => p.try_into(),
            _ => Err(Error::DecodableConversionError),
        }
    }
}
impl TryFrom<DecodableField> for B016M {
    type Error = Error;

    fn try_from(value: DecodableField) -> Result<Self, Self::Error> {
        match value {
            DecodableField::Primitive(p) => p.try_into(),
            _ => Err(Error::DecodableConversionError),
        }
    }
}
impl TryFrom<DecodableField> for U32AsRef {
    type Error = Error;

    fn try_from(value: DecodableField) -> Result<Self, Self::Error> {
        match value {
            DecodableField::Primitive(p) => p.try_into(),
            _ => Err(Error::DecodableConversionError),
        }
    }
}

// IMPL FROM PRIMITIVES FOR ENCODED FIELD

impl From<bool> for EncodableField {
    fn from(v: bool) -> Self {
        EncodableField::Primitive(EncodablePrimitive::Bool(v))
    }
}
impl TryFrom<EncodableField> for bool {
    type Error = Error;

    fn try_from(value: EncodableField) -> Result<Self, Self::Error> {
        match value {
            EncodableField::Primitive(EncodablePrimitive::Bool(v)) => Ok(v),
            _ => Err(Error::NonPrimitiveTypeCannotBeEncoded),
        }
    }
}
impl From<u8> for EncodableField {
    fn from(v: u8) -> Self {
        EncodableField::Primitive(EncodablePrimitive::U8(v))
    }
}
impl TryFrom<EncodableField> for u8 {
    type Error = Error;

    fn try_from(value: EncodableField) -> Result<Self, Self::Error> {
        match value {
            EncodableField::Primitive(EncodablePrimitive::U8(v)) => Ok(v),
            _ => Err(Error::NonPrimitiveTypeCannotBeEncoded),
        }
    }
}
impl From<u16> for EncodableField {
    fn from(v: u16) -> Self {
        EncodableField::Primitive(EncodablePrimitive::U16(v))
    }
}
impl TryFrom<EncodableField> for u16 {
    type Error = Error;

    fn try_from(value: EncodableField) -> Result<Self, Self::Error> {
        match value {
            EncodableField::Primitive(EncodablePrimitive::U16(v)) => Ok(v),
            _ => Err(Error::NonPrimitiveTypeCannotBeEncoded),
        }
    }
}
impl From<U24> for EncodableField {
    fn from(v: U24) -> Self {
        EncodableField::Primitive(EncodablePrimitive::U24(v))
    }
}
impl TryFrom<EncodableField> for U24 {
    type Error = Error;

    fn try_from(value: EncodableField) -> Result<Self, Self::Error> {
        match value {
            EncodableField::Primitive(EncodablePrimitive::U24(v)) => Ok(v),
            _ => Err(Error::NonPrimitiveTypeCannotBeEncoded),
        }
    }
}
impl From<u32> for EncodableField {
    fn from(v: u32) -> Self {
        EncodableField::Primitive(EncodablePrimitive::U32(v))
    }
}
impl TryFrom<EncodableField> for u32 {
    type Error = Error;

    fn try_from(value: EncodableField) -> Result<Self, Self::Error> {
        match value {
            EncodableField::Primitive(EncodablePrimitive::U32(v)) => Ok(v),
            _ => Err(Error::NonPrimitiveTypeCannotBeEncoded),
        }
    }
}
impl From<f32> for EncodableField {
    fn from(v: f32) -> Self {
        EncodableField::Primitive(EncodablePrimitive::F32(v))
    }
}
impl TryFrom<EncodableField> for f32 {
    type Error = Error;

    fn try_from(value: EncodableField) -> Result<Self, Self::Error> {
        match value {
            EncodableField::Primitive(EncodablePrimitive::F32(v)) => Ok(v),
            _ => Err(Error::NonPrimitiveTypeCannotBeEncoded),
        }
    }
}
impl From<u64> for EncodableField {
    fn from(v: u64) -> Self {
        EncodableField::Primitive(EncodablePrimitive::U64(v))
    }
}
impl TryFrom<EncodableField> for u64 {
    type Error = Error;

    fn try_from(value: EncodableField) -> Result<Self, Self::Error> {
        match value {
            EncodableField::Primitive(EncodablePrimitive::U64(v)) => Ok(v),
            _ => Err(Error::NonPrimitiveTypeCannotBeEncoded),
        }
    }
}
impl From<U256> for EncodableField {
    fn from(v: U256) -> Self {
        EncodableField::Primitive(EncodablePrimitive::U256(v))
    }
}
impl TryFrom<EncodableField> for U256 {
    type Error = Error;

    fn try_from(value: EncodableField) -> Result<Self, Self::Error> {
        match value {
            EncodableField::Primitive(EncodablePrimitive::U256(v)) => Ok(v),
            _ => Err(Error::NonPrimitiveTypeCannotBeEncoded),
        }
    }
}
impl From<Signature> for EncodableField {
    fn from(v: Signature) -> Self {
        EncodableField::Primitive(EncodablePrimitive::Signature(v))
    }
}
impl TryFrom<EncodableField> for Signature {
    type Error = Error;

    fn try_from(value: EncodableField) -> Result<Self, Self::Error> {
        match value {
            EncodableField::Primitive(EncodablePrimitive::Signature(v)) => Ok(v),
            _ => Err(Error::NonPrimitiveTypeCannotBeEncoded),
        }
    }
}
impl From<B032> for EncodableField {
    fn from(v: B032) -> Self {
        EncodableField::Primitive(EncodablePrimitive::B032(v))
    }
}
impl From<B0255> for EncodableField {
    fn from(v: B0255) -> Self {
        EncodableField::Primitive(EncodablePrimitive::B0255(v))
    }
}
impl TryFrom<EncodableField> for B032 {
    type Error = Error;

    fn try_from(value: EncodableField) -> Result<Self, Self::Error> {
        match value {
            EncodableField::Primitive(EncodablePrimitive::B032(v)) => Ok(v),
            _ => Err(Error::NonPrimitiveTypeCannotBeEncoded),
        }
    }
}
impl TryFrom<EncodableField> for B0255 {
    type Error = Error;

    fn try_from(value: EncodableField) -> Result<Self, Self::Error> {
        match value {
            EncodableField::Primitive(EncodablePrimitive::B0255(v)) => Ok(v),
            _ => Err(Error::NonPrimitiveTypeCannotBeEncoded),
        }
    }
}
impl From<B064K> for EncodableField {
    fn from(v: B064K) -> Self {
        EncodableField::Primitive(EncodablePrimitive::B064K(v))
    }
}
impl TryFrom<EncodableField> for B064K {
    type Error = Error;

    fn try_from(value: EncodableField) -> Result<Self, Self::Error> {
        match value {
            EncodableField::Primitive(EncodablePrimitive::B064K(v)) => Ok(v),
            _ => Err(Error::NonPrimitiveTypeCannotBeEncoded),
        }
    }
}
impl From<B016M> for EncodableField {
    fn from(v: B016M) -> Self {
        EncodableField::Primitive(EncodablePrimitive::B016M(v))
    }
}
impl TryFrom<EncodableField> for B016M {
    type Error = Error;

    fn try_from(value: EncodableField) -> Result<Self, Self::Error> {
        match value {
            EncodableField::Primitive(EncodablePrimitive::B016M(v)) => Ok(v),
            _ => Err(Error::NonPrimitiveTypeCannotBeEncoded),
        }
    }
}
impl From<U32AsRef> for EncodableField {
    fn from(v: U32AsRef) -> Self {
        EncodableField::Primitive(EncodablePrimitive::U32AsRef(v))
    }
}
impl TryFrom<EncodableField> for U32AsRef {
    type Error = Error;

    fn try_from(value: EncodableField) -> Result<Self, Self::Error> {
        match value {
            EncodableField::Primitive(EncodablePrimitive::U32AsRef(v)) => Ok(v),
            _ => Err(Error::NonPrimitiveTypeCannotBeEncoded),
        }
    }
}

// IMPL INTO FIELD MARKER FOR PRIMITIVES
impl From<bool> for FieldMarker {
    fn from(_: bool) -> Self {
        FieldMarker::Primitive(PrimitiveMarker::Bool)
    }
}
impl From<u8> for FieldMarker {
    fn from(_: u8) -> Self {
        FieldMarker::Primitive(PrimitiveMarker::U8)
    }
}

impl From<u16> for FieldMarker {
    fn from(_: u16) -> Self {
        FieldMarker::Primitive(PrimitiveMarker::U16)
    }
}

impl From<u32> for FieldMarker {
    fn from(_: u32) -> Self {
        FieldMarker::Primitive(PrimitiveMarker::U32)
    }
}

impl From<f32> for FieldMarker {
    fn from(_: f32) -> Self {
        FieldMarker::Primitive(PrimitiveMarker::F32)
    }
}

impl From<u64> for FieldMarker {
    fn from(_: u64) -> Self {
        FieldMarker::Primitive(PrimitiveMarker::U64)
    }
}

impl From<U24> for FieldMarker {
    fn from(_: U24) -> Self {
        FieldMarker::Primitive(PrimitiveMarker::U24)
    }
}

impl From<Inner<true, 32, 0, 0>> for FieldMarker {
    fn from(_: Inner<true, 32, 0, 0>) -> Self {
        FieldMarker::Primitive(PrimitiveMarker::U256)
    }
}

impl From<Inner<true, 64, 0, 0>> for FieldMarker {
    fn from(_: Inner<true, 64, 0, 0>) -> Self {
        FieldMarker::Primitive(PrimitiveMarker::Signature)
    }
}

impl From<B032> for FieldMarker {
    fn from(_: B032) -> Self {
        FieldMarker::Primitive(PrimitiveMarker::B032)
    }
}

impl From<Inner<false, 1, 1, 255>> for FieldMarker {
    fn from(_: Inner<false, 1, 1, 255>) -> Self {
        FieldMarker::Primitive(PrimitiveMarker::B0255)
    }
}

impl From<Inner<false, 1, 2, { 2_usize.pow(16) - 1 }>> for FieldMarker {
    fn from(_: Inner<false, 1, 2, { 2_usize.pow(16) - 1 }>) -> Self {
        FieldMarker::Primitive(PrimitiveMarker::B064K)
    }
}

impl From<Inner<false, 1, 3, { 2_usize.pow(24) - 1 }>> for FieldMarker {
    fn from(_: Inner<false, 1, 3, { 2_usize.pow(24) - 1 }>) -> Self {
        FieldMarker::Primitive(PrimitiveMarker::B016M)
    }
}
impl From<U32AsRef> for FieldMarker {
    fn from(_: U32AsRef) -> Self {
        FieldMarker::Primitive(PrimitiveMarker::U32AsRef)
    }
}
