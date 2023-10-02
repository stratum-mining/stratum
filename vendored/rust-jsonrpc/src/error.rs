// SPDX-License-Identifier: CC0-1.0

//! # Error handling
//!
//! Some useful methods for creating Error objects.

use std::{error, fmt};

use serde::{Deserialize, Serialize};

use crate::Response;

/// A library error
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// A transport error
    Transport(Box<dyn error::Error + Send + Sync>),
    /// Json error
    Json(serde_json::Error),
    /// Error response
    Rpc(RpcError),
    /// Response to a request did not have the expected nonce
    NonceMismatch,
    /// Response to a request had a jsonrpc field other than "2.0"
    VersionMismatch,
    /// Batches can't be empty
    EmptyBatch,
    /// Too many responses returned in batch
    WrongBatchResponseSize,
    /// Batch response contained a duplicate ID
    BatchDuplicateResponseId(serde_json::Value),
    /// Batch response contained an ID that didn't correspond to any request ID
    WrongBatchResponseId(serde_json::Value),
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Error {
        Error::Json(e)
    }
}

impl From<RpcError> for Error {
    fn from(e: RpcError) -> Error {
        Error::Rpc(e)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use Error::*;

        match *self {
            Transport(ref e) => write!(f, "transport error: {}", e),
            Json(ref e) => write!(f, "JSON decode error: {}", e),
            Rpc(ref r) => write!(f, "RPC error response: {:?}", r),
            BatchDuplicateResponseId(ref v) => write!(f, "duplicate RPC batch response ID: {}", v),
            WrongBatchResponseId(ref v) => write!(f, "wrong RPC batch response ID: {}", v),
            NonceMismatch => write!(f, "nonce of response did not match nonce of request"),
            VersionMismatch => write!(f, "`jsonrpc` field set to non-\"2.0\""),
            EmptyBatch => write!(f, "batches can't be empty"),
            WrongBatchResponseSize => write!(f, "too many responses returned in batch"),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        use self::Error::*;

        match *self {
            Rpc(_)
            | NonceMismatch
            | VersionMismatch
            | EmptyBatch
            | WrongBatchResponseSize
            | BatchDuplicateResponseId(_)
            | WrongBatchResponseId(_) => None,
            Transport(ref e) => Some(&**e),
            Json(ref _e) => todo!()//Some(e),
        }
    }
}

/// Standard error responses, as described at at
/// <http://www.jsonrpc.org/specification#error_object>
///
/// # Documentation Copyright
/// Copyright (C) 2007-2010 by the JSON-RPC Working Group
///
/// This document and translations of it may be used to implement JSON-RPC, it
/// may be copied and furnished to others, and derivative works that comment
/// on or otherwise explain it or assist in its implementation may be prepared,
/// copied, published and distributed, in whole or in part, without restriction
/// of any kind, provided that the above copyright notice and this paragraph
/// are included on all such copies and derivative works. However, this document
/// itself may not be modified in any way.
///
/// The limited permissions granted above are perpetual and will not be revoked.
///
/// This document and the information contained herein is provided "AS IS" and
/// ALL WARRANTIES, EXPRESS OR IMPLIED are DISCLAIMED, INCLUDING BUT NOT LIMITED
/// TO ANY WARRANTY THAT THE USE OF THE INFORMATION HEREIN WILL NOT INFRINGE ANY
/// RIGHTS OR ANY IMPLIED WARRANTIES OF MERCHANTABILITY OR FITNESS FOR A
/// PARTICULAR PURPOSE.
///
#[derive(Debug)]
pub enum StandardError {
    /// Invalid JSON was received by the server.
    /// An error occurred on the server while parsing the JSON text.
    ParseError,
    /// The JSON sent is not a valid Request object.
    InvalidRequest,
    /// The method does not exist / is not available.
    MethodNotFound,
    /// Invalid method parameter(s).
    InvalidParams,
    /// Internal JSON-RPC error.
    InternalError,
}

/// A JSONRPC error object
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RpcError {
    /// The integer identifier of the error
    pub code: i32,
    /// A string describing the error
    pub message: String,
    /// Additional data specific to the error
    pub data: Option<Box<serde_json::value::RawValue>>,
}

/// Create a standard error responses
pub fn standard_error(
    code: StandardError,
    data: Option<Box<serde_json::value::RawValue>>,
) -> RpcError {
    match code {
        StandardError::ParseError => RpcError {
            code: -32700,
            message: "Parse error".to_string(),
            data,
        },
        StandardError::InvalidRequest => RpcError {
            code: -32600,
            message: "Invalid Request".to_string(),
            data,
        },
        StandardError::MethodNotFound => RpcError {
            code: -32601,
            message: "Method not found".to_string(),
            data,
        },
        StandardError::InvalidParams => RpcError {
            code: -32602,
            message: "Invalid params".to_string(),
            data,
        },
        StandardError::InternalError => RpcError {
            code: -32603,
            message: "Internal error".to_string(),
            data,
        },
    }
}

/// Converts a Rust `Result` to a JSONRPC response object
pub fn result_to_response(
    result: Result<serde_json::Value, RpcError>,
    id: serde_json::Value,
) -> Response {
    match result {
        Ok(data) => Response {
            result: Some(
                serde_json::value::RawValue::from_string(serde_json::to_string(&data).unwrap())
                    .unwrap(),
            ),
            error: None,
            id,
            jsonrpc: Some(String::from("2.0")),
        },
        Err(err) => Response {
            result: None,
            error: Some(err),
            id,
            jsonrpc: Some(String::from("2.0")),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::StandardError::{
        InternalError, InvalidParams, InvalidRequest, MethodNotFound, ParseError,
    };
    use super::{result_to_response, standard_error};
    use serde_json;

    #[test]
    fn test_parse_error() {
        let resp = result_to_response(Err(standard_error(ParseError, None)), From::from(1));
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());
        assert_eq!(resp.id, serde_json::Value::from(1));
        assert_eq!(resp.error.unwrap().code, -32700);
    }

    #[test]
    fn test_invalid_request() {
        let resp = result_to_response(Err(standard_error(InvalidRequest, None)), From::from(1));
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());
        assert_eq!(resp.id, serde_json::Value::from(1));
        assert_eq!(resp.error.unwrap().code, -32600);
    }

    #[test]
    fn test_method_not_found() {
        let resp = result_to_response(Err(standard_error(MethodNotFound, None)), From::from(1));
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());
        assert_eq!(resp.id, serde_json::Value::from(1));
        assert_eq!(resp.error.unwrap().code, -32601);
    }

    #[test]
    fn test_invalid_params() {
        let resp = result_to_response(Err(standard_error(InvalidParams, None)), From::from("123"));
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());
        assert_eq!(resp.id, serde_json::Value::from("123"));
        assert_eq!(resp.error.unwrap().code, -32602);
    }

    #[test]
    fn test_internal_error() {
        let resp = result_to_response(Err(standard_error(InternalError, None)), From::from(-1));
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());
        assert_eq!(resp.id, serde_json::Value::from(-1));
        assert_eq!(resp.error.unwrap().code, -32603);
    }
}
