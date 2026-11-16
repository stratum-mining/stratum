//! Notifications a server sends to a client.

use crate::{
    error::{Error, ParamError},
    json_rpc::{Message, Method},
    utils::{
        extranonce2_size_param, hex_decode, hex_u32, params_array, params_array_mut, Extranonce,
        HexBytes, HexU32Be, MerkleNode, PrevHash, MAX_EXTRANONCE_LEN, VERSION_ROLLING_MASK,
    },
};
use serde_json::Value;
use std::{convert::TryFrom, mem::take};

/// `mining.notify`: a new job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notify {
    pub job_id: String,
    pub prev_hash: PrevHash,
    pub coinbase1: HexBytes,
    pub coinbase2: HexBytes,
    pub merkle_branch: Vec<MerkleNode>,
    pub version: HexU32Be,
    pub nbits: HexU32Be,
    pub ntime: HexU32Be,
    /// The miner should abandon its current work.
    pub clean_jobs: bool,
}

/// `mining.set_difficulty`, applied from the next job.
#[derive(Clone, Debug, PartialEq)]
pub struct SetDifficulty {
    difficulty: f64,
}

/// `mining.set_extranonce`, applied from the next job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetExtranonce {
    pub extranonce1: Extranonce,
    pub extranonce2_size: usize,
}

/// `mining.set_version_mask`, applied immediately.
///
/// The mask is clamped to [`VERSION_ROLLING_MASK`] when decoding and when encoding, so reserved
/// bits are never offered or accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetVersionMask {
    pub mask: HexU32Be,
}

impl From<Notify> for Message {
    fn from(notify: Notify) -> Self {
        let merkle_branch: Vec<Value> = notify
            .merkle_branch
            .into_iter()
            .map(|node| String::from(node).into())
            .collect();
        Message::notification(
            Method::Notify,
            vec![
                notify.job_id.into(),
                notify.prev_hash.into(),
                notify.coinbase1.into(),
                notify.coinbase2.into(),
                merkle_branch.into(),
                notify.version.into(),
                notify.nbits.into(),
                notify.ntime.into(),
                notify.clean_jobs.into(),
            ],
        )
    }
}

impl From<SetDifficulty> for Message {
    fn from(set: SetDifficulty) -> Self {
        Message::notification(Method::SetDifficulty, vec![set.difficulty.into()])
    }
}

impl From<SetExtranonce> for Message {
    fn from(set: SetExtranonce) -> Self {
        Message::notification(
            Method::SetExtranonce,
            vec![set.extranonce1.into(), set.extranonce2_size.into()],
        )
    }
}

impl From<SetVersionMask> for Message {
    fn from(set: SetVersionMask) -> Self {
        Message::notification(
            Method::SetVersionMask,
            vec![(set.mask & VERSION_ROLLING_MASK).into()],
        )
    }
}

/// A block's merkle branch has at most one entry per level of a 2^32-transaction tree.
const MAX_MERKLE_BRANCH_LEN: usize = 32;

impl Notify {
    pub(super) fn from_params(params: &mut Value) -> Result<Self, ParamError> {
        let [Value::String(job_id), Value::String(prev_hash), Value::String(coinbase1), Value::String(coinbase2), Value::Array(merkle_branch), Value::String(version), Value::String(nbits), Value::String(ntime), Value::Bool(clean_jobs)] =
            params_array_mut(params)?
        else {
            return Err(ParamError::new("params", "unexpected notify params"));
        };
        if merkle_branch.len() > MAX_MERKLE_BRANCH_LEN {
            return Err(ParamError::new("merkle_branch", "longer than 32 entries"));
        }
        let merkle_branch = merkle_branch
            .iter()
            .map(|node| {
                node.as_str()
                    .and_then(|node| MerkleNode::try_from(node).ok())
                    .ok_or(ParamError::new("merkle_branch", "not 32 bytes of hex"))
            })
            .collect::<Result<_, _>>()?;
        Ok(Notify {
            job_id: take(job_id),
            prev_hash: PrevHash::try_from(prev_hash.as_str())
                .map_err(|_| ParamError::new("prev_hash", "not 32 bytes of hex"))?,
            coinbase1: HexBytes::try_from(coinbase1.as_str())
                .map_err(|_| ParamError::new("coinbase1", "not hex"))?,
            coinbase2: HexBytes::try_from(coinbase2.as_str())
                .map_err(|_| ParamError::new("coinbase2", "not hex"))?,
            merkle_branch,
            version: hex_u32(version, "version")?,
            nbits: hex_u32(nbits, "nbits")?,
            ntime: hex_u32(ntime, "ntime")?,
            clean_jobs: *clean_jobs,
        })
    }
}

impl SetDifficulty {
    /// Fails unless the difficulty is finite and positive, the only kind a miner can work with.
    pub fn new(difficulty: f64) -> Result<Self, Error> {
        if difficulty.is_finite() && difficulty > 0.0 {
            Ok(SetDifficulty { difficulty })
        } else {
            Err(Error::InvalidDifficulty(difficulty))
        }
    }

    pub fn difficulty(&self) -> f64 {
        self.difficulty
    }

    pub(super) fn from_params(params: &Value) -> Result<Self, ParamError> {
        match params_array(params)? {
            [difficulty] => difficulty
                .as_f64()
                .and_then(|difficulty| SetDifficulty::new(difficulty).ok())
                .ok_or(ParamError::new("difficulty", "not a positive number")),
            _ => Err(ParamError::new(
                "params",
                "unexpected set_difficulty params",
            )),
        }
    }
}

impl SetExtranonce {
    pub(super) fn from_params(params: &Value) -> Result<Self, ParamError> {
        let [Value::String(extranonce1), Value::Number(extranonce2_size)] = params_array(params)?
        else {
            return Err(ParamError::new(
                "params",
                "unexpected set_extranonce params",
            ));
        };
        if extranonce1.len() > 2 * MAX_EXTRANONCE_LEN {
            return Err(ParamError::new(
                "extranonce1",
                "not hex of at most 32 bytes",
            ));
        }
        Ok(SetExtranonce {
            extranonce1: hex_decode(extranonce1)
                .and_then(Extranonce::try_from)
                .map_err(|_| ParamError::new("extranonce1", "not hex of at most 32 bytes"))?,
            extranonce2_size: extranonce2_size_param(extranonce2_size)?,
        })
    }
}

impl SetVersionMask {
    pub(super) fn from_params(params: &Value) -> Result<Self, ParamError> {
        match params_array(params)? {
            [Value::String(mask)] => Ok(SetVersionMask {
                mask: hex_u32(mask, "mask")? & VERSION_ROLLING_MASK,
            }),
            _ => Err(ParamError::new(
                "params",
                "unexpected set_version_mask params",
            )),
        }
    }
}
