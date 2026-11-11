//! Messages a server sends to a client: notifications, and the results of client requests.
//!
//! `mining.authorize`, `mining.submit` and `mining.extranonce.subscribe` return a plain boolean.

use crate::{
    error::{DecodeError, DecodeErrorKind, ParamError},
    json_rpc::{self, Id, Message, Method, Response, RpcError},
    utils::{
        extranonce2_size_param, extranonce_from_hex, hex_u32, min_bit_count, params_array,
        Extranonce, HexBytes, HexU32Be, MerkleNode, PrevHash, VERSION_ROLLING_MASK,
    },
};
use serde::{de, Deserialize, Deserializer};
use serde_json::{Map, Value};
use std::convert::TryFrom;

#[derive(Clone, Debug, PartialEq)]
pub enum ServerNotification {
    Notify(Notify),
    SetDifficulty(SetDifficulty),
    SetExtranonce(SetExtranonce),
    SetVersionMask(SetVersionMask),
}

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
    /// Must be finite and positive to be sent.
    pub difficulty: f64,
}

/// `mining.set_extranonce`, applied from the next job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetExtranonce {
    pub extranonce1: Extranonce,
    pub extranonce2_size: usize,
}

/// `mining.set_version_mask`, applied immediately.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetVersionMask {
    pub mask: HexU32Be,
}

/// A response is an error: only notifications are expected.
impl TryFrom<Message> for ServerNotification {
    type Error = DecodeError;

    fn try_from(message: Message) -> Result<Self, DecodeError> {
        match message {
            Message::Request(request) => ServerNotification::try_from(request),
            Message::Response(response) => Err(DecodeError::about(
                response.id,
                None,
                DecodeErrorKind::UnexpectedResponse,
            )),
        }
    }
}

/// Any request ID is ignored, so a server that attaches one to a notification is still understood.
impl TryFrom<json_rpc::Request> for ServerNotification {
    type Error = DecodeError;

    fn try_from(request: json_rpc::Request) -> Result<Self, DecodeError> {
        let error =
            |kind| DecodeError::about(request.id.clone(), Some(request.method.clone()), kind);
        let params = &request.params;
        let parsed = match &request.method {
            Method::Notify => Notify::from_params(params).map(ServerNotification::Notify),
            Method::SetDifficulty => {
                SetDifficulty::from_params(params).map(ServerNotification::SetDifficulty)
            }
            Method::SetExtranonce => {
                SetExtranonce::from_params(params).map(ServerNotification::SetExtranonce)
            }
            Method::SetVersionMask => {
                SetVersionMask::from_params(params).map(ServerNotification::SetVersionMask)
            }
            _ => return Err(error(DecodeErrorKind::UnknownMethod)),
        };
        parsed.map_err(|e| error(e.into()))
    }
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
    /// # Panics
    ///
    /// If the difficulty is not finite and positive, which no miner can work with.
    fn from(set: SetDifficulty) -> Self {
        assert!(
            set.difficulty.is_finite() && set.difficulty > 0.0,
            "mining.set_difficulty must be finite and positive, got {}",
            set.difficulty
        );
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
        Message::notification(Method::SetVersionMask, vec![set.mask.into()])
    }
}

impl From<ServerNotification> for Message {
    fn from(notification: ServerNotification) -> Self {
        match notification {
            ServerNotification::Notify(notify) => notify.into(),
            ServerNotification::SetDifficulty(set) => set.into(),
            ServerNotification::SetExtranonce(set) => set.into(),
            ServerNotification::SetVersionMask(set) => set.into(),
        }
    }
}

/// A block's merkle branch has at most one entry per level of a 2^32-transaction tree.
const MAX_MERKLE_BRANCH_LEN: usize = 32;

impl Notify {
    fn from_params(params: &Value) -> Result<Self, ParamError> {
        let [Value::String(job_id), Value::String(prev_hash), Value::String(coinbase1), Value::String(coinbase2), Value::Array(merkle_branch), Value::String(version), Value::String(nbits), Value::String(ntime), Value::Bool(clean_jobs)] =
            params_array(params)?
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
            job_id: job_id.clone(),
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
    fn from_params(params: &Value) -> Result<Self, ParamError> {
        match params_array(params)? {
            [difficulty] => difficulty
                .as_f64()
                .map(|difficulty| SetDifficulty { difficulty })
                .ok_or(ParamError::new("difficulty", "not a number")),
            _ => Err(ParamError::new(
                "params",
                "unexpected set_difficulty params",
            )),
        }
    }
}

impl SetExtranonce {
    fn from_params(params: &Value) -> Result<Self, ParamError> {
        let [Value::String(extranonce1), Value::Number(extranonce2_size)] = params_array(params)?
        else {
            return Err(ParamError::new(
                "params",
                "unexpected set_extranonce params",
            ));
        };
        Ok(SetExtranonce {
            extranonce1: extranonce_from_hex(extranonce1)
                .map_err(|_| ParamError::new("extranonce1", "not hex of at most 32 bytes"))?,
            extranonce2_size: extranonce2_size_param(extranonce2_size)?,
        })
    }
}

impl SetVersionMask {
    fn from_params(params: &Value) -> Result<Self, ParamError> {
        match params_array(params)? {
            [Value::String(mask)] => Ok(SetVersionMask {
                mask: hex_u32(mask, "mask")?,
            }),
            _ => Err(ParamError::new(
                "params",
                "unexpected set_version_mask params",
            )),
        }
    }
}

/// Result of `mining.subscribe`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubscribeResult {
    /// Pairs of notification method and subscription ID.
    pub subscriptions: Vec<(String, String)>,
    pub extranonce1: Extranonce,
    pub extranonce2_size: usize,
}

/// Result of `mining.configure`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConfigureResult {
    pub version_rolling: Option<VersionRollingResult>,
    pub minimum_difficulty: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionRollingResult {
    pub enabled: bool,
    pub mask: HexU32Be,
    pub min_bit_count: u32,
}

impl VersionRollingResult {
    /// Enables version rolling, clamping the mask to [`VERSION_ROLLING_MASK`] so it never covers
    /// reserved bits.
    pub fn new(mask: HexU32Be, min_bit_count: u32) -> Self {
        VersionRollingResult {
            enabled: true,
            mask: HexU32Be(mask.0 & VERSION_ROLLING_MASK),
            min_bit_count,
        }
    }
}

impl From<SubscribeResult> for Value {
    fn from(result: SubscribeResult) -> Self {
        let subscriptions: Vec<Value> = result
            .subscriptions
            .into_iter()
            .map(|(method, id)| Value::from(vec![method, id]))
            .collect();
        Value::Array(vec![
            subscriptions.into(),
            result.extranonce1.into(),
            result.extranonce2_size.into(),
        ])
    }
}

impl From<ConfigureResult> for Value {
    fn from(result: ConfigureResult) -> Self {
        let mut values = Map::new();
        if let Some(version_rolling) = result.version_rolling {
            values.insert("version-rolling".into(), version_rolling.enabled.into());
            values.insert("version-rolling.mask".into(), version_rolling.mask.into());
            values.insert(
                "version-rolling.min-bit-count".into(),
                HexU32Be(version_rolling.min_bit_count).into(),
            );
        }
        if let Some(minimum_difficulty) = result.minimum_difficulty {
            values.insert("minimum-difficulty".into(), minimum_difficulty.into());
        }
        values.into()
    }
}

impl SubscribeResult {
    fn from_value(result: &Value) -> Result<Self, ParamError> {
        let Some([Value::Array(subscriptions), Value::String(extranonce1), Value::Number(size)]) =
            result.as_array().map(Vec::as_slice)
        else {
            return Err(ParamError::new("result", "unexpected subscribe result"));
        };
        let subscriptions = subscriptions
            .iter()
            .map(
                |subscription| match subscription.as_array().map(Vec::as_slice) {
                    Some([Value::String(method), Value::String(id)]) => {
                        Ok((method.clone(), id.clone()))
                    }
                    _ => Err(ParamError::new("subscriptions", "not a pair of strings")),
                },
            )
            .collect::<Result<_, _>>()?;
        Ok(SubscribeResult {
            subscriptions,
            extranonce1: Extranonce::try_from(extranonce1.as_str())
                .map_err(|_| ParamError::new("extranonce1", "not hex of at most 32 bytes"))?,
            extranonce2_size: extranonce2_size_param(size)?,
        })
    }
}

impl ConfigureResult {
    /// Version rolling needs both `version-rolling` and `version-rolling.mask`; a missing
    /// `version-rolling.min-bit-count` reads as zero.
    fn from_value(result: &Value) -> Result<Self, ParamError> {
        let values = result
            .as_object()
            .ok_or(ParamError::new("result", "not an object"))?;
        let enabled = values.get("version-rolling");
        let mask = values.get("version-rolling.mask");
        let count = values.get("version-rolling.min-bit-count");
        let version_rolling = match (enabled, mask) {
            (Some(enabled), Some(mask)) => Some(VersionRollingResult {
                enabled: enabled
                    .as_bool()
                    .ok_or(ParamError::new("version-rolling", "not a boolean"))?,
                mask: mask
                    .as_str()
                    .and_then(|mask| HexU32Be::try_from(mask).ok())
                    .ok_or(ParamError::new(
                        "version-rolling.mask",
                        "not a 32-bit hex value",
                    ))?,
                min_bit_count: count
                    .map(|count| min_bit_count(count, "version-rolling.min-bit-count"))
                    .transpose()?
                    .unwrap_or(0),
            }),
            (None, None) if count.is_none() => None,
            _ => return Err(ParamError::new("version-rolling", "incomplete")),
        };
        let minimum_difficulty = values
            .get("minimum-difficulty")
            .map(|value| {
                value
                    .as_bool()
                    .ok_or(ParamError::new("minimum-difficulty", "not a boolean"))
            })
            .transpose()?;
        Ok(ConfigureResult {
            version_rolling,
            minimum_difficulty,
        })
    }
}

impl<'de> Deserialize<'de> for SubscribeResult {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        SubscribeResult::from_value(&Value::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for ConfigureResult {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        ConfigureResult::from_value(&Value::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

/// Result of validating a `mining.submit` request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitOutcome {
    Accepted,
    Rejected(SubmitError),
}

/// Standard Stratum V1 share-submission errors.
///
/// These codes originate from the original Stratum mining protocol and remain the format used by
/// production SV1 servers such as CKPool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitError {
    Other,
    JobNotFound,
    DuplicateShare,
    LowDifficultyShare,
    UnauthorizedWorker,
    NotSubscribed,
}

impl SubmitOutcome {
    /// `result: true` on acceptance; `result: null` with the standard error on rejection.
    pub fn into_response(self, id: Id) -> Response {
        match self {
            SubmitOutcome::Accepted => Response::success(id, true),
            SubmitOutcome::Rejected(error) => Response::failure(
                Some(id),
                RpcError {
                    code: error.code(),
                    message: error.message().to_string(),
                    data: None,
                },
            ),
        }
    }
}

impl SubmitError {
    pub const fn code(self) -> i32 {
        match self {
            Self::Other => 20,
            Self::JobNotFound => 21,
            Self::DuplicateShare => 22,
            Self::LowDifficultyShare => 23,
            Self::UnauthorizedWorker => 24,
            Self::NotSubscribed => 25,
        }
    }

    pub const fn message(self) -> &'static str {
        match self {
            Self::Other => "Other/Unknown",
            Self::JobNotFound => "Job not found",
            Self::DuplicateShare => "Duplicate share",
            Self::LowDifficultyShare => "Low difficulty share",
            Self::UnauthorizedWorker => "Unauthorized worker",
            Self::NotSubscribed => "Not subscribed",
        }
    }
}
