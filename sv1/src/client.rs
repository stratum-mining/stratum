//! Messages a client sends to a server.

use crate::{
    error::{DecodeError, DecodeErrorKind, ParamError},
    json_rpc::{self, Id, Message, Method},
    utils::{
        extranonce_from_hex, hex_u32, integer_u32, min_bit_count, params_array, Extranonce,
        HexU32Be, VERSION_ROLLING_MASK,
    },
};
use serde_json::{Map, Value};
use std::{convert::TryFrom, fmt};

/// A request from a client. Each variant carries the ID its reply must echo.
#[derive(Clone, Debug)]
pub enum ClientRequest {
    Configure(Id, Configure),
    Subscribe(Id, Subscribe),
    Authorize(Id, Authorize),
    Submit(Id, Submit),
    ExtranonceSubscribe(Id),
    SuggestDifficulty(Id, SuggestDifficulty),
}

/// `mining.configure`, as defined by [BIP 310](https://github.com/bitcoin/bips/blob/master/bip-0310.mediawiki).
#[derive(Clone, Debug, Default)]
pub struct Configure {
    pub extensions: Vec<ConfigureExtension>,
}

#[derive(Clone, Debug)]
pub enum ConfigureExtension {
    VersionRolling(VersionRolling),
    MinimumDifficulty(u64),
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

/// `mining.subscribe`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Subscribe {
    /// Empty when the miner sent no user agent.
    pub agent: String,
    /// A subscription the miner asks to resume. The server decides whether to honour it.
    pub subscription_id: Option<String>,
}

/// `mining.authorize`.
#[derive(Clone, PartialEq, Eq)]
pub struct Authorize {
    pub worker_name: String,
    /// Empty when the miner omitted it or sent `null`.
    pub password: String,
}

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

/// `mining.suggest_difficulty`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SuggestDifficulty {
    /// `None` when the params hold no usable difficulty. Servers are not required to honour it.
    pub difficulty: Option<f64>,
}

impl ClientRequest {
    pub fn id(&self) -> &Id {
        match self {
            ClientRequest::Configure(id, _)
            | ClientRequest::Subscribe(id, _)
            | ClientRequest::Authorize(id, _)
            | ClientRequest::Submit(id, _)
            | ClientRequest::ExtranonceSubscribe(id)
            | ClientRequest::SuggestDifficulty(id, _) => id,
        }
    }
}

impl TryFrom<json_rpc::Request> for ClientRequest {
    type Error = DecodeError;

    fn try_from(request: json_rpc::Request) -> Result<Self, DecodeError> {
        let json_rpc::Request { id, method, params } = request;
        let error = |kind| DecodeError::about(id.clone(), Some(method.clone()), kind);
        let Some(id) = id.clone() else {
            return Err(error(DecodeErrorKind::MissingId));
        };
        let request = match &method {
            Method::Configure => {
                Configure::from_params(&params).map(|p| ClientRequest::Configure(id, p))
            }
            Method::Subscribe => {
                Subscribe::from_params(&params).map(|p| ClientRequest::Subscribe(id, p))
            }
            Method::Authorize => {
                Authorize::from_params(&params).map(|p| ClientRequest::Authorize(id, p))
            }
            Method::Submit => Submit::from_params(&params).map(|p| ClientRequest::Submit(id, p)),
            Method::ExtranonceSubscribe => Ok(ClientRequest::ExtranonceSubscribe(id)),
            Method::SuggestDifficulty => Ok(ClientRequest::SuggestDifficulty(
                id,
                SuggestDifficulty::from_params(&params),
            )),
            _ => return Err(error(DecodeErrorKind::UnknownMethod)),
        };
        request.map_err(|e| error(e.into()))
    }
}

/// A response is an error: only requests are expected.
impl TryFrom<Message> for ClientRequest {
    type Error = DecodeError;

    fn try_from(message: Message) -> Result<Self, DecodeError> {
        match message {
            Message::Request(request) => ClientRequest::try_from(request),
            Message::Response(response) => Err(DecodeError::about(
                response.id,
                None,
                DecodeErrorKind::UnexpectedResponse,
            )),
        }
    }
}

impl From<ClientRequest> for Message {
    fn from(request: ClientRequest) -> Self {
        let (id, method, params) = match request {
            ClientRequest::Configure(id, configure) => {
                (id, Method::Configure, configure.to_params())
            }
            ClientRequest::Subscribe(id, subscribe) => {
                (id, Method::Subscribe, subscribe.to_params())
            }
            ClientRequest::Authorize(
                id,
                Authorize {
                    worker_name,
                    password,
                },
            ) => (
                id,
                Method::Authorize,
                Value::from(vec![worker_name, password]),
            ),
            ClientRequest::Submit(id, submit) => (id, Method::Submit, submit.to_params()),
            ClientRequest::ExtranonceSubscribe(id) => {
                (id, Method::ExtranonceSubscribe, Value::Array(vec![]))
            }
            ClientRequest::SuggestDifficulty(id, suggest) => {
                let params = suggest.difficulty.into_iter().map(Value::from);
                (
                    id,
                    Method::SuggestDifficulty,
                    Value::Array(params.collect()),
                )
            }
        };
        Message::request(Some(id), method, params)
    }
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

    /// The requested mask, defaulting to [`VERSION_ROLLING_MASK`] when only a count was sent.
    pub fn version_rolling_mask(&self) -> Option<HexU32Be> {
        self.version_rolling().map(|params| {
            params
                .mask
                .clone()
                .unwrap_or(HexU32Be(VERSION_ROLLING_MASK))
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

    /// Accepts a missing or null parameter object, and ignores elements after it.
    fn from_params(params: &Value) -> Result<Self, ParamError> {
        let params = params_array(params)?;
        let codes = params
            .first()
            .ok_or(ParamError::new("extensions", "missing"))?
            .as_array()
            .ok_or(ParamError::new("extensions", "not an array"))?;
        let parameter = |name: &str| params.get(1).and_then(|parameters| parameters.get(name));
        let string = |name: &'static str| match parameter(name) {
            None => Ok(None),
            Some(Value::String(value)) => Ok(Some(value.clone())),
            Some(_) => Err(ParamError::new(name, "not a string")),
        };

        let mut extensions = vec![];
        if codes.iter().any(|code| code == "subscribe-extranonce") {
            extensions.push(ConfigureExtension::SubscribeExtranonce);
        }

        let mask = match parameter("version-rolling.mask") {
            None => None,
            Some(Value::String(mask)) => Some(hex_u32(mask, "version-rolling.mask")?),
            Some(_) => return Err(ParamError::new("version-rolling.mask", "not a string")),
        };
        let min_bit_count = match parameter("version-rolling.min-bit-count") {
            None => None,
            Some(_) if mask.is_none() => {
                return Err(ParamError::new("version-rolling.mask", "missing"))
            }
            Some(count) => Some(min_bit_count(count, "version-rolling.min-bit-count")?),
        };
        if mask.is_some() {
            extensions.push(ConfigureExtension::VersionRolling(VersionRolling {
                mask,
                min_bit_count,
            }));
        }

        if let Some(value) = parameter("minimum-difficulty.value") {
            let value = value.as_u64().ok_or(ParamError::new(
                "minimum-difficulty.value",
                "not an unsigned integer",
            ))?;
            extensions.push(ConfigureExtension::MinimumDifficulty(value));
        }

        let info = InfoParams {
            connection_url: string("info.connection-url")?,
            hw_id: string("info.hw-id")?,
            hw_version: string("info.hw-version")?,
            sw_version: string("info.sw-version")?,
        };
        if info != InfoParams::default() {
            extensions.push(ConfigureExtension::Info(info));
        }

        Ok(Configure { extensions })
    }

    fn to_params(&self) -> Value {
        let mut codes = vec![];
        let mut parameters = Map::new();
        for extension in &self.extensions {
            match extension {
                ConfigureExtension::VersionRolling(params) => {
                    codes.push("version-rolling");
                    if let Some(mask) = &params.mask {
                        parameters.insert("version-rolling.mask".into(), mask.clone().into());
                    }
                    if let Some(count) = params.min_bit_count {
                        parameters.insert("version-rolling.min-bit-count".into(), count.into());
                    }
                }
                ConfigureExtension::MinimumDifficulty(value) => {
                    codes.push("minimum-difficulty");
                    parameters.insert("minimum-difficulty.value".into(), (*value).into());
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

impl Subscribe {
    fn from_params(params: &Value) -> Result<Self, ParamError> {
        let (agent, subscription_id) = match params_array(params)? {
            [] => (String::new(), None),
            [Value::String(agent)] | [Value::String(agent), Value::Null] => (agent.clone(), None),
            // bosminer
            [Value::String(agent), Value::Null, Value::String(_), Value::Null] => {
                (agent.clone(), None)
            }
            [Value::String(agent), Value::String(id)] => (agent.clone(), Some(id.clone())),
            _ => return Err(ParamError::new("params", "unexpected subscribe params")),
        };
        Ok(Subscribe {
            agent,
            subscription_id,
        })
    }

    fn to_params(&self) -> Value {
        match (self.agent.as_str(), &self.subscription_id) {
            ("", None) => Value::Array(vec![]),
            (agent, None) => Value::from(vec![agent]),
            (agent, Some(id)) => Value::from(vec![agent, id.as_str()]),
        }
    }
}

impl Authorize {
    fn from_params(params: &Value) -> Result<Self, ParamError> {
        let (worker_name, password) = match params_array(params)? {
            [Value::String(name), Value::String(password)] => (name.clone(), password.clone()),
            [Value::String(name)] | [Value::String(name), Value::Null] => {
                (name.clone(), String::new())
            }
            _ => return Err(ParamError::new("params", "unexpected authorize params")),
        };
        Ok(Authorize {
            worker_name,
            password,
        })
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

impl Submit {
    /// `ntime` and `nonce` are hex strings, or both JSON integers; short hex is left-padded.
    fn from_params(params: &Value) -> Result<Self, ParamError> {
        let params = params_array(params)?;
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
        let extranonce2 = extranonce_from_hex(extranonce2)
            .map_err(|_| ParamError::new("extranonce2", "not hex of at most 32 bytes"))?;
        Ok(Submit {
            worker_name: worker_name.clone(),
            job_id: job_id.clone(),
            extranonce2,
            ntime,
            nonce,
            version_bits,
        })
    }

    fn to_params(&self) -> Value {
        let mut params: Vec<Value> = vec![
            self.worker_name.clone().into(),
            self.job_id.clone().into(),
            self.extranonce2.clone().into(),
            self.ntime.clone().into(),
            self.nonce.clone().into(),
        ];
        if let Some(bits) = &self.version_bits {
            params.push(bits.clone().into());
        }
        params.into()
    }
}

impl SuggestDifficulty {
    fn from_params(params: &Value) -> Self {
        let difficulty = match params.as_array().map(Vec::as_slice) {
            Some([difficulty]) => difficulty.as_f64().filter(|d| d.is_finite() && *d >= 0.0),
            _ => None,
        };
        SuggestDifficulty { difficulty }
    }
}
