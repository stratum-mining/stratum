use async_std::io::{ReadExt, WriteExt};
use async_std::net::TcpStream;
use codec_sv2::Frame;
use codec_sv2::StandardDecoder;
use codec_sv2::{StandardEitherFrame, StandardSv2Frame};
use messages_sv2::parsers::TemplateDistribution;
use std::convert::TryInto;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

pub type Message = TemplateDistribution<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

#[async_std::main]
async fn main() {
    let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8442);
    let mut stream = TcpStream::connect(socket).await.unwrap();
    let mut decoder = StandardDecoder::<Message>::new();
    stream.write_all(b"test").await.unwrap();
    loop {
        let writable = decoder.writable();
        match stream.read_exact(writable).await {
            Ok(_) => {
                if let Ok(mut frame) = decoder.next_frame() {
                    // this is how mesg-type should be retreived
                    let _msg_type = frame.get_header().unwrap().msg_type();
                    // but I force it to be 113 (0x71) in order the decode the correct message type
                    let msg_type = 113;
                    let payload = frame.payload();
                    match (msg_type, payload).try_into() {
                        Ok(TemplateDistribution::NewTemplate(m)) => println!("{:#?}", m),
                        Ok(_) => panic!(),
                        Err(_) => panic!(),
                    }
                }
            }
            Err(_) => {
                let _ = stream.shutdown(async_std::net::Shutdown::Both);
                break;
            }
        }
    }
}
