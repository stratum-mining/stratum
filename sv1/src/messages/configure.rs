//! `mining.configure`, as defined by [BIP 310](https://github.com/bitcoin/bips/blob/master/bip-0310.mediawiki).

use crate::{
    error::ParamError,
    utils::{hex_u32, min_bit_count, params_array, HexU32Be, VERSION_ROLLING_MASK},
};
use serde::{de, Deserialize, Deserializer};
use serde_json::{Map, Value};
use std::{convert::TryFrom, fmt};

/// `mining.configure`, as defined by [BIP 310](https://github.com/bitcoin/bips/blob/master/bip-0310.mediawiki).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Configure {
    pub extensions: Vec<ConfigureExtension>,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ConfigureExtension {
    VersionRolling(VersionRolling),
    MinimumDifficulty(f64),
    SubscribeExtranonce,
    Info(InfoParams),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VersionRolling {
    pub mask: Option<HexU32Be>,
    pub min_bit_count: Option<u32>,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct InfoParams {
    /// May carry credentials, so it is redacted from `Debug`.
    pub connection_url: Option<String>,
    pub hw_id: Option<String>,
    pub hw_version: Option<String>,
    pub sw_version: Option<String>,
}

impl Configure {
    pub fn with_version_rolling(mask: Option<HexU32Be>, min_bit_count: Option<u32>) -> Self {
        Configure {
            extensions: vec![ConfigureExtension::VersionRolling(VersionRolling {
                mask,
                min_bit_count,
            })],
        }
    }

    pub fn version_rolling(&self) -> Option<&VersionRolling> {
        self.extensions
            .iter()
            .find_map(|extension| match extension {
                ConfigureExtension::VersionRolling(params) => Some(params),
                _ => None,
            })
    }

    /// The requested mask without its reserved bits, so it can be granted and checked against
    /// submits as it is. A request without a mask asks for all of [`VERSION_ROLLING_MASK`]. The
    /// mask as it was sent is in [`Configure::version_rolling`].
    pub fn version_rolling_mask(&self) -> Option<HexU32Be> {
        self.version_rolling().map(|params| {
            params.mask.unwrap_or(HexU32Be(VERSION_ROLLING_MASK)) & VERSION_ROLLING_MASK
        })
    }

    /// The requested minimum bit count, defaulting to zero when only a mask was sent.
    pub fn version_rolling_min_bit_count(&self) -> Option<u32> {
        self.version_rolling()
            .map(|params| params.min_bit_count.unwrap_or(0))
    }

    pub fn supports_extranonce(&self) -> bool {
        self.extensions
            .iter()
            .any(|extension| matches!(extension, ConfigureExtension::SubscribeExtranonce))
    }

    /// An extension is requested by its code, or by any of its parameters. Accepts a missing or
    /// null parameter object, and ignores elements after it.
    pub(super) fn from_params(params: &Value) -> Result<Self, ParamError> {
        let params = params_array(params)?;
        let codes = params
            .first()
            .ok_or(ParamError::new("extensions", "missing"))?
            .as_array()
            .ok_or(ParamError::new("extensions", "not an array"))?;
        let requested = |code: &str| codes.iter().any(|listed| listed == code);
        let parameter = |name: &str| params.get(1).and_then(|parameters| parameters.get(name));
        let string = |name: &'static str| match parameter(name) {
            None => Ok(None),
            Some(Value::String(value)) => Ok(Some(value.clone())),
            Some(_) => Err(ParamError::new(name, "not a string")),
        };

        let mut extensions = vec![];
        if requested("subscribe-extranonce") {
            extensions.push(ConfigureExtension::SubscribeExtranonce);
        }

        let mask = match parameter("version-rolling.mask") {
            None => None,
            Some(Value::String(mask)) => Some(hex_u32(mask, "version-rolling.mask")?),
            Some(_) => return Err(ParamError::new("version-rolling.mask", "not a string")),
        };
        let count = parameter("version-rolling.min-bit-count");
        if requested("version-rolling") || mask.is_some() || count.is_some() {
            extensions.push(ConfigureExtension::VersionRolling(VersionRolling {
                mask,
                min_bit_count: count.and_then(min_bit_count),
            }));
        }

        match parameter("minimum-difficulty.value") {
            Some(value) => {
                let value = value
                    .as_f64()
                    .filter(|value| *value >= 0.0)
                    .ok_or(ParamError::new(
                        "minimum-difficulty.value",
                        "not a non-negative number",
                    ))?;
                extensions.push(ConfigureExtension::MinimumDifficulty(value));
            }
            None if requested("minimum-difficulty") => {
                return Err(ParamError::new("minimum-difficulty.value", "missing"))
            }
            None => {}
        }

        let info = InfoParams {
            connection_url: string("info.connection-url")?,
            hw_id: string("info.hw-id")?,
            hw_version: string("info.hw-version")?,
            sw_version: string("info.sw-version")?,
        };
        if requested("info") || info != InfoParams::default() {
            extensions.push(ConfigureExtension::Info(info));
        }

        Ok(Configure { extensions })
    }

    pub(super) fn to_params(&self) -> Value {
        let mut codes = vec![];
        let mut parameters = Map::new();
        for extension in &self.extensions {
            match extension {
                ConfigureExtension::VersionRolling(params) => {
                    codes.push("version-rolling");
                    if let Some(mask) = &params.mask {
                        parameters.insert("version-rolling.mask".into(), (*mask).into());
                    }
                    if let Some(count) = params.min_bit_count {
                        parameters.insert("version-rolling.min-bit-count".into(), count.into());
                    }
                }
                ConfigureExtension::MinimumDifficulty(value) => {
                    codes.push("minimum-difficulty");
                    // A whole value goes out as an integer, the form BIP 310 shows.
                    let value = if value.fract() == 0.0 && (0.0..u64::MAX as f64).contains(value) {
                        Value::from(*value as u64)
                    } else {
                        Value::from(*value)
                    };
                    parameters.insert("minimum-difficulty.value".into(), value);
                }
                ConfigureExtension::SubscribeExtranonce => codes.push("subscribe-extranonce"),
                ConfigureExtension::Info(info) => {
                    codes.push("info");
                    for (name, value) in [
                        ("info.connection-url", &info.connection_url),
                        ("info.hw-id", &info.hw_id),
                        ("info.hw-version", &info.hw_version),
                        ("info.sw-version", &info.sw_version),
                    ] {
                        if let Some(value) = value {
                            parameters.insert(name.into(), value.clone().into());
                        }
                    }
                }
            }
        }
        Value::Array(vec![codes.into(), parameters.into()])
    }
}

impl fmt::Debug for InfoParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InfoParams")
            .field(
                "connection_url",
                &self
                    .connection_url
                    .as_ref()
                    .map(|_| format_args!("<redacted>")),
            )
            .field("hw_id", &self.hw_id)
            .field("hw_version", &self.hw_version)
            .field("sw_version", &self.sw_version)
            .finish()
    }
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
}

impl VersionRollingResult {
    /// Enables version rolling, clamping the mask to [`VERSION_ROLLING_MASK`] so it never covers
    /// reserved bits.
    pub fn new(mask: HexU32Be) -> Self {
        VersionRollingResult {
            enabled: true,
            mask: mask & VERSION_ROLLING_MASK,
        }
    }
}

/// The mask is clamped to [`VERSION_ROLLING_MASK`], so reserved bits are never offered.
impl From<ConfigureResult> for Value {
    fn from(result: ConfigureResult) -> Self {
        let mut values = Map::new();
        if let Some(version_rolling) = result.version_rolling {
            values.insert("version-rolling".into(), version_rolling.enabled.into());
            values.insert(
                "version-rolling.mask".into(),
                (version_rolling.mask & VERSION_ROLLING_MASK).into(),
            );
        }
        if let Some(minimum_difficulty) = result.minimum_difficulty {
            values.insert("minimum-difficulty".into(), minimum_difficulty.into());
        }
        values.into()
    }
}

impl ConfigureResult {
    /// Version rolling needs both `version-rolling` and `version-rolling.mask`. The mask is
    /// clamped to [`VERSION_ROLLING_MASK`], so a client never rolls reserved bits. BIP 310 gives
    /// the result no `version-rolling.min-bit-count`, so one is ignored.
    fn from_value(result: &Value) -> Result<Self, ParamError> {
        let values = result
            .as_object()
            .ok_or(ParamError::new("result", "not an object"))?;
        let enabled = values.get("version-rolling");
        let mask = values.get("version-rolling.mask");
        let version_rolling = match (enabled, mask) {
            (Some(enabled), Some(mask)) => Some(VersionRollingResult {
                enabled: enabled
                    .as_bool()
                    .ok_or(ParamError::new("version-rolling", "not a boolean"))?,
                mask: mask
                    .as_str()
                    .and_then(|mask| HexU32Be::try_from(mask).ok())
                    .map(|mask| mask & VERSION_ROLLING_MASK)
                    .ok_or(ParamError::new(
                        "version-rolling.mask",
                        "not a 32-bit hex value",
                    ))?,
            }),
            (None, None) => None,
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

impl<'de> Deserialize<'de> for ConfigureResult {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        ConfigureResult::from_value(&Value::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}
