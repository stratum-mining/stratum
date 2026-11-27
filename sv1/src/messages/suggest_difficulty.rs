//! `mining.suggest_difficulty`.

use serde_json::Value;

/// `mining.suggest_difficulty`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SuggestDifficulty {
    /// `None` when the params hold no usable difficulty. Servers are not required to honour it.
    /// A difficulty that is not finite and non-negative is not sent.
    pub difficulty: Option<f64>,
}

impl SuggestDifficulty {
    pub(super) fn from_params(params: &Value) -> Self {
        let difficulty = match params.as_array().map(Vec::as_slice) {
            Some([difficulty]) => difficulty.as_f64().filter(|d| d.is_finite() && *d >= 0.0),
            _ => None,
        };
        SuggestDifficulty { difficulty }
    }

    pub(super) fn to_params(&self) -> Value {
        let difficulty = self.difficulty.filter(|d| d.is_finite() && *d >= 0.0);
        Value::Array(difficulty.into_iter().map(Value::from).collect())
    }
}
