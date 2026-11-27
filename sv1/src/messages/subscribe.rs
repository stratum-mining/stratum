//! `mining.subscribe` and `mining.extranonce.subscribe`.

use crate::{
    error::ParamError,
    utils::{extranonce2_size_param, params_array_mut, Extranonce},
};
use serde::{de, Deserialize, Deserializer};
use serde_json::Value;
use std::{convert::TryFrom, mem::take};

/// `mining.subscribe`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Subscribe {
    /// Empty when the miner sent no user agent.
    pub agent: String,
    /// A subscription the miner asks to resume. The server decides whether to honour it.
    pub subscription_id: Option<String>,
}

/// `mining.extranonce.subscribe`: the client supports `mining.set_extranonce`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExtranonceSubscribe;

impl Subscribe {
    pub(super) fn from_params(params: &mut Value) -> Result<Self, ParamError> {
        let (agent, subscription_id) = match params_array_mut(params)? {
            [] => (String::new(), None),
            [Value::String(agent)] | [Value::String(agent), Value::Null] => (take(agent), None),
            // bosminer
            [Value::String(agent), Value::Null, Value::String(_), Value::Null] => {
                (take(agent), None)
            }
            [Value::String(agent), Value::String(id)] => (take(agent), Some(take(id))),
            _ => return Err(ParamError::new("params", "unexpected subscribe params")),
        };
        Ok(Subscribe {
            agent,
            subscription_id,
        })
    }

    pub(super) fn to_params(&self) -> Value {
        match (self.agent.as_str(), &self.subscription_id) {
            ("", None) => Value::Array(vec![]),
            (agent, None) => Value::from(vec![agent]),
            (agent, Some(id)) => Value::from(vec![agent, id.as_str()]),
        }
    }
}

/// Result of `mining.subscribe`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubscribeResult {
    /// Pairs of notification method and subscription ID.
    pub subscriptions: Vec<(String, String)>,
    pub extranonce1: Extranonce,
    /// At most 32. A larger size is refused when decoding, and a `mining.submit` could not carry
    /// an extranonce2 of that size.
    pub extranonce2_size: usize,
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

impl<'de> Deserialize<'de> for SubscribeResult {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        SubscribeResult::from_value(&Value::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}
