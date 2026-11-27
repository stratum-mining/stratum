//! `mining.authorize`.

use crate::{error::ParamError, utils::params_array_mut};
use serde_json::Value;
use std::{fmt, mem::take};

/// `mining.authorize`.
#[derive(Clone, PartialEq, Eq)]
pub struct Authorize {
    pub worker_name: String,
    /// Empty when the miner omitted it or sent `null`.
    pub password: String,
}

impl Authorize {
    pub(super) fn from_params(params: &mut Value) -> Result<Self, ParamError> {
        let (worker_name, password) = match params_array_mut(params)? {
            [Value::String(name), Value::String(password)] => (take(name), take(password)),
            [Value::String(name)] | [Value::String(name), Value::Null] => {
                (take(name), String::new())
            }
            _ => return Err(ParamError::new("params", "unexpected authorize params")),
        };
        Ok(Authorize {
            worker_name,
            password,
        })
    }

    pub(super) fn to_params(&self) -> Value {
        Value::from(vec![self.worker_name.as_str(), self.password.as_str()])
    }
}

impl fmt::Debug for Authorize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Authorize")
            .field("worker_name", &self.worker_name)
            .field("password", &format_args!("<redacted>"))
            .finish()
    }
}
