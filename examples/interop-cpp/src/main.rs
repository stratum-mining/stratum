use codec_sv2::{Encoder, Frame, StandardSv2Frame};
use common_messages_sv2::{Protocol, SetupConnection};
use const_sv2::MESSAGE_TYPE_SETUP_CONNECTION;
use std::convert::TryInto;
use std::io::Write;
use std::net::TcpStream;

fn main() -> Result<(), std::io::Error> {
    let mut encoder = Encoder::<SetupConnection>::new();

    let setup_connection = SetupConnection {
        protocol: Protocol::TemplateDistributionProtocol,
        min_version: 2,
        max_version: 2,
        flags: 0,
        endpoint_host: "0.0.0.0".to_string().into_bytes().try_into().unwrap(),
        endpoint_port: 8081,
        vendor: "Bitmain".to_string().into_bytes().try_into().unwrap(),
        hardware_version: "Bitmain".to_string().into_bytes().try_into().unwrap(),
        firmware: "Bitmain".to_string().into_bytes().try_into().unwrap(),
        device_id: "Bitmain".to_string().into_bytes().try_into().unwrap(),
    };

    let setup_connection =
        StandardSv2Frame::from_message(setup_connection, MESSAGE_TYPE_SETUP_CONNECTION, 0).unwrap();
    let setup_connection = encoder.encode(setup_connection).unwrap();

    #[allow(deprecated)]
    std::thread::sleep_ms(2000);

    let mut stream = TcpStream::connect("0.0.0.0:8080")?;

    loop {
        #[allow(deprecated)]
        std::thread::sleep_ms(500);
        stream.write_all(setup_connection)?;
    }
}
