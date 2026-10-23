use binary_sv2::*;

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
    fn rejects_invalid_values_only_when_serializing() {
        for bits in [
            0x8000_0000, // negative zero
            0xbf80_0000, // -1
            0x8000_0001, // negative subnormal
            0xff7f_ffff, // most negative finite value
            0x7f80_0000, // positive infinity
            0xff80_0000, // negative infinity
            0x7fc0_0000, // quiet NaN
            0x7f80_0001, // signaling NaN
            0xffc0_0001, // negative NaN with payload
        ] {
            let value = f32::from_bits(bits);
            let mut dst = [0xaa; 4];
            assert_eq!(to_writer(value, &mut dst), Err(Error::InvalidF32(bits)));
            assert_eq!(dst, [0xaa; 4]);
            assert_eq!(
                to_bytes(Test {
                    a: 9,
                    b: 67_u32.try_into().unwrap(),
                    c: value
                }),
                Err(Error::InvalidF32(bits))
            );

            let mut bytes = value.to_le_bytes();
            let decoded: f32 = from_bytes(&mut bytes).unwrap();
            assert_eq!(decoded.to_bits(), bits);

            let mut message = [9, 67, 0, 0, 0, 0, 0, 0];
            message[4..].copy_from_slice(&bytes);
            let decoded: Test = from_bytes(&mut message).unwrap();
            assert_eq!(decoded.c.to_bits(), bits);
        }
    }

    #[test]
    fn preserves_valid_encodings_and_buffer_bounds() {
        for value in [0.0, f32::from_bits(1), f32::MIN_POSITIVE, 1.0, f32::MAX] {
            let mut dst = [0xaa; 5];
            assert_eq!(to_writer(value, &mut dst), Ok(4));
            assert_eq!(&dst[..4], &value.to_le_bytes());
            assert_eq!(dst[4], 0xaa);
            let decoded: f32 = from_bytes(&mut dst[..4]).unwrap();
            assert_eq!(decoded.to_bits(), value.to_bits());
        }
        let mut short = [0xaa; 3];
        assert_eq!(to_writer(1.0_f32, &mut short), Err(Error::WriteError(4, 3)));
        assert_eq!(short, [0xaa; 3]);
        assert!(from_bytes::<f32>(&mut short).is_err());
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

mod test_owned_fields_in_struct {
    use super::*;
    use core::convert::TryInto;

    #[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
    struct Test<'decoder> {
        borrowed: B0255<'decoder>,
        owned: U256Owned,
        owned_seq: Seq0255Owned<U256Owned>,
    }

    #[test]
    fn test_owned_fields_are_preserved_in_owned_variant() {
        let mut borrowed = [1_u8, 2, 3];
        let borrowed: B0255 = (&mut borrowed[..]).try_into().unwrap();
        let owned = U256Owned::from([4_u8; 32]);
        let owned_seq = Seq0255Owned::new(vec![U256Owned::from([5_u8; 32])]).unwrap();

        let expected = Test {
            borrowed,
            owned: owned.clone(),
            owned_seq: owned_seq.clone(),
        };

        let expected_owned = expected.as_owned();

        assert_eq!(expected_owned.borrowed.as_ref(), expected.borrowed.as_ref());
        assert_eq!(expected_owned.owned, owned);
        assert_eq!(expected_owned.owned_seq, owned_seq);

        let expected_bytes = to_bytes(expected).unwrap();
        let owned_bytes = to_bytes(expected_owned).unwrap();

        assert_eq!(owned_bytes, expected_bytes);
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

mod test_b032_oversize {
    use super::*;

    // B032 has MAXSIZE=32 and HEADERSIZE=1. A header byte of 33 declares a
    // 33-byte payload, which decoding must reject.
    #[test]
    fn b032_from_bytes_should_reject_oversize_payload() {
        let mut buf = vec![33u8];
        buf.extend(vec![0u8; 33]);
        let result = B032::from_bytes_(&mut buf[..]);
        assert!(
            matches!(
                result,
                Err(Error::ValueExceedsMaxSize(false, 1, 1, 32, _, 33))
            ),
            "B032::from_bytes_ must reject a 33-byte payload (MAXSIZE = 32)"
        );
    }
}

mod test_fixed_primitive_from_bytes_truncated {
    use super::*;

    // Regression test for the blanket `impl<T: Fixed> SizeHint for T` in
    // `src/codec/mod.rs`. `Sv2DataType::from_bytes_` is the documented *checked*
    // constructor: it calls `Self::size_hint(data, 0)?` to validate the buffer
    // before delegating to the panicking `from_bytes_unchecked`. For fixed-size
    // copy primitives (u8/u16/u32/u64/f32/U24/bool) `size_hint` ignores `data`
    // and unconditionally returns `Ok(Self::SIZE)`, so a truncated buffer is
    // never rejected. `from_bytes_unchecked` then slices `data[0..SIZE]` and
    // panics ("range end index out of range") instead of returning `Err`.
    #[test]
    fn u16_from_bytes_truncated_must_be_err_not_panic() {
        // u16 has SIZE == 2; provide only 1 byte.
        let mut data = [0u8; 1];
        let res = <u16 as Sv2DataType>::from_bytes_(&mut data[..]);
        assert!(
            res.is_err(),
            "u16::from_bytes_ must reject a 1-byte buffer (SIZE = 2) with Err, not panic"
        );
    }
}

mod test_b064k_length_prefix {
    use super::*;
    use core::convert::TryInto;

    #[test]
    fn b064k_to_bytes_includes_length_prefix() {
        let mut payload = [1u8, 2, 3];
        let b: B064K = (&mut payload[..]).try_into().unwrap();

        let via_bytes = to_bytes(b).unwrap();

        assert_eq!(
            via_bytes,
            vec![3u8, 0, 1, 2, 3],
            "to_bytes must emit the 2-byte length prefix followed by the payload"
        );
    }
}

mod test_to_writer_len {
    use super::*;

    #[test]
    fn to_writer_reports_bytes_written_into_oversized_buffer() {
        let mut oversized = [0xAAu8; 16];

        let written = to_writer(0x0102_0304_u32, &mut oversized[..]).unwrap();

        assert_eq!(written, 4);
        assert_eq!(&oversized[..written], &[0x04, 0x03, 0x02, 0x01]);
        assert!(oversized[written..].iter().all(|b| *b == 0xAA));
    }
}

mod test_fixed_size_hint_overflow {
    use super::*;

    struct HugeFixed;

    impl Fixed for HugeFixed {
        const SIZE: usize = usize::MAX;
    }

    #[test]
    fn unrepresentable_fixed_end_offset_does_not_wrap() {
        let data = [0u8; 1];
        match <HugeFixed as SizeHint>::size_hint(&data, data.len()) {
            Err(Error::ReadError(actual, required)) => assert!(required >= actual),
            other => panic!("expected ReadError, got {other:?}"),
        }
    }
}

mod test_deep_encodable_field {
    use super::*;
    use binary_sv2::encodable::EncodablePrimitive;

    fn nested(depth: usize) -> EncodableField<'static> {
        let mut field = EncodableField::Primitive(EncodablePrimitive::U8(0x5a));
        for _ in 0..depth {
            field = EncodableField::Struct(vec![field]);
        }
        field
    }

    #[test]
    fn deeply_nested_field_encodes_sizes_and_drops_without_recursion() {
        let field = nested(100_000);
        let mut dst = [0u8; 1];

        assert_eq!(field.get_size(), 1);
        assert_eq!(field.encode(&mut dst, 0), Ok(1));
        assert_eq!(dst, [0x5a]);
        drop(field);
    }

    #[test]
    fn encode_reports_offset_past_buffer_end() {
        let field = EncodableField::Struct(vec![
            EncodableField::Primitive(EncodablePrimitive::U8(1)),
            EncodableField::Primitive(EncodablePrimitive::U8(2)),
        ]);
        let mut dst = [0u8; 1];

        assert_eq!(field.encode(&mut dst, 0), Err(Error::WriteError(1, 0)));
        assert_eq!(field.encode(&mut dst, 2), Err(Error::WriteError(2, 1)));

        let empty = EncodableField::Struct(vec![]);
        assert_eq!(empty.encode(&mut dst, 1), Ok(0));
        assert_eq!(empty.encode(&mut dst, 2), Err(Error::WriteError(2, 1)));
    }
}

mod test_seq_decode_ast {
    use super::*;
    use binary_sv2::decodable::FieldMarker;

    #[derive(Deserialize)]
    struct ExtensionRequest<'decoder> {
        request_id: u16,
        requested_extensions: Seq064K<'decoder, u16>,
    }

    #[test]
    fn nested_seq064k_is_not_expanded_into_per_element_markers() {
        let count = u16::MAX as usize;
        let mut bytes = vec![0_u8; 2 + 2 + count * 2];
        bytes[2..4].copy_from_slice(&u16::MAX.to_le_bytes());

        let structure = ExtensionRequest::get_structure(&bytes).unwrap();
        assert_eq!(structure.len(), 2);
        assert!(matches!(structure[1], FieldMarker::Raw(size) if size == bytes.len() - 2));

        let decoded: ExtensionRequest = from_bytes(&mut bytes).unwrap();
        assert_eq!(decoded.request_id, 0);
        assert_eq!(decoded.requested_extensions.len(), count);
    }
}

mod test_owned_visibility {
    macro_rules! define_plain {
        ($struct_vis:vis $name:ident, $field_vis:vis $field:ident) => {
            #[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
            $struct_vis struct $name<'decoder> {
                $field_vis $field: B0255<'decoder>,
                pub tail: u32,
            }
        };
    }

    macro_rules! define_documented {
        ($struct_vis:vis $name:ident, $field_vis:vis $field:ident) => {
            /// Struct docs.
            #[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
            $struct_vis struct $name<'decoder> {
                /// Field docs.
                $field_vis $field: B0255<'decoder>,
                pub tail: u32,
            }
        };
    }

    mod message {
        use binary_sv2::*;

        #[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
        pub(crate) struct Message<'decoder> {
            pub public: u32,
            pub(super) restricted: u16,
            private: B0255<'decoder>,
        }

        impl<'decoder> Message<'decoder> {
            pub fn new(public: u32, restricted: u16, private: B0255<'decoder>) -> Self {
                Self {
                    public,
                    restricted,
                    private,
                }
            }
        }

        impl MessageOwned {
            pub fn private(&self) -> &B0255Owned {
                &self.private
            }
        }

        define_plain!(pub Plain, pub shown);
        define_documented!(pub Documented, pub shown);
        define_plain!(pub(crate) Hidden, hidden);

        impl<'decoder> Hidden<'decoder> {
            pub fn new(hidden: B0255<'decoder>, tail: u32) -> Self {
                Self { hidden, tail }
            }
        }

        impl HiddenOwned {
            pub fn hidden(&self) -> &B0255Owned {
                &self.hidden
            }
        }
    }

    use core::convert::TryInto;
    use message::{Documented, Hidden, HiddenOwned, Message, MessageOwned, Plain};

    #[test]
    fn test_owned_struct_keeps_source_visibility() {
        let mut private = [1_u8, 2, 3];
        let message = Message::new(7, 9, (&mut private[..]).try_into().unwrap());

        let owned: MessageOwned = message.into_owned();

        assert_eq!(owned.public, 7);
        assert_eq!(owned.restricted, 9);
        assert_eq!(owned.private().as_ref(), &[1_u8, 2, 3]);
    }

    #[test]
    fn test_owned_struct_keeps_macro_generated_visibility() {
        let mut shown = [4_u8, 5];
        let plain = Plain {
            shown: (&mut shown[..]).try_into().unwrap(),
            tail: 11,
        };
        let owned = plain.into_owned();
        assert_eq!(owned.shown.as_ref(), &[4_u8, 5]);
        assert_eq!(owned.tail, 11);

        let mut shown = [6_u8];
        let documented = Documented {
            shown: (&mut shown[..]).try_into().unwrap(),
            tail: 12,
        };
        let owned = documented.into_owned();
        assert_eq!(owned.shown.as_ref(), &[6_u8]);
        assert_eq!(owned.tail, 12);
    }

    #[test]
    fn test_owned_struct_keeps_macro_generated_private_field() {
        let mut hidden = [7_u8, 8, 9];
        let message = Hidden::new((&mut hidden[..]).try_into().unwrap(), 13);

        let owned: HiddenOwned = message.into_owned();

        assert_eq!(owned.hidden().as_ref(), &[7_u8, 8, 9]);
        assert_eq!(owned.tail, 13);
    }
}
