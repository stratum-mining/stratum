//! `mining.submit`.

use super::Response;
use crate::{
    error::ParamError,
    json_rpc::{Id, RpcError},
    utils::{hex_decode, hex_u32, integer_u32, params_array_mut, Extranonce, HexU32Be},
};
use serde_json::Value;
use std::{convert::TryFrom, mem::take};

/// `mining.submit`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Submit {
    pub worker_name: String,
    pub job_id: String,
    pub extranonce2: Extranonce,
    pub ntime: HexU32Be,
    pub nonce: HexU32Be,
    /// Present when version rolling is negotiated.
    pub version_bits: Option<HexU32Be>,
}

impl Submit {
    /// `ntime` and `nonce` are hex strings, or both JSON integers; short hex is left-padded.
    pub(super) fn from_params(params: &mut Value) -> Result<Self, ParamError> {
        let params = params_array_mut(params)?;
        let (worker_name, job_id, extranonce2, ntime, nonce, version_bits) = match params {
            [Value::String(worker), Value::String(job), Value::String(extranonce2), ntime, nonce, rest @ ..]
                if rest.len() <= 1 =>
            {
                (worker, job, extranonce2, ntime, nonce, rest.first())
            }
            _ => return Err(ParamError::new("params", "unexpected submit params")),
        };
        let (ntime, nonce) = match (ntime, nonce) {
            (Value::String(ntime), Value::String(nonce)) => {
                (hex_u32(ntime, "ntime")?, hex_u32(nonce, "nonce")?)
            }
            (Value::Number(ntime), Value::Number(nonce)) => {
                (integer_u32(ntime, "ntime")?, integer_u32(nonce, "nonce")?)
            }
            _ => return Err(ParamError::new("params", "ntime and nonce differ in type")),
        };
        let version_bits = match version_bits {
            None => None,
            Some(Value::String(bits)) => Some(hex_u32(bits, "version_bits")?),
            Some(_) => return Err(ParamError::new("version_bits", "not a string")),
        };
        let extranonce2 = hex_decode(extranonce2)
            .and_then(Extranonce::try_from)
            .map_err(|_| ParamError::new("extranonce2", "not hex of at most 32 bytes"))?;
        Ok(Submit {
            worker_name: take(worker_name),
            job_id: take(job_id),
            extranonce2,
            ntime,
            nonce,
            version_bits,
        })
    }

    pub(super) fn to_params(&self) -> Value {
        let mut params: Vec<Value> = vec![
            self.worker_name.clone().into(),
            self.job_id.clone().into(),
            self.extranonce2.clone().into(),
            self.ntime.into(),
            self.nonce.into(),
        ];
        if let Some(bits) = &self.version_bits {
            params.push((*bits).into());
        }
        params.into()
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
    pub fn into_response(self, id: Id) -> Response<bool> {
        match self {
            SubmitOutcome::Accepted => Response::success(id, true),
            SubmitOutcome::Rejected(error) => Response::failure(id, error.into()),
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

    /// The error with this code, or `None` when the code is not a standard one.
    pub const fn from_code(code: i32) -> Option<Self> {
        Some(match code {
            20 => Self::Other,
            21 => Self::JobNotFound,
            22 => Self::DuplicateShare,
            23 => Self::LowDifficultyShare,
            24 => Self::UnauthorizedWorker,
            25 => Self::NotSubscribed,
            _ => return None,
        })
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

impl From<SubmitError> for RpcError {
    fn from(error: SubmitError) -> Self {
        RpcError {
            code: error.code(),
            message: error.message().to_string(),
            data: None,
        }
    }
}
