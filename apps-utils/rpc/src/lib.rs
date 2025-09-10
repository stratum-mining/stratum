pub mod mini_rpc_client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Hash([u8; 32]);

#[allow(dead_code)]
#[derive(Clone, Deserialize)]
pub struct Amount(f64);

#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockHash(Hash);

pub use hyper::Uri;
