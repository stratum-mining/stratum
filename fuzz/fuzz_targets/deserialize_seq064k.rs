#![no_main]
use arbitrary::Arbitrary;
use binary_sv2::{from_bytes, Encodable, GetSize, Seq064K, B0255, U256};
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

    match input.type_selector % 4 {
        0 => {
            let mut data1 = data.to_vec();
            if let Ok(seq) = from_bytes::<Seq064K<bool>>(&mut data1) {
                let mut encoded = vec![0x0; seq.get_size()];
                seq.clone().to_bytes(&mut encoded).unwrap();
                let seq2: Seq064K<bool> = from_bytes(&mut encoded).unwrap();

                assert_eq!(seq, seq2);
            }
        }
        1 => {
            let mut data1 = data.to_vec();
            if let Ok(seq) = from_bytes::<Seq064K<u64>>(&mut data1) {
                let mut encoded = vec![0x0; seq.get_size()];
                seq.clone().to_bytes(&mut encoded).unwrap();
                let seq2: Seq064K<u64> = from_bytes(&mut encoded).unwrap();

                assert_eq!(seq, seq2);
            }
        }
        2 => {
            let mut data1 = data.to_vec();
            if let Ok(seq) = from_bytes::<Seq064K<B0255>>(&mut data1) {
                let mut encoded = vec![0x0; seq.get_size()];
                seq.clone().to_bytes(&mut encoded).unwrap();
                let seq2: Seq064K<B0255> = from_bytes(&mut encoded).unwrap();

                assert_eq!(seq, seq2);
            }
        }
        3 => {
            let mut data1 = data.to_vec();
            if let Ok(seq) = from_bytes::<Seq064K<U256>>(&mut data1) {
                let mut encoded = vec![0x0; seq.get_size()];
                seq.clone().to_bytes(&mut encoded).unwrap();
                let seq2: Seq064K<U256> = from_bytes(&mut encoded).unwrap();

                assert_eq!(seq, seq2);
            }
        }
        _ => unreachable!(),
    }
});
