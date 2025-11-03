#![no_main]
use arbitrary::Arbitrary;
use binary_sv2::{
    from_bytes, Encodable, GetSize, PubKey, Signature, Str0255, U32AsRef, B016M, B0255, B032,
    B064K, U24, U256,
};
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
struct FuzzInput<'a> {
    // Let fuzzer choose which type to test
    type_selector: u8,
    // Raw data for parsing
    data: &'a [u8],
}

fuzz_target!(|input: FuzzInput| {
    let data = input.data;

    match input.type_selector % 16 {
        0 => {
            let mut data1 = data.to_vec();
            if let Ok(value) = from_bytes::<u8>(&mut data1) {
                let mut encoded = vec![0x0; value.get_size()];
                value.clone().to_bytes(&mut encoded).unwrap();
                let value1: u8 = from_bytes(&mut encoded).unwrap();

                assert_eq!(value, value1);
            }
        }
        1 => {
            let mut data1 = data.to_vec();
            if let Ok(value) = from_bytes::<u16>(&mut data1) {
                let mut encoded = vec![0x0; value.get_size()];
                value.clone().to_bytes(&mut encoded).unwrap();
                let value1: u16 = from_bytes(&mut encoded).unwrap();

                assert_eq!(value, value1);
            }
        }
        2 => {
            let mut data1 = data.to_vec();
            if let Ok(u24) = from_bytes::<U24>(&mut data1) {
                let mut encoded = vec![0x0; u24.get_size()];
                u24.clone().to_bytes(&mut encoded).unwrap();
                let u24_1: U24 = from_bytes(&mut encoded).unwrap();

                assert_eq!(u24, u24_1);
            }
        }
        3 => {
            let mut data1 = data.to_vec();
            if let Ok(value) = from_bytes::<bool>(&mut data1) {
                let mut encoded = vec![0x0; value.get_size()];
                value.clone().to_bytes(&mut encoded).unwrap();
                let value1: bool = from_bytes(&mut encoded).unwrap();

                assert_eq!(value, value1);
            }
        }
        4 => {
            let mut data1 = data.to_vec();
            if let Ok(value) = from_bytes::<u32>(&mut data1) {
                let mut encoded = vec![0x0; value.get_size()];
                value.clone().to_bytes(&mut encoded).unwrap();
                let value1: u32 = from_bytes(&mut encoded).unwrap();

                assert_eq!(value, value1);
            }
        }
        5 => {
            let mut data1 = data.to_vec();
            if let Ok(ur32asref) = from_bytes::<U32AsRef>(&mut data1) {
                let mut encoded = vec![0x0; ur32asref.get_size()];
                ur32asref.clone().to_bytes(&mut encoded).unwrap();
                let ur32asref1: U32AsRef = from_bytes(&mut encoded).unwrap();

                assert_eq!(ur32asref, ur32asref1);
            }
        }
        6 => {
            let mut data1 = data.to_vec();
            if let Ok(value) = from_bytes::<f32>(&mut data1) {
                let mut encoded = vec![0x0; value.get_size()];
                value.clone().to_bytes(&mut encoded).unwrap();
                let value1: f32 = from_bytes(&mut encoded).unwrap();

                if value.is_nan() {
                    assert!(value1.is_nan());
                } else {
                    assert_eq!(value, value1);
                }
            }
        }
        7 => {
            let mut data1 = data.to_vec();
            if let Ok(value) = from_bytes::<u64>(&mut data1) {
                let mut encoded = vec![0x0; value.get_size()];
                value.clone().to_bytes(&mut encoded).unwrap();
                let value1: u64 = from_bytes(&mut encoded).unwrap();

                assert_eq!(value, value1);
            }
        }
        8 => {
            let mut data1 = data.to_vec();
            if let Ok(u256) = from_bytes::<U256>(&mut data1) {
                let mut encoded = vec![0x0; u256.get_size()];
                u256.clone().to_bytes(&mut encoded).unwrap();
                let u256_1: U256 = from_bytes(&mut encoded).unwrap();

                assert_eq!(u256, u256_1);
            }
        }
        9 => {
            let mut data1 = data.to_vec();
            if let Ok(pubkey) = from_bytes::<PubKey>(&mut data1) {
                let mut encoded = vec![0x0; pubkey.get_size()];
                pubkey.clone().to_bytes(&mut encoded).unwrap();
                let pubkey1: PubKey = from_bytes(&mut encoded).unwrap();

                assert_eq!(pubkey, pubkey1);
            }
        }
        10 => {
            let mut data1 = data.to_vec();
            if let Ok(signature) = from_bytes::<Signature>(&mut data1) {
                let mut encoded = vec![0x0; signature.get_size()];
                signature.clone().to_bytes(&mut encoded).unwrap();
                let signature1: Signature = from_bytes(&mut encoded).unwrap();

                assert_eq!(signature, signature1);
            }
        }
        11 => {
            let mut data1 = data.to_vec();
            if let Ok(b032) = from_bytes::<B032>(&mut data1) {
                let mut encoded = vec![0x0; b032.get_size()];
                b032.clone().to_bytes(&mut encoded).unwrap();
                let b032_1: B032 = from_bytes(&mut encoded).unwrap();

                assert_eq!(b032, b032_1);
            }
        }
        12 => {
            let mut data1 = data.to_vec();
            if let Ok(b0255) = from_bytes::<B0255>(&mut data1) {
                let mut encoded = vec![0x0; b0255.get_size()];
                b0255.clone().to_bytes(&mut encoded).unwrap();
                let b0255_1: B0255 = from_bytes(&mut encoded).unwrap();

                assert_eq!(b0255, b0255_1);
            }
        }
        13 => {
            let mut data1 = data.to_vec();
            if let Ok(str0255) = from_bytes::<Str0255>(&mut data1) {
                let mut encoded = vec![0x0; str0255.get_size()];
                str0255.clone().to_bytes(&mut encoded).unwrap();
                let str0255_1: Str0255 = from_bytes(&mut encoded).unwrap();

                assert_eq!(str0255, str0255_1);
            }
        }
        14 => {
            let mut data1 = data.to_vec();
            if let Ok(b064k) = from_bytes::<B064K>(&mut data1) {
                let mut encoded = vec![0x0; b064k.get_size()];
                b064k.clone().to_bytes(&mut encoded).unwrap();
                let b064k_1: B064K = from_bytes(&mut encoded).unwrap();

                assert_eq!(b064k, b064k_1);
            }
        }
        15 => {
            let mut data1 = data.to_vec();
            if let Ok(b016m) = from_bytes::<B016M>(&mut data1) {
                let mut encoded = vec![0x0; b016m.get_size()];
                b016m.clone().to_bytes(&mut encoded).unwrap();
                let b016m_1: B016M = from_bytes(&mut encoded).unwrap();

                assert_eq!(b016m, b016m_1);
            }
        }
        _ => unreachable!(),
    }
});
