#![no_main]
use libfuzzer_sys::fuzz_target;
use binary_codec_sv2::{Seq064K,U256,B0255,Seq0255};
use binary_codec_sv2::from_bytes;
use codec_sv2::{StandardDecoder,Sv2Frame};
use roles_logic_sv2::parsers::AnyMessage;

type F = Sv2Frame<AnyMessage<'static>,Vec<u8>>;

fuzz_target!(|data: Vec<u8>| {
    let mut data = data;
    let mut decoder = StandardDecoder::<AnyMessage>::new();
    let _: Result<Seq064K<bool>,_> = from_bytes(&mut data);
    let _: Result<Seq064K<u64>,_> = from_bytes(&mut data);
    let _: Result<U256,_> = from_bytes(&mut data);
    let _: Result<B0255,_> = from_bytes(&mut data);
    let _: Result<Seq064K<B0255>,_> = from_bytes(&mut data);
    let _: Result<Seq0255<B0255>,_> = from_bytes(&mut data);
    let _: Result<Seq0255<U256>,_> = from_bytes(&mut data);
    let _: Result<F,_> = Sv2Frame::from_bytes(data.clone());

    let mut data_iter = data.clone().into_iter();
    loop {
        let writable = decoder.writable();
        for i in 0..writable.len() {
            match data_iter.next() {
                Some(x) => writable[i] = x,
                None => break
            }
        };
        let _ = decoder.next_frame();
        break;
    }
});
