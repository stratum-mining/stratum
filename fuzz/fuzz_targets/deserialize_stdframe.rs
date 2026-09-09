#![no_main]
use codec_sv2::StandardSv2Frame;
use libfuzzer_sys::fuzz_target;
use parsers_sv2::AnyMessage;

mod generators;

type Message = AnyMessage<'static>;
type StdFrame = StandardSv2Frame<Message>;

fuzz_target!(|data: Vec<u8>| {
    let mut u = arbitrary::Unstructured::new(&data);
    let frame_bytes = match generators::gen_sv2_frame(&mut u) {
        Ok(b) => b,
        Err(_) => return,
    };

    if let Ok(frame) = StdFrame::from_bytes(frame_bytes.clone().into()) {
        let header = frame.get_header().expect("Sv2Frame always has header");

        let ext_type_raw = u16::from_le_bytes([frame_bytes[0], frame_bytes[1]]);
        assert_eq!(
            header.ext_type(),
            ext_type_raw,
            "extension_type must match raw bytes"
        );

        let msg_type_raw = frame_bytes[2];
        assert_eq!(
            header.msg_type(),
            msg_type_raw,
            "msg_type must match raw byte"
        );

        assert_eq!(
            header.channel_msg(),
            ext_type_raw & 0x8000 != 0,
            "channel_msg() must match bit 15 of extension_type"
        );

        assert_eq!(
            header.ext_type_without_channel_msg(),
            ext_type_raw & 0x7FFF,
            "ext_type_without_channel_msg must clear bit 15"
        );

        assert_eq!(
            frame.encoded_length(),
            frame_bytes.len(),
            "encoded_length() must match total frame size"
        );

        // --- Serialization roundtrip ---
        let mut serialized = vec![0u8; frame.encoded_length()];
        frame.clone().serialize(&mut serialized).unwrap();

        assert_eq!(
            frame_bytes, serialized,
            "Serialized frame must match generated input"
        );

        let frame2 = StdFrame::from_bytes(serialized.clone().into()).unwrap();
        let mut serialized2 = vec![0u8; frame2.encoded_length()];
        frame2.clone().serialize(&mut serialized2).unwrap();

        assert_eq!(
            serialized, serialized2,
            "Frame serialization must be stable"
        );

        // --- Roundtrip header field preservation ---
        let header2 = frame2.get_header().expect("Sv2Frame always has header");
        assert_eq!(
            header2.ext_type(),
            ext_type_raw,
            "extension_type must survive roundtrip"
        );
        assert_eq!(
            header2.msg_type(),
            msg_type_raw,
            "msg_type must survive roundtrip"
        );
        assert_eq!(
            header2.channel_msg(),
            ext_type_raw & 0x8000 != 0,
            "channel_msg must survive roundtrip"
        );
    }
});
