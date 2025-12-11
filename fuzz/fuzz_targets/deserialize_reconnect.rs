#![no_main]
use binary_sv2::{Deserialize, Encodable, GetSize};
use common_messages_sv2::Reconnect;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: Vec<u8>| {
    let mut data1 = data.clone();
    if let Ok(msg) = Reconnect::from_bytes(&mut data1) {
        let mut dst = vec![0u8; msg.get_size()];
        msg.clone().to_bytes(&mut dst).unwrap();

        let mut data2 = dst.clone();

        let msg2 = Reconnect::from_bytes(&mut data2).unwrap();
        let mut dst2 = vec![0u8; msg2.get_size()];
        msg2.clone().to_bytes(&mut dst2).unwrap();

        // just a sanity check: if we can parse it, impl Display should also work.
        let _ = format!("{}", msg2);

        assert_eq!(msg2.eq(&msg), true);
    }
});
