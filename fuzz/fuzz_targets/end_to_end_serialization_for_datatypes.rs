#![no_main]

use arbitrary::Arbitrary;
mod common;
mod generators;

use binary_sv2::{
    Decodable, Encodable, GetSize, PubKey, Seq0255, Seq064K, Signature, Str0255, Sv2Option,
    B016M, B0255, B032, B064K, U24, U256, Mac
};
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
enum FuzzInput {
    // Fixed-size types — raw-bytes mode (parser accepts any N bytes)
    Mac(Vec<u8>),
    PubKey(Vec<u8>),
    Signature(Vec<u8>),
    U256(Vec<u8>),
    U8(Vec<u8>),
    U16(Vec<u8>),
    Bool(Vec<u8>),
    U32(Vec<u8>),
    F32(Vec<u8>),
    U64(Vec<u8>),
    U24(Vec<u8>),


    // Variable-length types — generator mode
    Str0255(Vec<u8>),
    B016M(Vec<u8>),
    B0255(Vec<u8>),
    B032(Vec<u8>),
    B064K(Vec<u8>),


    // Seq0255<T> — generator mode
    Seq0255U8(Vec<u8>),
    Seq0255U16(Vec<u8>),
    Seq0255U32(Vec<u8>),
    Seq0255U64(Vec<u8>),
    Seq0255PubKey(Vec<u8>),
    Seq0255Signature(Vec<u8>),
    Seq0255Str0255(Vec<u8>),
    Seq0255B016M(Vec<u8>),
    Seq0255B0255(Vec<u8>),
    Seq0255B064K(Vec<u8>),
    Seq0255U24(Vec<u8>),
    Seq0255U256(Vec<u8>),


    // Seq064K<T> — generator mode
    Seq064KU8(Vec<u8>),
    Seq064KU16(Vec<u8>),
    Seq064KU32(Vec<u8>),
    Seq064KU64(Vec<u8>),
    Seq064KPubKey(Vec<u8>),
    Seq064KSignature(Vec<u8>),
    Seq064KStr0255(Vec<u8>),
    Seq064KB016M(Vec<u8>),
    Seq064KB0255(Vec<u8>),
    Seq064KB064K(Vec<u8>),
    Seq064KU24(Vec<u8>),
    Seq064KU256(Vec<u8>),


    // Sv2Option<T> — generator mode
    Sv2OptionU8(Vec<u8>),
    Sv2OptionU16(Vec<u8>),
    Sv2OptionU32(Vec<u8>),
    Sv2OptionU64(Vec<u8>),
    Sv2OptionPubKey(Vec<u8>),
    Sv2OptionSignature(Vec<u8>),
    Sv2OptionStr0255(Vec<u8>),
    Sv2OptionB016M(Vec<u8>),
    Sv2OptionB0255(Vec<u8>),
    Sv2OptionB064K(Vec<u8>),
    Sv2OptionU24(Vec<u8>),
    Sv2OptionU256(Vec<u8>),
}

fuzz_target!(|input: FuzzInput| {
    match input {
        // ---- Fixed-size: raw-bytes mode ----
        FuzzInput::Mac(data) => test_datatype_roundtrip!(Mac, data),
        FuzzInput::PubKey(data) => test_datatype_roundtrip!(PubKey, data),
        FuzzInput::Signature(data) => test_datatype_roundtrip!(Signature, data),
        FuzzInput::U256(data) => test_datatype_roundtrip!(U256, data),
        FuzzInput::U8(data) => test_datatype_roundtrip!(u8, data),
        FuzzInput::U16(data) => test_datatype_roundtrip!(u16, data),
        FuzzInput::Bool(data) => test_datatype_roundtrip!(bool, data),
        FuzzInput::U32(data) => test_datatype_roundtrip!(u32, data),
        FuzzInput::F32(data) => test_datatype_roundtrip!(f32, data),
        FuzzInput::U64(data) => test_datatype_roundtrip!(u64, data),
        FuzzInput::U24(data) => test_datatype_roundtrip!(U24, data),

        // ---- Variable-length: generator mode ----
        FuzzInput::Str0255(data) => test_datatype_roundtrip!(Str0255, data, generators::gen_str0255),
        FuzzInput::B016M(data)=> test_datatype_roundtrip!(B016M, data, generators::gen_b016m),
        FuzzInput::B0255(data) => test_datatype_roundtrip!(B0255, data, generators::gen_b0255),
        FuzzInput::B032(data) => test_datatype_roundtrip!(B032, data, generators::gen_b032),
        FuzzInput::B064K(data) => test_datatype_roundtrip!(B064K, data, generators::gen_b064k),

        // ---- Seq0255<T>: generator mode ----
        FuzzInput::Seq0255U8(data) => test_datatype_roundtrip!(Seq0255<u8>, data, |u| generators::gen_seq0255(u, generators::gen_u8, 255)),
        FuzzInput::Seq0255U16(data) => test_datatype_roundtrip!(Seq0255<u16>, data, |u| generators::gen_seq0255(u, generators::gen_u16, 255)),
        FuzzInput::Seq0255U32(data) => test_datatype_roundtrip!(Seq0255<u32>, data, |u| generators::gen_seq0255(u, generators::gen_u32, 255)),
        FuzzInput::Seq0255U64(data) => test_datatype_roundtrip!(Seq0255<u64>, data, |u| generators::gen_seq0255(u, generators::gen_u64, 255)),
        FuzzInput::Seq0255PubKey(data) => test_datatype_roundtrip!(Seq0255<PubKey>, data, |u| generators::gen_seq0255(u, generators::gen_pubkey, 255)),
        FuzzInput::Seq0255Signature(data) => test_datatype_roundtrip!(Seq0255<Signature>, data, |u| generators::gen_seq0255(u, generators::gen_signature, 255)),
        FuzzInput::Seq0255Str0255(data) => test_datatype_roundtrip!(Seq0255<Str0255>, data, |u| generators::gen_seq0255(u, generators::gen_str0255, 64)),
        FuzzInput::Seq0255B016M(data) => test_datatype_roundtrip!(Seq0255<B016M>, data, |u| generators::gen_seq0255(u, generators::gen_b016m, 10)),
        FuzzInput::Seq0255B0255(data) => test_datatype_roundtrip!(Seq0255<B0255>, data, |u| generators::gen_seq0255(u, generators::gen_b0255, 64)),
        FuzzInput::Seq0255B064K(data) => test_datatype_roundtrip!(Seq0255<B064K>, data, |u| generators::gen_seq0255(u, generators::gen_b064k, 10)),
        FuzzInput::Seq0255U24(data) => test_datatype_roundtrip!(Seq0255<U24>, data, |u| generators::gen_seq0255(u, generators::gen_u24, 255)),
        FuzzInput::Seq0255U256(data) => test_datatype_roundtrip!(Seq0255<U256>, data, |u| generators::gen_seq0255(u, generators::gen_u256, 255)),

        // ---- Seq064K<T>: generator mode ----
        FuzzInput::Seq064KU8(data) => test_datatype_roundtrip!(Seq064K<u8>, data, |u| generators::gen_seq064k(u, generators::gen_u8, 256)),
        FuzzInput::Seq064KU16(data) => test_datatype_roundtrip!(Seq064K<u16>, data, |u| generators::gen_seq064k(u, generators::gen_u16, 256)),
        FuzzInput::Seq064KU32(data) => test_datatype_roundtrip!(Seq064K<u32>, data, |u| generators::gen_seq064k(u, generators::gen_u32, 256)),
        FuzzInput::Seq064KU64(data) => test_datatype_roundtrip!(Seq064K<u64>, data, |u| generators::gen_seq064k(u, generators::gen_u64, 256)),
        FuzzInput::Seq064KPubKey(data) => test_datatype_roundtrip!(Seq064K<PubKey>, data, |u| generators::gen_seq064k(u, generators::gen_pubkey, 256)),
        FuzzInput::Seq064KSignature(data) => test_datatype_roundtrip!(Seq064K<Signature>, data, |u| generators::gen_seq064k(u, generators::gen_signature, 256)),
        FuzzInput::Seq064KStr0255(data) => test_datatype_roundtrip!(Seq064K<Str0255>, data, |u| generators::gen_seq064k(u, generators::gen_str0255, 64)),
        FuzzInput::Seq064KB016M(data) => test_datatype_roundtrip!(Seq064K<B016M>, data, |u| generators::gen_seq064k(u, generators::gen_b016m, 10)),
        FuzzInput::Seq064KB0255(data) => test_datatype_roundtrip!(Seq064K<B0255>, data, |u| generators::gen_seq064k(u, generators::gen_b0255, 64)),
        FuzzInput::Seq064KB064K(data) => test_datatype_roundtrip!(Seq064K<B064K>, data, |u| generators::gen_seq064k(u, generators::gen_b064k, 10)),
        FuzzInput::Seq064KU24(data) => test_datatype_roundtrip!(Seq064K<U24>, data, |u| generators::gen_seq064k(u, generators::gen_u24, 256)),
        FuzzInput::Seq064KU256(data) => test_datatype_roundtrip!(Seq064K<U256>, data, |u| generators::gen_seq064k(u, generators::gen_u256, 256)),

        // ---- Sv2Option<T>: generator mode ----
        FuzzInput::Sv2OptionU8(data) => test_datatype_roundtrip!(Sv2Option<u8>, data, |u| generators::gen_sv2_option(u, generators::gen_u8)),
        FuzzInput::Sv2OptionU16(data) => test_datatype_roundtrip!(Sv2Option<u16>, data, |u| generators::gen_sv2_option(u, generators::gen_u16)),
        FuzzInput::Sv2OptionU32(data) => test_datatype_roundtrip!(Sv2Option<u32>, data, |u| generators::gen_sv2_option(u, generators::gen_u32)),
        FuzzInput::Sv2OptionU64(data) => test_datatype_roundtrip!(Sv2Option<u64>, data, |u| generators::gen_sv2_option(u, generators::gen_u64)),
        FuzzInput::Sv2OptionPubKey(data) => test_datatype_roundtrip!(Sv2Option<PubKey>, data, |u| generators::gen_sv2_option(u, generators::gen_pubkey)),
        FuzzInput::Sv2OptionSignature(data) => test_datatype_roundtrip!(Sv2Option<Signature>, data, |u| generators::gen_sv2_option(u, generators::gen_signature)),
        FuzzInput::Sv2OptionStr0255(data) => test_datatype_roundtrip!(Sv2Option<Str0255>, data, |u| generators::gen_sv2_option(u, generators::gen_str0255)),
        FuzzInput::Sv2OptionB016M(data) => test_datatype_roundtrip!(Sv2Option<B016M>, data, |u| generators::gen_sv2_option(u, generators::gen_b016m)),
        FuzzInput::Sv2OptionB0255(data) => test_datatype_roundtrip!(Sv2Option<B0255>, data, |u| generators::gen_sv2_option(u, generators::gen_b0255)),
        FuzzInput::Sv2OptionB064K(data) => test_datatype_roundtrip!(Sv2Option<B064K>, data, |u| generators::gen_sv2_option(u, generators::gen_b064k)),
        FuzzInput::Sv2OptionU24(data) => test_datatype_roundtrip!(Sv2Option<U24>, data, |u| generators::gen_sv2_option(u, generators::gen_u24)),
        FuzzInput::Sv2OptionU256(data) => test_datatype_roundtrip!(Sv2Option<U256>, data, |u| generators::gen_sv2_option(u, generators::gen_u256)),
    }
});
