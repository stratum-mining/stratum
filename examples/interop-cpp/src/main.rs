fn main() -> Result<(), std::io::Error> {
    use main_::main;
    main()
}

#[cfg(feature = "with_serde")]
mod main_ {
    pub fn main() -> Result<(), std::io::Error> {
        Ok(())
    }
}

#[cfg(not(feature = "with_serde"))]
mod main_ {
    use codec_sv2::{Encoder, Frame, StandardDecoder, StandardSv2Frame};
    use common_messages_sv2::{Protocol, SetupConnection, SetupConnectionError};
    use const_sv2::{
        CHANNEL_BIT_SETUP_CONNECTION, MESSAGE_TYPE_SETUP_CONNECTION,
        MESSAGE_TYPE_SETUP_CONNECTION_ERROR,
    };
    use std::{
        convert::{TryFrom, TryInto},
        io::{Read, Write},
        net::TcpStream,
    };

    use binary_sv2::{
        decodable::{DecodableField, FieldMarker},
        encodable::EncodableField,
        from_bytes, Deserialize, Error,
    };

    #[derive(Clone, Debug)]
    pub enum Sv2Message<'a> {
        SetupConnection(SetupConnection<'a>),
        SetupConnectionError(SetupConnectionError<'a>),
    }

    impl binary_sv2::GetSize for Sv2Message<'_> {
        fn get_size(&self) -> usize {
            match self {
                Sv2Message::SetupConnection(a) => a.get_size(),
                Sv2Message::SetupConnectionError(a) => a.get_size(),
            }
        }
    }

    impl<'decoder> Deserialize<'decoder> for Sv2Message<'decoder> {
        fn get_structure(_v: &[u8]) -> std::result::Result<Vec<FieldMarker>, binary_sv2::Error> {
            unimplemented!()
        }
        fn from_decoded_fields(
            _v: Vec<DecodableField<'decoder>>,
        ) -> std::result::Result<Self, binary_sv2::Error> {
            unimplemented!()
        }
    }

    impl<'a> TryFrom<(u8, &'a mut [u8])> for Sv2Message<'a> {
        type Error = Error;

        fn try_from(v: (u8, &'a mut [u8])) -> Result<Self, Self::Error> {
            let msg_type = v.0;
            match msg_type {
                MESSAGE_TYPE_SETUP_CONNECTION => {
                    let message: SetupConnection<'a> = from_bytes(v.1)?;
                    Ok(Sv2Message::SetupConnection(message))
                }
                MESSAGE_TYPE_SETUP_CONNECTION_ERROR => {
                    let message: SetupConnectionError<'a> = from_bytes(v.1)?;
                    Ok(Sv2Message::SetupConnectionError(message))
                }
                _ => panic!(),
            }
        }
    }

    impl<'decoder> From<Sv2Message<'decoder>> for EncodableField<'decoder> {
        fn from(m: Sv2Message<'decoder>) -> Self {
            match m {
                Sv2Message::SetupConnection(a) => a.into(),
                Sv2Message::SetupConnectionError(a) => a.into(),
            }
        }
    }

    pub fn main() -> Result<(), std::io::Error> {
        let mut encoder = Encoder::<SetupConnection>::new();

        let setup_connection = SetupConnection {
            protocol: Protocol::TemplateDistributionProtocol,
            min_version: 2,
            max_version: 2,
            flags: 0,
            endpoint_host: "0.0.0.0".to_string().into_bytes().try_into().unwrap(),
            endpoint_port: 8081,
            vendor: "Bitmain".to_string().into_bytes().try_into().unwrap(),
            hardware_version: "901".to_string().into_bytes().try_into().unwrap(),
            firmware: "abcX".to_string().into_bytes().try_into().unwrap(),
            device_id: "89567".to_string().into_bytes().try_into().unwrap(),
        };

        let setup_connection = StandardSv2Frame::from_message(
            setup_connection,
            MESSAGE_TYPE_SETUP_CONNECTION,
            0,
            CHANNEL_BIT_SETUP_CONNECTION,
        )
        .unwrap();
        let setup_connection = encoder.encode(setup_connection).unwrap();

        #[allow(deprecated)]
        std::thread::sleep_ms(2000);

        let mut stream = TcpStream::connect("0.0.0.0:8080")?;

        let mut decoder = StandardDecoder::<Sv2Message<'static>>::new();

        loop {
            #[allow(deprecated)]
            std::thread::sleep_ms(500);

            stream.write_all(setup_connection)?;

            loop {
                let buffer = decoder.writable();
                stream.read_exact(buffer).unwrap();
                if let Ok(mut f) = decoder.next_frame() {
                    let msg_type = f.get_header().unwrap().msg_type();
                    let payload = f.payload();
                    let message: Sv2Message = (msg_type, payload).try_into().unwrap();
                    match message {
                        Sv2Message::SetupConnection(_) => panic!(),
                        Sv2Message::SetupConnectionError(m) => {
                            println!("RUST MESSAGE RECEIVED");
                            println!("  {}", std::str::from_utf8(m.error_code.as_ref()).unwrap());
                        }
                    }
                    break;
                }
            }
        }
    }
}
