#![no_main]
use framing_sv2::framing::Sv2Frame;
use libfuzzer_sys::fuzz_target;
use parsers_sv2::AnyMessage;

type Message = AnyMessage<'static>;

fuzz_target!(|data: Vec<u8>| {
    if let Ok(frame) = Sv2Frame::<Message, Vec<u8>>::from_bytes(data.clone()) {
        let mut serialized = vec![0u8; frame.encoded_length()];
        frame.clone().serialize(&mut serialized).unwrap();
        let frame2 = Sv2Frame::<Message, Vec<u8>>::from_bytes(serialized.clone()).unwrap();
        let mut serialized2 = vec![0u8; frame.encoded_length()];
        frame2.serialize(&mut serialized2).unwrap();

        assert_eq!(serialized, serialized2);
    }
});
