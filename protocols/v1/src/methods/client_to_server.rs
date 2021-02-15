use serde_json::Value;
use serde_json::Value::Array as JArrary;
use serde_json::Value::Number as JNumber;
use serde_json::Value::String as JString;
use std::convert::TryFrom;
use std::convert::TryInto;

use crate::json_rpc::{Message, Response, StandardRequest};
use crate::utils::{HexBytes, HexU32Be};

use crate::methods::{MethodError, ParsingMethodError};

#[cfg(test)]
use quickcheck::{Arbitrary, Gen};

#[cfg(test)]
use quickcheck_macros;

/// _mining.authorize("username", "password")_
///
/// The result from an authorize request is usually true (successful), or false.
/// The password may be omitted if the server does not require passwords.
///
#[derive(Debug, Clone, PartialEq)]
pub struct Authorize {
    pub name: String,
    pub password: String,
    pub id: String,
}

impl Authorize {
    pub fn respond(self, is_ok: bool) -> Response {
        let result = serde_json::to_value(is_ok).unwrap();
        Response {
            id: self.id,
            result,
            error: None,
        }
    }
}

impl From<Authorize> for Message {
    fn from(auth: Authorize) -> Self {
        Message::StandardRequest(StandardRequest {
            id: auth.id,
            method: "mining.authorize".into(),
            parameters: (&[auth.name, auth.password][..]).into(),
        })
    }
}

impl TryFrom<StandardRequest> for Authorize {
    type Error = MethodError;

    fn try_from(msg: StandardRequest) -> Result<Self, Self::Error> {
        match msg.parameters.as_array() {
            Some(params) => {
                let (name, password) = match &params[..] {
                    [JString(a), JString(b)] => (a.into(), b.into()),
                    _ => {
                        return Err(ParsingMethodError::wrong_args_from_value(msg.parameters).into())
                    }
                };
                let id = msg.id;
                Ok(Self { name, password, id })
            }
            None => Err(ParsingMethodError::not_array_from_value(msg.parameters).into()),
        }
    }
}

#[cfg(test)]
impl Arbitrary for Authorize {
    fn arbitrary(g: &mut Gen) -> Self {
        Authorize {
            name: String::arbitrary(g),
            password: String::arbitrary(g),
            id: String::arbitrary(g),
        }
    }
}

#[cfg(test)]
#[quickcheck_macros::quickcheck]
fn from_to_json_rpc(auth: Authorize) -> bool {
    let message = Into::<Message>::into(auth.clone());
    let request = match message {
        Message::StandardRequest(s) => s,
        _ => panic!(),
    };
    auth == TryInto::<Authorize>::try_into(request).unwrap()
}

// mining.capabilities (DRAFT) TODO (incompatible with mining.configure)

/// _mining.extranonce.subscribe()_
/// Indicates to the server that the client supports the mining.set_extranonce method.
/// TODO https://en.bitcoin.it/wiki/BIP_0310
#[derive(Debug)]
pub struct ExtranonceSubscribe();

// mining.get_transactions TODO

/// _mining.submit("username", "job id", "ExtraNonce2", "nTime", "nOnce")_
///
/// Miners submit shares using the method "mining.submit". Client submissions contain:
///
/// * Worker Name.
/// * Job ID.
/// * ExtraNonce2.
/// * nTime.
/// * nOnce.
/// * version_bits (used by version-rolling extension)
///
/// Server response is result: true for accepted, false for rejected (or you may get an error with
/// more details).
#[derive(Debug, Clone, PartialEq)]
pub struct Submit {
    pub user_name: String,
    pub job_id: String,
    pub extra_nonce2: HexBytes,
    pub time: i64,
    pub nonce: i64,
    pub version_bits: Option<HexU32Be>,
    pub id: String,
}

impl Submit {
    pub fn respond(self, is_ok: bool) -> Response {
        let result = serde_json::to_value(is_ok).unwrap();
        Response {
            id: self.id,
            result,
            error: None,
        }
    }
}

impl From<Submit> for Message {
    fn from(submit: Submit) -> Self {
        let ex: String = submit.extra_nonce2.try_into().unwrap(); // TODO check if unwrap is ok here
        let mut parameters: Vec<Value> = vec![
            submit.user_name.into(),
            submit.job_id.into(),
            ex.into(),
            submit.time.into(),
            submit.nonce.into(),
        ];
        match submit.version_bits {
            Some(a) => {
                let a: String = a.try_into().unwrap(); // TODO check if unwrap is ok here
                parameters.push(a.into());
            }
            _ => (),
        };
        Message::StandardRequest(StandardRequest {
            id: submit.id,
            method: "mining.submit".into(),
            parameters: parameters.into(),
        })
    }
}

impl TryFrom<StandardRequest> for Submit {
    type Error = MethodError;

    fn try_from(msg: StandardRequest) -> Result<Self, Self::Error> {
        match msg.parameters.as_array() {
            Some(params) => {
                let (user_name, job_id, extra_nonce2, time, nonce, version_bits) = match &params[..]
                {
                    [JString(a), JString(b), JString(c), JNumber(d), JNumber(e), JString(f)] => (
                        a.into(),
                        b.into(),
                        (c.as_str()).try_into()?,
                        d.as_i64()
                            .ok_or_else(|| ParsingMethodError::not_int_from_value(d.clone()))?,
                        e.as_i64()
                            .ok_or_else(|| ParsingMethodError::not_int_from_value(e.clone()))?,
                        Some((f.as_str()).try_into()?),
                    ),
                    [JString(a), JString(b), JString(c), JNumber(d), JNumber(e)] => (
                        a.into(),
                        b.into(),
                        (c.as_str()).try_into()?,
                        d.as_i64()
                            .ok_or_else(|| ParsingMethodError::not_int_from_value(d.clone()))?,
                        e.as_i64()
                            .ok_or_else(|| ParsingMethodError::not_int_from_value(e.clone()))?,
                        None,
                    ),
                    _ => {
                        return Err(ParsingMethodError::wrong_args_from_value(msg.parameters).into())
                    }
                };
                let id = msg.id;
                let res = crate::client_to_server::Submit {
                    user_name,
                    job_id,
                    extra_nonce2,
                    time,
                    nonce,
                    version_bits,
                    id,
                };
                Ok(res)
            }
            None => Err(ParsingMethodError::not_array_from_value(msg.parameters).into()),
        }
    }
}

#[cfg(test)]
impl Arbitrary for Submit {
    fn arbitrary(g: &mut Gen) -> Self {
        let extra = Vec::<u8>::arbitrary(g);
        let bits = Option::<u32>::arbitrary(g);
        let extra = HexBytes(extra);
        let bits = bits.map(|x| HexU32Be(x));
        Submit {
            user_name: String::arbitrary(g),
            job_id: String::arbitrary(g),
            extra_nonce2: extra,
            time: i64::arbitrary(g),
            nonce: i64::arbitrary(g),
            version_bits: bits,
            id: String::arbitrary(g),
        }
    }
}
 
#[cfg(test)]
#[quickcheck_macros::quickcheck]
fn submit_from_to_json_rpc(submit: Submit) -> bool {
    let message = Into::<Message>::into(submit.clone());
    let request = match message {
        Message::StandardRequest(s) => s,
        _ => panic!(),
    };
    submit == TryInto::<Submit>::try_into(request).unwrap()
}


/// _mining.subscribe("user agent/version", "extranonce1")_
///
/// extranonce1 specifies a [mining.notify][a] extranonce1 the client wishes to
/// resume working with (possibly due to a dropped connection). If provided, a server MAY (at its
/// option) issue the connection the same extranonce1. Note that the extranonce1 may be the same
/// (allowing a resumed connection) even if the subscription id is changed!
///
/// [a]: crate::methods::server_to_client::Notify
///
///
#[derive(Debug)]
pub struct Subscribe {
    pub id: String,
    pub agent_signature: String,
    pub extranonce1: Option<HexBytes>,
}

impl Subscribe {
    pub fn respond(
        self,
        subscriptions: Vec<(String, String)>, // TODO should be a custom type?
        extra_nonce1: HexBytes,
        extra_nonce2_size: usize,
    ) -> Response {
        let response = crate::server_to_client::Subscribe {
            subscriptions,
            extra_nonce1,
            extra_nonce2_size,
            id: self.id,
        };
        match Message::try_from(response) {
            Ok(r) => match r {
                Message::Response(r) => r,
                _ => todo!(),
            },
            Err(_) => todo!(),
        }
    }
}

impl TryFrom<Subscribe> for Message {
    type Error = ();

    fn try_from(subscribe: Subscribe) -> Result<Self, ()> {
        let parameters = match (subscribe.agent_signature, subscribe.extranonce1) {
            (a, Some(b)) => vec![a, b.try_into().map_err(|_| ())?],
            (a, None) => vec![a],
        };
        Ok(Message::StandardRequest(StandardRequest {
            id: subscribe.id,
            method: "mining.subscribe".into(),
            parameters: (&parameters[..]).into(),
        }))
    }
}

impl TryFrom<StandardRequest> for Subscribe {
    type Error = MethodError;

    fn try_from(msg: StandardRequest) -> Result<Self, Self::Error> {
        match msg.parameters.as_array() {
            Some(params) => {
                let (agent_signature, extranonce1) = match &params[..] {
                    [JString(a), JString(b)] => (a.into(), Some(b.as_str().try_into()?)),
                    [JString(a)] => (a.into(), None),
                    _ => {
                        return Err(ParsingMethodError::wrong_args_from_value(msg.parameters).into())
                    }
                };
                let id = msg.id;
                let res = crate::client_to_server::Subscribe {
                    agent_signature,
                    extranonce1,
                    id,
                };
                Ok(res)
            }
            None => Err(ParsingMethodError::not_array_from_value(msg.parameters).into()),
        }
    }
}

/// TODO
#[derive(Debug)]
pub struct Configure {
    extensions: Vec<ConfigureExtension>,
    id: String,
}

impl Configure {
    pub fn new(id: String, mask: Option<HexU32Be>, min_bit_count: Option<HexU32Be>) -> Self {
        let extension = ConfigureExtension::VersionRolling(VersionRollingParams {
            mask,
            min_bit_count,
        });
        Configure {
            extensions: vec![extension],
            id,
        }
    }

    pub fn respond(
        self,
        version_rolling: Option<crate::server_to_client::VersionRollingParams>,
        minimum_difficulty: Option<bool>,
    ) -> Response {
        let response = crate::server_to_client::Configure {
            id: self.id,
            version_rolling,
            minimum_difficulty,
        };
        match Message::try_from(response) {
            Ok(r) => match r {
                Message::Response(r) => r,
                _ => todo!(),
            },
            Err(_) => todo!(),
        }
    }

    pub fn version_rolling_mask(&self) -> Option<HexU32Be> {
        let mut res = None;
        for ext in &self.extensions {
            match ext {
                ConfigureExtension::VersionRolling(p) => {
                    res = Some(p.mask.clone().unwrap_or(HexU32Be(0xffffffff)));
                }
                _ => (),
            }
        }
        res
    }

    pub fn version_rolling_min_bit_count(&self) -> Option<HexU32Be> {
        let mut res = None;
        for ext in &self.extensions {
            match ext {
                ConfigureExtension::VersionRolling(p) => {
                    // TODO check if 0 is the right default value
                    res = Some(p.min_bit_count.clone().unwrap_or(HexU32Be(0)));
                }
                _ => (),
            }
        }
        res
    }
}

impl From<Configure> for Message {
    fn from(conf: Configure) -> Self {
        let mut parameters = serde_json::Map::new();
        let extension_names: Vec<Value> = conf
            .extensions
            .iter()
            .map(|x| x.get_extension_name())
            .collect();
        for parameter in conf.extensions {
            let mut parameter: serde_json::Map<String, Value> = parameter.into();
            parameters.append(&mut parameter);
        }
        Message::StandardRequest(StandardRequest {
            id: conf.id,
            method: "mining.configure".into(),
            parameters: vec![JArrary(extension_names), parameters.into()].into(),
        })
    }
}

impl TryFrom<StandardRequest> for Configure {
    type Error = MethodError;

    fn try_from(msg: StandardRequest) -> Result<Self, Self::Error> {
        let extensions = ConfigureExtension::from_value(&msg.parameters)?;
        let id = msg.id;
        Ok(Self { extensions, id })
    }
}

#[derive(Debug)]
pub enum ConfigureExtension {
    VersionRolling(VersionRollingParams),
    MinimumDifficulty(u64),
    SubcribeExtraNonce,
    Info(InfoParams),
}

impl ConfigureExtension {
    pub fn from_value(val: &Value) -> Result<Vec<ConfigureExtension>, MethodError> {
        let mut res = vec![];
        let root = val
            .as_array()
            .ok_or_else(|| ParsingMethodError::not_array_from_value(val.clone()))?;
        if root.len() < 1 {
            return Err(ParsingMethodError::Todo.into());
        };
        let version_rolling_mask = val.pointer("1/version-rolling.mask");
        let version_rolling_min_bit = val.pointer("1/version-rolling.min-bit-count");
        let info_connection_url = val.pointer("1/info.connection-url");
        let info_hw_version = val.pointer("1/info.hw-version");
        let info_sw_version = val.pointer("1/info.sw-version");
        let info_hw_id = val.pointer("1/info.hw-id");
        let minimum_difficulty_value = val.pointer("1/minimum-difficulty.value");

        if root[0]
            .as_array()
            .ok_or_else(|| ParsingMethodError::not_array_from_value(root[0].clone()))?
            .contains(&JString("subscribe-extranonce".to_string()))
        {
            res.push(ConfigureExtension::SubcribeExtraNonce)
        }
        if version_rolling_mask.is_some() || version_rolling_min_bit.is_some() {
            let mask: Option<HexU32Be> = if version_rolling_mask.is_some()
                && version_rolling_mask.unwrap().as_str().is_some()
            {
                Some(version_rolling_mask.unwrap().as_str().unwrap().try_into()?)
            } else if version_rolling_mask.is_some() {
                return Err(ParsingMethodError::Todo.into());
            } else {
                None
            };
            let min_bit_count: Option<HexU32Be> = if version_rolling_min_bit.is_some()
                && version_rolling_min_bit.unwrap().as_str().is_some()
            {
                Some(
                    version_rolling_min_bit
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .try_into()?,
                )
            } else if version_rolling_mask.is_some() {
                return Err(ParsingMethodError::Todo.into());
            } else {
                None
            };
            let params = VersionRollingParams {
                mask,
                min_bit_count,
            };
            res.push(ConfigureExtension::VersionRolling(params));
        };

        if minimum_difficulty_value.is_some() {
            let min_diff = match minimum_difficulty_value.unwrap() {
                JNumber(a) => a
                    .as_u64()
                    .ok_or_else(|| ParsingMethodError::not_unsigned_from_value(a.clone()))?,
                _ => {
                    return Err(ParsingMethodError::unexpected_value_from_value(
                        minimum_difficulty_value.unwrap().clone(),
                    )
                    .into())
                }
            };

            res.push(ConfigureExtension::MinimumDifficulty(min_diff));
        };

        if info_connection_url.is_some()
            || info_hw_id.is_some()
            || info_hw_version.is_some()
            || info_sw_version.is_some()
        {
            let connection_url = if info_connection_url.is_some()
                && info_connection_url.unwrap().as_str().is_some()
            {
                Some(info_connection_url.unwrap().as_str().unwrap().to_string())
            } else if info_connection_url.is_some() {
                return Err(ParsingMethodError::Todo.into());
            } else {
                None
            };
            let hw_id = if info_hw_id.is_some() && info_hw_id.unwrap().as_str().is_some() {
                Some(info_hw_id.unwrap().as_str().unwrap().to_string())
            } else if info_hw_id.is_some() {
                return Err(ParsingMethodError::Todo.into());
            } else {
                None
            };
            let hw_version =
                if info_hw_version.is_some() && info_hw_version.unwrap().as_str().is_some() {
                    Some(info_hw_version.unwrap().as_str().unwrap().to_string())
                } else if info_hw_version.is_some() {
                    return Err(ParsingMethodError::Todo.into());
                } else {
                    None
                };
            let sw_version =
                if info_sw_version.is_some() && info_sw_version.unwrap().as_str().is_some() {
                    Some(info_sw_version.unwrap().as_str().unwrap().to_string())
                } else if info_sw_version.is_some() {
                    return Err(ParsingMethodError::Todo.into());
                } else {
                    None
                };
            let params = InfoParams {
                connection_url,
                hw_id,
                hw_version,
                sw_version,
            };
            res.push(ConfigureExtension::Info(params));
        };
        Ok(res)
    }
}

impl ConfigureExtension {
    pub fn get_extension_name(&self) -> Value {
        match self {
            ConfigureExtension::VersionRolling(_) => "version-rolling".into(),
            ConfigureExtension::MinimumDifficulty(_) => "minimum-difficulty".into(),
            ConfigureExtension::SubcribeExtraNonce => "subscribe-extranonce".into(),
            ConfigureExtension::Info(_) => "info".into(),
        }
    }
}

impl From<ConfigureExtension> for serde_json::Map<String, Value> {
    fn from(conf: ConfigureExtension) -> Self {
        match conf {
            ConfigureExtension::VersionRolling(a) => a.into(),
            ConfigureExtension::SubcribeExtraNonce => serde_json::Map::new(),
            ConfigureExtension::Info(a) => a.into(),
            ConfigureExtension::MinimumDifficulty(a) => {
                let mut map = serde_json::Map::new();
                map.insert("minimum-difficulty".to_string(), a.into());
                map
            }
        }
    }
}

#[derive(Debug)]
pub struct VersionRollingParams {
    mask: Option<HexU32Be>, // TODO chech if better to use just u32
    min_bit_count: Option<HexU32Be>,
}

impl From<VersionRollingParams> for serde_json::Map<String, Value> {
    fn from(conf: VersionRollingParams) -> Self {
        let mut params = serde_json::Map::new();
        match (conf.mask, conf.min_bit_count) {
            (Some(mask), Some(min)) => {
                let mask: String = mask.into();
                let min: String = min.into();
                params.insert("version-rolling.mask".to_string(), mask.into());
                params.insert("version-rolling.min-bit-count".to_string(), min.into());
            }
            (Some(mask), None) => {
                let mask: String = mask.into();
                params.insert("version-rolling.mask".to_string(), mask.into());
            }
            (None, Some(min)) => {
                let min: String = min.into();
                params.insert("version-rolling.min-bit-count".to_string(), min.into());
            }
            (None, None) => (),
        };
        params
    }
}

#[derive(Debug)]
pub struct InfoParams {
    connection_url: Option<String>,
    hw_version: Option<String>,
    sw_version: Option<String>,
    hw_id: Option<String>,
}

impl From<InfoParams> for serde_json::Map<String, Value> {
    fn from(info: InfoParams) -> Self {
        let mut params = serde_json::Map::new();
        if info.connection_url.is_some() {
            params.insert(
                "info.connection-url".to_string(),
                info.connection_url.unwrap().into(),
            );
        }
        params.into()
    }
}

// mining.suggest_difficulty TODO

// mining.suggest_target TODO

// mining.minimum_difficulty TODO (extension)
