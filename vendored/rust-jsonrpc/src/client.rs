// SPDX-License-Identifier: CC0-1.0

//! # Client support
//!
//! Support for connecting to JSONRPC servers over HTTP, sending requests,
//! and parsing responses

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::atomic;

use serde_json::value::RawValue;
use serde_json::Value;

use crate::error::Error;
use crate::{Request, Response};

/// An interface for a transport over which to use the JSONRPC protocol.
pub trait Transport: Send + Sync + 'static {
    /// Sends an RPC request over the transport.
    fn send_request(&self, _: Request) -> Result<Response, Error>;
    /// Sends a batch of RPC requests over the transport.
    fn send_batch(&self, _: &[Request]) -> Result<Vec<Response>, Error>;
    /// Formats the target of this transport. I.e. the URL/socket/...
    fn fmt_target(&self, f: &mut fmt::Formatter) -> fmt::Result;
}

/// A JSON-RPC client.
///
/// Creates a new Client using one of the transport-specific constructors e.g.,
/// [`Client::simple_http`] for a bare-minimum HTTP transport.
pub struct Client {
    pub(crate) transport: Box<dyn Transport>,
    nonce: atomic::AtomicUsize,
}

impl Client {
    /// Creates a new client with the given transport.
    pub fn with_transport<T: Transport>(transport: T) -> Client {
        Client {
            transport: Box::new(transport),
            nonce: atomic::AtomicUsize::new(1),
        }
    }

    /// Builds a request.
    ///
    /// To construct the arguments, one can use one of the shorthand methods
    /// [`crate::arg`] or [`crate::try_arg`].
    pub fn build_request<'a>(&self, method: &'a str, params: &'a [Box<RawValue>]) -> Request<'a> {
        let nonce = self.nonce.fetch_add(1, atomic::Ordering::Relaxed);
        Request {
            method,
            params,
            id: serde_json::Value::from(nonce),
            jsonrpc: Some("2.0"),
        }
    }

    /// Sends a request to a client.
    pub fn send_request(&self, request: Request) -> Result<Response, Error> {
        self.transport.send_request(request)
    }

    /// Sends a batch of requests to the client.
    ///
    /// Note that the requests need to have valid IDs, so it is advised to create the requests
    /// with [`Client::build_request`].
    ///
    /// # Returns
    ///
    /// The return vector holds the response for the request at the corresponding index. If no
    /// response was provided, it's [`None`].
    pub fn send_batch(&self, requests: &[Request]) -> Result<Vec<Option<Response>>, Error> {
        if requests.is_empty() {
            return Err(Error::EmptyBatch);
        }

        // If the request body is invalid JSON, the response is a single response object.
        // We ignore this case since we are confident we are producing valid JSON.
        let responses = self.transport.send_batch(requests)?;
        if responses.len() > requests.len() {
            return Err(Error::WrongBatchResponseSize);
        }

        //TODO(stevenroose) check if the server preserved order to avoid doing the mapping

        // First index responses by ID and catch duplicate IDs.
        let mut by_id = HashMap::with_capacity(requests.len());
        for resp in responses.into_iter() {
            let id = HashableValue(Cow::Owned(resp.id.clone()));
            if let Some(dup) = by_id.insert(id, resp) {
                return Err(Error::BatchDuplicateResponseId(dup.id));
            }
        }
        // Match responses to the requests.
        let results =
            requests.iter().map(|r| by_id.remove(&HashableValue(Cow::Borrowed(&r.id)))).collect();

        // Since we're also just producing the first duplicate ID, we can also just produce the
        // first incorrect ID in case there are multiple.
        if let Some(id) = by_id.keys().next() {
            return Err(Error::WrongBatchResponseId((*id.0).clone()));
        }

        Ok(results)
    }

    /// Makes a request and deserializes the response.
    ///
    /// To construct the arguments, one can use one of the shorthand methods
    /// [`crate::arg`] or [`crate::try_arg`].
    pub fn call<R: for<'a> serde::de::Deserialize<'a>>(
        &self,
        method: &str,
        args: &[Box<RawValue>],
    ) -> Result<R, Error> {
        let request = self.build_request(method, args);
        let id = request.id.clone();

        let response = self.send_request(request)?;
        if response.jsonrpc.is_some() && response.jsonrpc != Some(From::from("2.0")) {
            return Err(Error::VersionMismatch);
        }
        if response.id != id {
            return Err(Error::NonceMismatch);
        }

        response.result()
    }
}

impl fmt::Debug for crate::Client {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "jsonrpc::Client(")?;
        self.transport.fmt_target(f)?;
        write!(f, ")")
    }
}

impl<T: Transport> From<T> for Client {
    fn from(t: T) -> Client {
        Client::with_transport(t)
    }
}

/// Newtype around `Value` which allows hashing for use as hashmap keys,
/// this is needed for batch requests.
///
/// The reason `Value` does not support `Hash` or `Eq` by itself
/// is that it supports `f64` values; but for batch requests we
/// will only be hashing the "id" field of the request/response
/// pair, which should never need decimal precision and therefore
/// never use `f64`.
#[derive(Clone, PartialEq, Debug)]
struct HashableValue<'a>(pub Cow<'a, Value>);

impl<'a> Eq for HashableValue<'a> {}

impl<'a> Hash for HashableValue<'a> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match *self.0.as_ref() {
            Value::Null => "null".hash(state),
            Value::Bool(false) => "false".hash(state),
            Value::Bool(true) => "true".hash(state),
            Value::Number(ref n) => {
                "number".hash(state);
                if let Some(n) = n.as_i64() {
                    n.hash(state);
                } else if let Some(n) = n.as_u64() {
                    n.hash(state);
                } else {
                    n.to_string().hash(state);
                }
            }
            Value::String(ref s) => {
                "string".hash(state);
                s.hash(state);
            }
            Value::Array(ref v) => {
                "array".hash(state);
                v.len().hash(state);
                for obj in v {
                    HashableValue(Cow::Borrowed(obj)).hash(state);
                }
            }
            Value::Object(ref m) => {
                "object".hash(state);
                m.len().hash(state);
                for (key, val) in m {
                    key.hash(state);
                    HashableValue(Cow::Borrowed(val)).hash(state);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::borrow::Cow;
    use std::collections::HashSet;
    use std::str::FromStr;
    use std::sync;

    struct DummyTransport;
    impl Transport for DummyTransport {
        fn send_request(&self, _: Request) -> Result<Response, Error> {
            Err(Error::NonceMismatch)
        }
        fn send_batch(&self, _: &[Request]) -> Result<Vec<Response>, Error> {
            Ok(vec![])
        }
        fn fmt_target(&self, _: &mut fmt::Formatter) -> fmt::Result {
            Ok(())
        }
    }

    #[test]
    fn sanity() {
        let client = Client::with_transport(DummyTransport);
        assert_eq!(client.nonce.load(sync::atomic::Ordering::Relaxed), 1);
        let req1 = client.build_request("test", &[]);
        assert_eq!(client.nonce.load(sync::atomic::Ordering::Relaxed), 2);
        let req2 = client.build_request("test", &[]);
        assert_eq!(client.nonce.load(sync::atomic::Ordering::Relaxed), 3);
        assert!(req1.id != req2.id);
    }

    #[test]
    fn hash_value() {
        let val = HashableValue(Cow::Owned(Value::from_str("null").unwrap()));
        let t = HashableValue(Cow::Owned(Value::from_str("true").unwrap()));
        let f = HashableValue(Cow::Owned(Value::from_str("false").unwrap()));
        let ns =
            HashableValue(Cow::Owned(Value::from_str("[0, -0, 123.4567, -100000000]").unwrap()));
        let m =
            HashableValue(Cow::Owned(Value::from_str("{ \"field\": 0, \"field\": -0 }").unwrap()));

        let mut coll = HashSet::new();

        assert!(!coll.contains(&val));
        coll.insert(val.clone());
        assert!(coll.contains(&val));

        assert!(!coll.contains(&t));
        assert!(!coll.contains(&f));
        coll.insert(t.clone());
        assert!(coll.contains(&t));
        assert!(!coll.contains(&f));
        coll.insert(f.clone());
        assert!(coll.contains(&t));
        assert!(coll.contains(&f));

        assert!(!coll.contains(&ns));
        coll.insert(ns.clone());
        assert!(coll.contains(&ns));

        assert!(!coll.contains(&m));
        coll.insert(m.clone());
        assert!(coll.contains(&m));
    }
}
