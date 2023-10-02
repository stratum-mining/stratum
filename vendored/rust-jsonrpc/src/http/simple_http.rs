// SPDX-License-Identifier: CC0-1.0

//! This module implements a minimal and non standard conforming HTTP 1.0
//! round-tripper that works with the bitcoind RPC server. This can be used
//! if minimal dependencies are a goal and synchronous communication is ok.

#[cfg(feature = "proxy")]
use socks::Socks5Stream;
use std::io::{BufRead, BufReader, Read, Write};
#[cfg(not(jsonrpc_fuzz))]
use std::net::TcpStream;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use std::{error, fmt, io, net, num};

use crate::client::Transport;
use crate::http::DEFAULT_PORT;
#[cfg(feature = "proxy")]
use crate::http::DEFAULT_PROXY_PORT;
use crate::{Request, Response};

/// Absolute maximum content length allowed before cutting off the response.
const FINAL_RESP_ALLOC: u64 = 1024 * 1024 * 1024;

#[cfg(not(jsonrpc_fuzz))]
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

#[cfg(jsonrpc_fuzz)]
const DEFAULT_TIMEOUT: Duration = Duration::from_millis(1);

/// Simple HTTP transport that implements the necessary subset of HTTP for
/// running a bitcoind RPC client.
#[derive(Clone, Debug)]
pub struct SimpleHttpTransport {
    addr: net::SocketAddr,
    path: String,
    timeout: Duration,
    /// The value of the `Authorization` HTTP header.
    basic_auth: Option<String>,
    #[cfg(feature = "proxy")]
    proxy_addr: net::SocketAddr,
    #[cfg(feature = "proxy")]
    proxy_auth: Option<(String, String)>,
    sock: Arc<Mutex<Option<BufReader<TcpStream>>>>,
}

impl Default for SimpleHttpTransport {
    fn default() -> Self {
        SimpleHttpTransport {
            addr: net::SocketAddr::new(
                net::IpAddr::V4(net::Ipv4Addr::new(127, 0, 0, 1)),
                DEFAULT_PORT,
            ),
            path: "/".to_owned(),
            timeout: DEFAULT_TIMEOUT,
            basic_auth: None,
            #[cfg(feature = "proxy")]
            proxy_addr: net::SocketAddr::new(
                net::IpAddr::V4(net::Ipv4Addr::new(127, 0, 0, 1)),
                DEFAULT_PROXY_PORT,
            ),
            #[cfg(feature = "proxy")]
            proxy_auth: None,
            sock: Arc::new(Mutex::new(None)),
        }
    }
}

impl SimpleHttpTransport {
    /// Constructs a new [`SimpleHttpTransport`] with default parameters.
    pub fn new() -> Self {
        SimpleHttpTransport::default()
    }

    /// Returns a builder for [`SimpleHttpTransport`].
    pub fn builder() -> Builder {
        Builder::new()
    }

    /// Replaces the URL of the transport.
    pub fn set_url(&mut self, url: &str) -> Result<(), Error> {
        let url = check_url(url)?;
        self.addr = url.0;
        self.path = url.1;
        Ok(())
    }

    /// Replaces only the path part of the URL.
    pub fn set_url_path(&mut self, path: String) {
        self.path = path;
    }

    fn request<R>(&self, req: impl serde::Serialize) -> Result<R, Error>
    where
        R: for<'a> serde::de::Deserialize<'a>,
    {
        match self.try_request(req) {
            Ok(response) => Ok(response),
            Err(err) => {
                // No part of this codebase should panic, so unwrapping a mutex lock is fine
                *self.sock.lock().expect("poisoned mutex") = None;
                Err(err)
            }
        }
    }

    #[cfg(feature = "proxy")]
    fn fresh_socket(&self) -> Result<TcpStream, Error> {
        let stream = if let Some((username, password)) = &self.proxy_auth {
            Socks5Stream::connect_with_password(
                self.proxy_addr,
                self.addr,
                username.as_str(),
                password.as_str(),
            )?
        } else {
            Socks5Stream::connect(self.proxy_addr, self.addr)?
        };
        Ok(stream.into_inner())
    }

    #[cfg(not(feature = "proxy"))]
    fn fresh_socket(&self) -> Result<TcpStream, Error> {
        let stream = TcpStream::connect_timeout(&self.addr, self.timeout)?;
        stream.set_read_timeout(Some(self.timeout))?;
        stream.set_write_timeout(Some(self.timeout))?;
        Ok(stream)
    }

    fn try_request<R>(&self, req: impl serde::Serialize) -> Result<R, Error>
    where
        R: for<'a> serde::de::Deserialize<'a>,
    {
        // No part of this codebase should panic, so unwrapping a mutex lock is fine
        let mut sock_lock: MutexGuard<Option<_>> = self.sock.lock().expect("poisoned mutex");
        if sock_lock.is_none() {
            *sock_lock = Some(BufReader::new(self.fresh_socket()?));
        };
        // In the immediately preceding block, we made sure that `sock` is non-`None`,
        // so unwrapping here is fine.
        let sock: &mut BufReader<_> = sock_lock.as_mut().unwrap();

        // Serialize the body first so we can set the Content-Length header.
        let body = serde_json::to_vec(&req)?;

        let mut request_bytes = Vec::new();

        request_bytes.write_all(b"POST ")?;
        request_bytes.write_all(self.path.as_bytes())?;
        request_bytes.write_all(b" HTTP/1.1\r\n")?;
        // Write headers
        request_bytes.write_all(b"host: ")?;
        request_bytes.write_all(self.addr.to_string().as_bytes())?;
        request_bytes.write_all(b"\r\n")?;
        request_bytes.write_all(b"Content-Type: application/json\r\n")?;
        request_bytes.write_all(b"Content-Length: ")?;
        request_bytes.write_all(body.len().to_string().as_bytes())?;
        request_bytes.write_all(b"\r\n")?;
        if let Some(ref auth) = self.basic_auth {
            request_bytes.write_all(b"Authorization: ")?;
            request_bytes.write_all(auth.as_ref())?;
            request_bytes.write_all(b"\r\n")?;
        }
        // Write body
        request_bytes.write_all(b"\r\n")?;
        request_bytes.write_all(&body)?;

        // Send HTTP request
        let write_success = sock.get_mut().write_all(request_bytes.as_slice()).is_ok()
            && sock.get_mut().flush().is_ok();

        // This indicates the socket is broken so let's retry the send once with a fresh socket
        if !write_success {
            *sock.get_mut() = self.fresh_socket()?;
            sock.get_mut().write_all(request_bytes.as_slice())?;
            sock.get_mut().flush()?;
        }

        // Parse first HTTP response header line
        let mut header_buf = String::new();
        let read_success = sock.read_line(&mut header_buf).is_ok();

        // This is another possible indication that the socket is broken so let's retry the send once
        // with a fresh socket IF the write attempt has not already experienced a failure
        if (!read_success || header_buf.is_empty()) && write_success {
            *sock.get_mut() = self.fresh_socket()?;
            sock.get_mut().write_all(request_bytes.as_slice())?;
            sock.get_mut().flush()?;

            sock.read_line(&mut header_buf)?;
        }

        if header_buf.len() < 12 {
            return Err(Error::HttpResponseTooShort {
                actual: header_buf.len(),
                needed: 12,
            });
        }
        if !header_buf.as_bytes()[..12].is_ascii() {
            return Err(Error::HttpResponseNonAsciiHello(header_buf.as_bytes()[..12].to_vec()));
        }
        if !header_buf.starts_with("HTTP/1.1 ") {
            return Err(Error::HttpResponseBadHello {
                actual: header_buf[0..9].into(),
                expected: "HTTP/1.1 ".into(),
            });
        }
        let response_code = match header_buf[9..12].parse::<u16>() {
            Ok(n) => n,
            Err(e) => return Err(Error::HttpResponseBadStatus(header_buf[9..12].into(), e)),
        };

        // Parse response header fields
        let mut content_length = None;
        loop {
            header_buf.clear();
            sock.read_line(&mut header_buf)?;
            if header_buf == "\r\n" {
                break;
            }
            header_buf.make_ascii_lowercase();

            const CONTENT_LENGTH: &str = "content-length: ";
            if let Some(s) = header_buf.strip_prefix(CONTENT_LENGTH) {
                content_length = Some(
                    s.trim()
                        .parse::<u64>()
                        .map_err(|e| Error::HttpResponseBadContentLength(s.into(), e))?,
                );
            }
        }

        if response_code == 401 {
            // There is no body in a 401 response, so don't try to read it
            return Err(Error::HttpErrorCode(response_code));
        }

        // Read up to `content_length` bytes. Note that if there is no content-length
        // header, we will assume an effectively infinite content length, i.e. we will
        // just keep reading from the socket until it is closed.
        let reader = match content_length {
            None => sock.take(FINAL_RESP_ALLOC),
            Some(n) if n > FINAL_RESP_ALLOC => {
                return Err(Error::HttpResponseContentLengthTooLarge {
                    length: n,
                    max: FINAL_RESP_ALLOC,
                });
            }
            Some(n) => sock.take(n),
        };


        let mut message_reader = BufReader::new(reader).lines();
        let message_r = message_reader.next().unwrap().unwrap();
        // Attempt to parse the response. Don't check the HTTP error code until
        // after parsing, since Bitcoin Core will often return a descriptive JSON
        // error structure which is more useful than the error code.
        match serde_json::from_str(&message_r) {
            Ok(s) => {
                if content_length.is_some() {
                    message_reader.for_each(std::mem::drop);
                    //reader.bytes().count(); // consume any trailing bytes
                }
                Ok(s)
            }
            Err(e) => {
                // If the response was not 200, assume the parse failed because of that
                if response_code != 200 {
                    Err(Error::HttpErrorCode(response_code))
                } else {
                    // If it was 200 then probably it was legitimately a parse error
                    Err(e.into())
                }
            }
        }
    }
}

/// Does some very basic manual URL parsing because the uri/url crates
/// all have unicode-normalization as a dependency and that's broken.
fn check_url(url: &str) -> Result<(SocketAddr, String), Error> {
    // The fallback port in case no port was provided.
    // This changes when the http or https scheme was provided.
    let mut fallback_port = DEFAULT_PORT;

    // We need to get the hostname and the port.
    // (1) Split scheme
    let after_scheme = {
        let mut split = url.splitn(2, "://");
        let s = split.next().unwrap();
        match split.next() {
            None => s, // no scheme present
            Some(after) => {
                // Check if the scheme is http or https.
                if s == "http" {
                    fallback_port = 80;
                } else if s == "https" {
                    fallback_port = 443;
                } else {
                    return Err(Error::url(url, "scheme should be http or https"));
                }
                after
            }
        }
    };
    // (2) split off path
    let (before_path, path) = {
        if let Some(slash) = after_scheme.find('/') {
            (&after_scheme[0..slash], &after_scheme[slash..])
        } else {
            (after_scheme, "/")
        }
    };
    // (3) split off auth part
    let after_auth = {
        let mut split = before_path.splitn(2, '@');
        let s = split.next().unwrap();
        split.next().unwrap_or(s)
    };

    // (4) Parse into socket address.
    // At this point we either have <host_name> or <host_name_>:<port>
    // `std::net::ToSocketAddrs` requires `&str` to have <host_name_>:<port> format.
    let mut addr = match after_auth.to_socket_addrs() {
        Ok(addr) => addr,
        Err(_) => {
            // Invalid socket address. Try to add port.
            format!("{}:{}", after_auth, fallback_port).to_socket_addrs()?
        }
    };

    match addr.next() {
        Some(a) => Ok((a, path.to_owned())),
        None => Err(Error::url(url, "invalid hostname: error extracting socket address")),
    }
}

impl Transport for SimpleHttpTransport {
    fn send_request(&self, req: Request) -> Result<Response, crate::Error> {
        Ok(self.request(req)?)
    }

    fn send_batch(&self, reqs: &[Request]) -> Result<Vec<Response>, crate::Error> {
        Ok(self.request(reqs)?)
    }

    fn fmt_target(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "http://{}:{}{}", self.addr.ip(), self.addr.port(), self.path)
    }
}

/// Builder for simple bitcoind [`SimpleHttpTransport`].
#[derive(Clone, Debug)]
pub struct Builder {
    tp: SimpleHttpTransport,
}

impl Builder {
    /// Constructs a new [`Builder`] with default configuration.
    pub fn new() -> Builder {
        Builder {
            tp: SimpleHttpTransport::new(),
        }
    }

    /// Sets the timeout after which requests will abort if they aren't finished.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.tp.timeout = timeout;
        self
    }

    /// Sets the URL of the server to the transport.
    pub fn url(mut self, url: &str) -> Result<Self, Error> {
        self.tp.set_url(url)?;
        Ok(self)
    }

    /// Adds authentication information to the transport.
    pub fn auth<S: AsRef<str>>(mut self, user: S, pass: Option<S>) -> Self {
        let mut auth = user.as_ref().to_owned();
        auth.push(':');
        if let Some(ref pass) = pass {
            auth.push_str(pass.as_ref());
        }
        self.tp.basic_auth = Some(format!("Basic {}", &base64::encode(auth.as_bytes())));
        self
    }

    /// Adds authentication information to the transport using a cookie string ('user:pass').
    pub fn cookie_auth<S: AsRef<str>>(mut self, cookie: S) -> Self {
        self.tp.basic_auth = Some(format!("Basic {}", &base64::encode(cookie.as_ref().as_bytes())));
        self
    }

    /// Adds proxy address to the transport for SOCKS5 proxy.
    #[cfg(feature = "proxy")]
    pub fn proxy_addr<S: AsRef<str>>(mut self, proxy_addr: S) -> Result<Self, Error> {
        // We don't expect path in proxy address.
        self.tp.proxy_addr = check_url(proxy_addr.as_ref())?.0;
        Ok(self)
    }

    /// Adds optional proxy authentication as ('username', 'password').
    #[cfg(feature = "proxy")]
    pub fn proxy_auth<S: AsRef<str>>(mut self, user: S, pass: S) -> Self {
        self.tp.proxy_auth =
            Some((user, pass)).map(|(u, p)| (u.as_ref().to_string(), p.as_ref().to_string()));
        self
    }

    /// Builds the final [`SimpleHttpTransport`].
    pub fn build(self) -> SimpleHttpTransport {
        self.tp
    }
}

impl Default for Builder {
    fn default() -> Self {
        Builder::new()
    }
}

impl crate::Client {
    /// Creates a new JSON-RPC client using a bare-minimum HTTP transport.
    pub fn simple_http(
        url: &str,
        user: Option<String>,
        pass: Option<String>,
    ) -> Result<crate::Client, Error> {
        let mut builder = Builder::new().url(url)?;
        if let Some(user) = user {
            builder = builder.auth(user, pass);
        }
        Ok(crate::Client::with_transport(builder.build()))
    }

    /// Creates a new JSON_RPC client using a HTTP-Socks5 proxy transport.
    #[cfg(feature = "proxy")]
    pub fn http_proxy(
        url: &str,
        user: Option<String>,
        pass: Option<String>,
        proxy_addr: &str,
        proxy_auth: Option<(&str, &str)>,
    ) -> Result<crate::Client, Error> {
        let mut builder = Builder::new().url(url)?;
        if let Some(user) = user {
            builder = builder.auth(user, pass);
        }
        builder = builder.proxy_addr(proxy_addr)?;
        if let Some((user, pass)) = proxy_auth {
            builder = builder.proxy_auth(user, pass);
        }
        let tp = builder.build();
        Ok(crate::Client::with_transport(tp))
    }
}

/// Error that can happen when sending requests.
#[derive(Debug)]
pub enum Error {
    /// An invalid URL was passed.
    InvalidUrl {
        /// The URL passed.
        url: String,
        /// The reason the URL is invalid.
        reason: &'static str,
    },
    /// An error occurred on the socket layer.
    SocketError(io::Error),
    /// The HTTP response was too short to even fit a HTTP 1.1 header.
    HttpResponseTooShort {
        /// The total length of the response.
        actual: usize,
        /// Minimum length we can parse.
        needed: usize,
    },
    /// The HTTP response started with a HTTP/1.1 line which was not ASCII.
    HttpResponseNonAsciiHello(Vec<u8>),
    /// The HTTP response did not start with HTTP/1.1
    HttpResponseBadHello {
        /// Actual HTTP-whatever string.
        actual: String,
        /// The hello string of the HTTP version we support.
        expected: String,
    },
    /// Could not parse the status value as a number.
    HttpResponseBadStatus(String, num::ParseIntError),
    /// Could not parse the status value as a number.
    HttpResponseBadContentLength(String, num::ParseIntError),
    /// The indicated content-length header exceeded our maximum.
    HttpResponseContentLengthTooLarge {
        /// The length indicated in the content-length header.
        length: u64,
        /// Our hard maximum on number of bytes we'll try to read.
        max: u64,
    },
    /// Unexpected HTTP error code (non-200).
    HttpErrorCode(u16),
    /// Received EOF before getting as many bytes as were indicated by the content-length header.
    IncompleteResponse {
        /// The content-length header.
        content_length: u64,
        /// The number of bytes we actually read.
        n_read: u64,
    },
    /// JSON parsing error.
    Json(serde_json::Error),
}

impl Error {
    /// Utility method to create [`Error::InvalidUrl`] variants.
    fn url<U: Into<String>>(url: U, reason: &'static str) -> Error {
        Error::InvalidUrl {
            url: url.into(),
            reason,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        use Error::*;

        match *self {
            InvalidUrl {
                ref url,
                ref reason,
            } => write!(f, "invalid URL '{}': {}", url, reason),
            SocketError(ref e) => write!(f, "Couldn't connect to host: {}", e),
            HttpResponseTooShort {
                ref actual,
                ref needed,
            } => {
                write!(f, "HTTP response too short: length {}, needed {}.", actual, needed)
            }
            HttpResponseNonAsciiHello(ref bytes) => {
                write!(f, "HTTP response started with non-ASCII {:?}", bytes)
            }
            HttpResponseBadHello {
                ref actual,
                ref expected,
            } => {
                write!(f, "HTTP response started with `{}`; expected `{}`.", actual, expected)
            }
            HttpResponseBadStatus(ref status, ref err) => {
                write!(f, "HTTP response had bad status code `{}`: {}.", status, err)
            }
            HttpResponseBadContentLength(ref len, ref err) => {
                write!(f, "HTTP response had bad content length `{}`: {}.", len, err)
            }
            HttpResponseContentLengthTooLarge {
                length,
                max,
            } => {
                write!(f, "HTTP response content length {} exceeds our max {}.", length, max)
            }
            HttpErrorCode(c) => write!(f, "unexpected HTTP code: {}", c),
            IncompleteResponse {
                content_length,
                n_read,
            } => {
                write!(
                    f,
                    "read {} bytes but HTTP response content-length header was {}.",
                    n_read, content_length
                )
            }
            Json(ref e) => write!(f, "JSON error: {}", e),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        use self::Error::*;

        match *self {
            InvalidUrl {
                ..
            }
            | HttpResponseTooShort {
                ..
            }
            | HttpResponseNonAsciiHello(..)
            | HttpResponseBadHello {
                ..
            }
            | HttpResponseBadStatus(..)
            | HttpResponseBadContentLength(..)
            | HttpResponseContentLengthTooLarge {
                ..
            }
            | HttpErrorCode(_)
            | IncompleteResponse {
                ..
            } => None,
            SocketError(ref e) => Some(e),
            Json(ref _e) => todo!()//Some(e),
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

/// Global mutex used by the fuzzing harness to inject data into the read end of the TCP stream.
#[cfg(jsonrpc_fuzz)]
pub static FUZZ_TCP_SOCK: Mutex<Option<io::Cursor<Vec<u8>>>> = Mutex::new(None);

#[cfg(jsonrpc_fuzz)]
#[derive(Clone, Debug)]
struct TcpStream;

#[cfg(jsonrpc_fuzz)]
mod impls {
    use super::*;
    impl Read for TcpStream {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            match *FUZZ_TCP_SOCK.lock().unwrap() {
                Some(ref mut cursor) => io::Read::read(cursor, buf),
                None => Ok(0),
            }
        }
    }
    impl Write for TcpStream {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            io::sink().write(buf)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl TcpStream {
        pub fn connect_timeout(_: &SocketAddr, _: Duration) -> io::Result<Self> {
            Ok(TcpStream)
        }
        pub fn set_read_timeout(&self, _: Option<Duration>) -> io::Result<()> {
            Ok(())
        }
        pub fn set_write_timeout(&self, _: Option<Duration>) -> io::Result<()> {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net;
    #[cfg(feature = "proxy")]
    use std::str::FromStr;

    use super::*;
    use crate::Client;

    #[test]
    fn test_urls() {
        let addr: net::SocketAddr = ("localhost", 22).to_socket_addrs().unwrap().next().unwrap();
        let urls = [
            "localhost:22",
            "http://localhost:22/",
            "https://localhost:22/walletname/stuff?it=working",
            "http://me:weak@localhost:22/wallet",
        ];
        for u in &urls {
            let tp = Builder::new().url(u).unwrap().build();
            assert_eq!(tp.addr, addr);
        }

        // Default port and 80 and 443 fill-in.
        let addr: net::SocketAddr = ("localhost", 80).to_socket_addrs().unwrap().next().unwrap();
        let tp = Builder::new().url("http://localhost/").unwrap().build();
        assert_eq!(tp.addr, addr);
        let addr: net::SocketAddr = ("localhost", 443).to_socket_addrs().unwrap().next().unwrap();
        let tp = Builder::new().url("https://localhost/").unwrap().build();
        assert_eq!(tp.addr, addr);
        let addr: net::SocketAddr =
            ("localhost", super::DEFAULT_PORT).to_socket_addrs().unwrap().next().unwrap();
        let tp = Builder::new().url("localhost").unwrap().build();
        assert_eq!(tp.addr, addr);

        let valid_urls = [
            "localhost",
            "127.0.0.1:8080",
            "http://127.0.0.1:8080/",
            "http://127.0.0.1:8080/rpc/test",
            "https://127.0.0.1/rpc/test",
            "http://[2001:0db8:85a3:0000:0000:8a2e:0370:7334]:8300",
            "http://[2001:0db8:85a3:0000:0000:8a2e:0370:7334]",
        ];
        for u in &valid_urls {
            let (addr, path) = check_url(u).unwrap();
            let builder = Builder::new().url(u).unwrap_or_else(|_| panic!("error for: {}", u));
            assert_eq!(builder.tp.addr, addr);
            assert_eq!(builder.tp.path, path);
            assert_eq!(builder.tp.timeout, DEFAULT_TIMEOUT);
            assert_eq!(builder.tp.basic_auth, None);
            #[cfg(feature = "proxy")]
            assert_eq!(builder.tp.proxy_addr, SocketAddr::from_str("127.0.0.1:9050").unwrap());
        }

        let invalid_urls = [
            "127.0.0.1.0:8080",
            "httpx://127.0.0.1:8080/",
            "ftp://127.0.0.1:8080/rpc/test",
            "http://127.0.0./rpc/test",
            // NB somehow, Rust's IpAddr accepts "127.0.0" and adds the extra 0..
        ];
        for u in &invalid_urls {
            if let Ok(b) = Builder::new().url(u) {
                let tp = b.build();
                panic!("expected error for url {}, got {:?}", u, tp);
            }
        }
    }

    #[test]
    fn construct() {
        let tp = Builder::new()
            .timeout(Duration::from_millis(100))
            .url("localhost:22")
            .unwrap()
            .auth("user", None)
            .build();
        let _ = Client::with_transport(tp);

        let _ = Client::simple_http("localhost:22", None, None).unwrap();
    }

    #[cfg(feature = "proxy")]
    #[test]
    fn construct_with_proxy() {
        let tp = Builder::new()
            .timeout(Duration::from_millis(100))
            .url("localhost:22")
            .unwrap()
            .auth("user", None)
            .proxy_addr("127.0.0.1:9050")
            .unwrap()
            .build();
        let _ = Client::with_transport(tp);

        let _ = Client::http_proxy(
            "localhost:22",
            None,
            None,
            "127.0.0.1:9050",
            Some(("user", "password")),
        )
        .unwrap();
    }

    /// Test that the client will detect that a socket is closed and open a fresh one before sending
    /// the request
    #[cfg(all(not(feature = "proxy"), not(jsonrpc_fuzz)))]
    #[test]
    fn request_to_closed_socket() {
        use serde_json::{Number, Value};
        use std::net::{Shutdown, TcpListener};
        use std::sync::mpsc;
        use std::thread;

        let (tx, rx) = mpsc::sync_channel(1);

        thread::spawn(move || {
            let server = TcpListener::bind("localhost:0").expect("Binding a Tcp Listener");
            tx.send(server.local_addr().unwrap().port()).unwrap();
            for (request_id, stream) in server.incoming().enumerate() {
                let mut stream = stream.unwrap();

                let buf_reader = BufReader::new(&mut stream);

                let _http_request: Vec<_> = buf_reader
                    .lines()
                    .map(|result| result.unwrap())
                    .take_while(|line| !line.is_empty())
                    .collect();

                let response = Response {
                    result: None,
                    error: None,
                    id: Value::Number(Number::from(request_id)),
                    jsonrpc: Some(String::from("2.0")),
                };
                let response_str = serde_json::to_string(&response).unwrap();

                stream.write_all(b"HTTP/1.1 200\r\n").unwrap();
                stream.write_all(b"Content-Length: ").unwrap();
                stream.write_all(response_str.len().to_string().as_bytes()).unwrap();
                stream.write_all(b"\r\n").unwrap();
                stream.write_all(b"\r\n").unwrap();
                stream.write_all(response_str.as_bytes()).unwrap();
                stream.flush().unwrap();

                stream.shutdown(Shutdown::Both).unwrap();
            }
        });

        // Give the server thread a second to start up and listen
        thread::sleep(Duration::from_secs(1));

        let port = rx.recv().unwrap();
        let client =
            Client::simple_http(format!("localhost:{}", port).as_str(), None, None).unwrap();
        let request = client.build_request("test_request", &[]);
        let result = client.send_request(request).unwrap();
        assert_eq!(result.id, Value::Number(Number::from(0)));
        thread::sleep(Duration::from_secs(1));
        let request = client.build_request("test_request2", &[]);
        let result2 = client.send_request(request)
            .expect("This second request should not be an Err like `Err(Transport(HttpResponseTooShort { actual: 0, needed: 12 }))`");
        assert_eq!(result2.id, Value::Number(Number::from(1)));
    }
}
