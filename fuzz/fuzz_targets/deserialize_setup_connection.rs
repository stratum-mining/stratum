#![no_main]
use binary_sv2::{Deserialize, Encodable};
use common_messages_sv2::SetupConnection;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: Vec<u8>| {
    let mut data1 = data.clone();
    if let Ok(setup_msg) = SetupConnection::from_bytes(&mut data1) {
        let dst = Vec::new();
        setup_msg.to_bytes(&mut dst.clone()).unwrap();

        let mut data2 = dst.clone();

        let setup_msg2 = SetupConnection::from_bytes(&mut data2).unwrap();
        let mut dst2 = Vec::new();
        setup_msg2.to_bytes(&mut dst2).unwrap();

        assert_eq!(dst, dst2);
    }
});
