// SPDX-License-Identifier: CC0-1.0

//! This module implements a synchronous transport over a raw [`std::net::TcpListener`].
//! Note that it does not handle TCP over Unix Domain Sockets, see `simple_uds` for this.

use std::io::{BufRead, BufReader, BufWriter, Write};
use std::{error, fmt, io, net, time};

use crate::client::Transport;
use crate::{Request, Response};

#[derive(Debug, Clone)]
/// Simple synchronous TCP transport.
pub struct TcpTransport {
    /// The internet socket address to connect to.
    pub addr: net::SocketAddr,
    /// The read and write timeout to use for this connection.
    pub timeout: Option<time::Duration>,
}

impl TcpTransport {
    /// Creates a new `TcpTransport` without timeouts.
    pub fn new(addr: net::SocketAddr) -> TcpTransport {
        TcpTransport {
            addr,
            timeout: None,
        }
    }

    fn request<R>(&self, req: impl serde::Serialize) -> Result<R, Error>
    where
        R: for<'a> serde::de::Deserialize<'a>,
    {
        let sock = net::TcpStream::connect(self.addr)?;
        sock.set_read_timeout(self.timeout)?;
        sock.set_write_timeout(self.timeout)?;

        let mut message_writer = BufWriter::new(sock.try_clone().unwrap());
        let message_w = serde_json::to_string(&req).unwrap();
        let message_w = message_w.into_bytes();
        message_writer.write_all(&message_w)?;
        message_writer.flush()?;
        //serde_json::to_writer(&mut sock, &req)?;

        // NOTE: we don't check the id there, so it *must* be synchronous
        // memo reader and writer are changed because the serde import is custom, therefore we do
        // bnot have serde::json::to_writer or serde_json::from_reader

        let mut message_reader = BufReader::new(sock);
        let mut message_w = String::new();
        let _ = message_reader.read_line(&mut message_w);

        let resp: R = serde_json::Deserializer::from_str(&message_w)
            .into_iter()
            .next()
            .ok_or(Error::Timeout)??;
        Ok(resp)
    }
}

impl Transport for TcpTransport {
    fn send_request(&self, req: Request) -> Result<Response, crate::Error> {
        Ok(self.request(req)?)
    }

    fn send_batch(&self, reqs: &[Request]) -> Result<Vec<Response>, crate::Error> {
        Ok(self.request(reqs)?)
    }

    fn fmt_target(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.addr)
    }
}

/// Error that can occur while using the TCP transport.
#[derive(Debug)]
pub enum Error {
    /// An error occurred on the socket layer.
    SocketError(io::Error),
    /// We didn't receive a complete response till the deadline ran out.
    Timeout,
    /// JSON parsing error.
    Json(serde_json::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        use Error::*;

        match *self {
            SocketError(ref e) => write!(f, "couldn't connect to host: {}", e),
            Timeout => f.write_str("didn't receive response data in time, timed out."),
            Json(ref e) => write!(f, "JSON error: {}", e),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        use self::Error::*;

        match *self {
            SocketError(ref e) => Some(e),
            Timeout => None,
            Json(ref _e) => todo!(), //Some(e),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::SocketError(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(e)
    }
}

impl From<Error> for crate::Error {
    fn from(e: Error) -> crate::Error {
        match e {
            Error::Json(e) => crate::Error::Json(e),
            e => crate::Error::Transport(Box::new(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        thread,
    };

    use super::*;
    use crate::Client;

    // Test a dummy request / response over a raw TCP transport
    #[test]
    fn sanity_check_tcp_transport() {
        let addr: net::SocketAddr =
            net::SocketAddrV4::new(net::Ipv4Addr::new(127, 0, 0, 1), 0).into();
        let server = net::TcpListener::bind(addr).unwrap();
        let addr = server.local_addr().unwrap();
        let dummy_req = Request {
            method: "arandommethod",
            params: &[],
            id: serde_json::Value::Number(4242242.into()),
            jsonrpc: Some("2.0"),
        };
        let dummy_req_ser = serde_json::to_vec(&dummy_req).unwrap();
        let dummy_resp = Response {
            result: None,
            error: None,
            id: serde_json::Value::Number(4242242.into()),
            jsonrpc: Some("2.0".into()),
        };
        let dummy_resp_ser = serde_json::to_vec(&dummy_resp).unwrap();

        let client_thread = thread::spawn(move || {
            let transport = TcpTransport {
                addr,
                timeout: Some(time::Duration::from_secs(5)),
            };
            let client = Client::with_transport(transport);

            client.send_request(dummy_req.clone()).unwrap()
        });

        let (mut stream, _) = server.accept().unwrap();
        stream.set_read_timeout(Some(time::Duration::from_secs(5))).unwrap();
        let mut recv_req = vec![0; dummy_req_ser.len()];
        let mut read = 0;
        while read < dummy_req_ser.len() {
            read += stream.read(&mut recv_req[read..]).unwrap();
        }
        assert_eq!(recv_req, dummy_req_ser);

        stream.write_all(&dummy_resp_ser).unwrap();
        stream.flush().unwrap();
        let recv_resp = client_thread.join().unwrap(); //line 184
        assert_eq!(serde_json::to_vec(&recv_resp).unwrap(), dummy_resp_ser);
    }
}
