#![no_main]
use binary_sv2::{Deserialize, Encodable, GetSize};
use common_messages_sv2::SetupConnectionError;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: Vec<u8>| {
    let mut data1 = data.clone();
    if let Ok(setup_msg) = SetupConnectionError::from_bytes(&mut data1) {
        let mut dst = vec![0u8; setup_msg.get_size()];
        setup_msg.clone().to_bytes(&mut dst).unwrap();

        let mut data2 = dst.clone();

        let setup_msg2 = SetupConnectionError::from_bytes(&mut data2).unwrap();
        let mut dst2 = vec![0u8; setup_msg2.get_size()];
        setup_msg2.to_bytes(&mut dst2).unwrap();

        // just a sanity check: if we can parse it, impl Display should also work.
        let _ = format!("{setup_msg}");

        assert_eq!(dst, dst2);
    }
});
