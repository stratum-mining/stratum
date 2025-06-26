//! Export custom implementations of the `binary_sv2` protocol,
//! enabling encoding and decoding through custom traits.
//!
//! # Overview
//!
//! This crate will re-export implementations of the`Deserialize` and `Serialize` traits
//! from a custom implementation provided by `binary_codec_sv2` and `derive_codec_sv2`.
//! This allows for flexible integration of SV2  protocol types and binary serialization.
//!
//! ## Features
//! - **prop_test**: Adds support for property testing for protocol types.
//! - **with_buffer_pool**: Enables support for buffer pooling to optimize memory usage during
//!   serialization and deserialization.
#![no_std]

#[macro_use]
extern crate alloc;

use core::convert::TryInto;

pub use binary_codec_sv2::{self, Decodable as Deserialize, Encodable as Serialize, *};
pub use derive_codec_sv2::{Decodable as Deserialize, Encodable as Serialize};

/// Does nothing and will be removed during refactor
pub fn clone_message<T: Serialize>(_: T) -> T {
    todo!()
}

/// Converts a value implementing the `Into<u64>` trait into a custom `U256` type.
pub fn u256_from_int<V: Into<u64>>(value: V) -> U256<'static> {
    // initialize u256 as a bytes vec of len 24
    let mut u256 = vec![0_u8; 24];
    let val: u64 = value.into();
    for v in &(val.to_le_bytes()) {
        // add 8 bytes to u256
        u256.push(*v)
    }
    // Always safe cause u256 is 24 + 8 (32) bytes
    let u256: U256 = u256.try_into().unwrap();
    u256
}

#[cfg(test)]
mod test {
    use super::*;
    use alloc::vec::Vec;

    mod test_struct {
        use super::*;
        use core::convert::TryInto;

        #[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
        struct Test {
            a: u32,
            b: u8,
            c: U24,
        }

        #[test]
        fn test_struct() {
            let expected = Test {
                a: 456,
                b: 9,
                c: 67_u32.try_into().unwrap(),
            };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }

    mod test_f32 {
        use super::*;
        use core::convert::TryInto;

        #[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
        struct Test {
            a: u8,
            b: U24,
            c: f32,
        }

        #[test]
        fn test_struct() {
            let expected = Test {
                c: 0.345,
                a: 9,
                b: 67_u32.try_into().unwrap(),
            };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }

    mod test_b0255 {
        use super::*;
        use core::convert::TryInto;

        #[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
        struct Test<'decoder> {
            a: B0255<'decoder>,
        }

        #[test]
        fn test_b0255() {
            let mut b0255 = [6; 3];
            let b0255: B0255 = (&mut b0255[..]).try_into().unwrap();

            let expected = Test { a: b0255 };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }

        #[test]
        fn test_b0255_max() {
            let mut b0255 = [6; 255];
            let b0255: B0255 = (&mut b0255[..]).try_into().unwrap();

            let expected = Test { a: b0255 };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }

    mod test_u256 {
        use super::*;
        use core::convert::TryInto;

        #[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
        struct Test<'decoder> {
            a: U256<'decoder>,
        }

        #[test]
        fn test_u256() {
            let mut u256 = [6_u8; 32];
            let u256: U256 = (&mut u256[..]).try_into().unwrap();

            let expected = Test { a: u256 };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }

    mod test_signature {
        use super::*;
        use core::convert::TryInto;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Signature<'decoder>,
        }

        #[test]
        fn test_signature() {
            let mut s = [6; 64];
            let s: Signature = (&mut s[..]).try_into().unwrap();

            let expected = Test { a: s };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }

    mod test_b016m {
        use super::*;
        use core::convert::TryInto;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            b: bool,
            a: B016M<'decoder>,
        }

        #[test]
        fn test_b016m() {
            let mut b = [0_u8; 70000];
            let b: B016M = (&mut b[..]).try_into().unwrap();
            //println!("{:?}", to_bytes(&b).unwrap().len());

            let expected = Test { a: b, b: true };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }

        #[test]
        fn test_b016m_max() {
            let mut b = vec![0_u8; 16777215];
            let b: B016M = (&mut b[..]).try_into().unwrap();
            //println!("{:?}", to_bytes(&b).unwrap().len());

            let expected = Test { a: b, b: true };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }

    mod test_b064k {
        use super::*;
        use core::convert::TryInto;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            b: bool,
            a: B064K<'decoder>,
        }

        #[test]
        fn test_b064k() {
            let mut b = [1, 2, 9];
            let b: B064K = (&mut b[..])
                .try_into()
                .expect("vector smaller than 64K should not fail");

            let expected = Test { a: b, b: true };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }

    mod test_seq0255_u256 {
        use super::*;
        use core::convert::TryInto;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Seq0255<'decoder, U256<'decoder>>,
        }

        #[test]
        fn test_seq0255_u256() {
            let mut u256_1 = [6; 32];
            let mut u256_2 = [5; 32];
            let mut u256_3 = [0; 32];
            let u256_1: U256 = (&mut u256_1[..]).try_into().unwrap();
            let u256_2: U256 = (&mut u256_2[..]).try_into().unwrap();
            let u256_3: U256 = (&mut u256_3[..]).try_into().unwrap();

            let val = vec![u256_1, u256_2, u256_3];
            let s = Seq0255::new(val).unwrap();

            let test = Test { a: s };

            let mut bytes = to_bytes(test.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            let bytes_2 = to_bytes(deserialized.clone()).unwrap();

            assert_eq!(bytes, bytes_2);
        }
    }

    mod test_0255_bool {
        use super::*;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Seq0255<'decoder, bool>,
        }

        #[test]
        fn test_seq0255_bool() {
            let s: Seq0255<bool> = Seq0255::new(vec![true, false, true]).unwrap();

            let expected = Test { a: s };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }

    mod test_seq0255_u16 {
        use super::*;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Seq0255<'decoder, u16>,
        }

        #[test]
        fn test_seq0255_u16() {
            let s: Seq0255<u16> = Seq0255::new(vec![10, 43, 89]).unwrap();

            let expected = Test { a: s };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }

    mod test_seq_0255_u24 {
        use super::*;
        use core::convert::TryInto;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Seq0255<'decoder, U24>,
        }

        #[test]
        fn test_seq0255_u24() {
            let u24_1: U24 = 56_u32.try_into().unwrap();
            let u24_2: U24 = 59_u32.try_into().unwrap();
            let u24_3: U24 = 70999_u32.try_into().unwrap();

            let val = vec![u24_1, u24_2, u24_3];
            let s: Seq0255<U24> = Seq0255::new(val).unwrap();

            let expected = Test { a: s };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }

    mod test_seqo255_u32 {
        use super::*;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Seq0255<'decoder, u32>,
        }

        #[test]
        fn test_seq0255_u32() {
            let s: Seq0255<u32> = Seq0255::new(vec![546, 99999, 87, 32]).unwrap();

            let expected = Test { a: s };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }

    mod test_seq0255_signature {
        use super::*;
        use core::convert::TryInto;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Seq0255<'decoder, Signature<'decoder>>,
        }

        #[test]
        fn test_seq0255_signature() {
            let mut siganture_1 = [88_u8; 64];
            let mut siganture_2 = [99_u8; 64];
            let mut siganture_3 = [220_u8; 64];
            let siganture_1: Signature = (&mut siganture_1[..]).try_into().unwrap();
            let siganture_2: Signature = (&mut siganture_2[..]).try_into().unwrap();
            let siganture_3: Signature = (&mut siganture_3[..]).try_into().unwrap();

            let val = vec![siganture_1, siganture_2, siganture_3];
            let s: Seq0255<Signature> = Seq0255::new(val).unwrap();

            let expected = Test { a: s };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }

    mod test_seq_064_u256 {
        use super::*;
        use core::convert::TryInto;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Seq064K<'decoder, U256<'decoder>>,
        }

        #[test]
        fn test_seq064k_u256() {
            let mut u256_1 = [6; 32];
            let mut u256_2 = [5; 32];
            let mut u256_3 = [0; 32];
            let u256_1: U256 = (&mut u256_1[..]).try_into().unwrap();
            let u256_2: U256 = (&mut u256_2[..]).try_into().unwrap();
            let u256_3: U256 = (&mut u256_3[..]).try_into().unwrap();

            let val = vec![u256_1, u256_2, u256_3];
            let s = Seq064K::new(val).unwrap();

            let test = Test { a: s };

            let mut bytes = to_bytes(test.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            let bytes_2 = to_bytes(deserialized.clone()).unwrap();

            assert_eq!(bytes, bytes_2);
        }
    }

    mod test_064_bool {
        use super::*;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Seq064K<'decoder, bool>,
        }

        #[test]
        fn test_seq064k_bool() {
            let s: Seq064K<bool> = Seq064K::new(vec![true, false, true]).unwrap();
            let s2: Seq064K<bool> = Seq064K::new(vec![true; 64000]).unwrap();

            let expected = Test { a: s };
            let expected2 = Test { a: s2 };

            let mut bytes = to_bytes(expected.clone()).unwrap();
            let mut bytes2 = to_bytes(expected2.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();
            let deserialized2: Test = from_bytes(&mut bytes2[..]).unwrap();

            assert_eq!(deserialized, expected);
            assert_eq!(deserialized2, expected2);
        }
    }

    mod test_se1o64k_u16 {
        use super::*;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Seq064K<'decoder, u16>,
        }

        #[test]
        fn test_seq064k_u16() {
            let s: Seq064K<u16> = Seq064K::new(vec![10, 43, 89]).unwrap();

            let expected = Test { a: s };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }

    mod test_seq064k_u24 {
        use super::*;
        use core::convert::TryInto;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Seq064K<'decoder, U24>,
        }

        #[test]
        fn test_seq064k_u24() {
            let u24_1: U24 = 56_u32.try_into().unwrap();
            let u24_2: U24 = 59_u32.try_into().unwrap();
            let u24_3: U24 = 70999_u32.try_into().unwrap();

            let val = vec![u24_1, u24_2, u24_3];
            let s: Seq064K<U24> = Seq064K::new(val).unwrap();

            let expected = Test { a: s };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }

    mod test_seq064k_u32 {
        use super::*;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Seq064K<'decoder, u32>,
        }

        #[test]
        fn test_seq064k_u32() {
            let s: Seq064K<u32> = Seq064K::new(vec![546, 99999, 87, 32]).unwrap();

            let expected = Test { a: s };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }
    mod test_seq064k_signature {
        use super::*;
        use core::convert::TryInto;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Seq064K<'decoder, Signature<'decoder>>,
        }

        #[test]
        fn test_seq064k_signature() {
            let mut siganture_1 = [88_u8; 64];
            let mut siganture_2 = [99_u8; 64];
            let mut siganture_3 = [220_u8; 64];
            let siganture_1: Signature = (&mut siganture_1[..]).try_into().unwrap();
            let siganture_2: Signature = (&mut siganture_2[..]).try_into().unwrap();
            let siganture_3: Signature = (&mut siganture_3[..]).try_into().unwrap();

            let val = vec![siganture_1, siganture_2, siganture_3];
            let s: Seq064K<Signature> = Seq064K::new(val).unwrap();

            let expected = Test { a: s };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }
    mod test_seq064k_b016m {
        use super::*;
        use core::convert::TryInto;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Seq064K<'decoder, B016M<'decoder>>,
        }

        #[test]
        fn test_seq064k_b016m() {
            let mut bytes_1 = [88_u8; 64];
            let mut bytes_2 = [99_u8; 64];
            let mut bytes_3 = [220_u8; 64];
            let bytes_1: B016M = (&mut bytes_1[..]).try_into().unwrap();
            let bytes_2: B016M = (&mut bytes_2[..]).try_into().unwrap();
            let bytes_3: B016M = (&mut bytes_3[..]).try_into().unwrap();

            let val = vec![bytes_1, bytes_2, bytes_3];
            let s: Seq064K<B016M> = Seq064K::new(val).unwrap();

            let expected = Test { a: s };

            let mut bytes = to_bytes(expected.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            assert_eq!(deserialized, expected);
        }
    }
    mod test_seq_0255_in_struct {
        use super::*;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: u8,
            b: Seq0255<'decoder, u8>,
            c: u32,
        }

        #[test]
        fn test_seq_0255_in_struct() {
            let expected = Test {
                a: 89,
                b: Seq0255::new(vec![]).unwrap(),
                c: 32,
            };
            let len = expected.get_size();
            let mut buffer = vec![0; len];
            to_writer(expected.clone(), &mut buffer).unwrap();
            let deserialized: Test = from_bytes(&mut buffer[..]).unwrap();
            assert_eq!(deserialized, expected);
        }
    }
    mod test_sv2_option_u256 {
        use super::*;
        use core::convert::TryInto;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Sv2Option<'decoder, U256<'decoder>>,
        }

        #[test]
        fn test_sv2_option_u256() {
            let mut u256_1 = [6; 32];
            let u256_1: U256 = (&mut u256_1[..]).try_into().unwrap();

            let val = Some(u256_1);
            let s = Sv2Option::new(val);

            let test = Test { a: s };

            let mut bytes = to_bytes(test.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            let bytes_2 = to_bytes(deserialized.clone()).unwrap();

            assert_eq!(bytes, bytes_2);
        }
    }
    mod test_sv2_option_none {
        use super::*;

        #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
        struct Test<'decoder> {
            a: Sv2Option<'decoder, U256<'decoder>>,
        }

        #[test]
        fn test_sv2_option_none() {
            let val = None;
            let s = Sv2Option::new(val);

            let test = Test { a: s };

            let mut bytes = to_bytes(test.clone()).unwrap();

            let deserialized: Test = from_bytes(&mut bytes[..]).unwrap();

            let bytes_2 = to_bytes(deserialized.clone()).unwrap();

            assert_eq!(bytes, bytes_2);
        }
    }
}
