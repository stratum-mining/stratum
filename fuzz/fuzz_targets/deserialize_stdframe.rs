#![no_main]
use codec_sv2::StandardSv2Frame;
use libfuzzer_sys::fuzz_target;
use parsers_sv2::AnyMessage;

type Message = AnyMessage<'static>;
type StdFrame = StandardSv2Frame<Message>;

fuzz_target!(|data: Vec<u8>| {
    if let Ok(frame) = StdFrame::from_bytes(data.clone()) {
        let mut serialized = vec![0u8; frame.encoded_length()];
        frame.clone().serialize(&mut serialized).unwrap();
        let frame2 = StdFrame::from_bytes(serialized.clone()).unwrap();
        let mut serialized2 = vec![0u8; frame.encoded_length()];
        frame2.serialize(&mut serialized2).unwrap();

        assert_eq!(serialized, serialized2);
    }
});
