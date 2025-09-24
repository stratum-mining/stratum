use bitcoin_hashes::hex::ToHex;
use serde_json::{
    Value,
    Value::{Array as JArrary, Null, Number as JNumber, String as JString},
};
use std::convert::{TryFrom, TryInto};

use crate::{
    error::Error,
    json_rpc::{Message, Response, StandardRequest},
    methods::ParsingMethodError,
    utils::{Extranonce, HexU32Be},
};

#[cfg(test)]
use quickcheck::{Arbitrary, Gen};

#[cfg(test)]
use quickcheck_macros;

/// _mining.authorize("username", "password")_
///
/// The result from an authorize request is usually true (successful), or false.
/// The password may be omitted if the server does not require passwords.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authorize {
    pub id: u64,
    pub name: String,
    pub password: String,
}

impl Authorize {
    pub fn respond(self, is_ok: bool) -> Response {
        // infallible
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
            params: (&[auth.name, auth.password][..]).into(),
        })
    }
}

impl TryFrom<StandardRequest> for Authorize {
    type Error = ParsingMethodError;

    fn try_from(msg: StandardRequest) -> Result<Self, Self::Error> {
        match msg.params.as_array() {
            Some(params) => {
                let (name, password) = match &params[..] {
                    [JString(a), JString(b)] => (a.into(), b.into()),
                    _ => return Err(ParsingMethodError::wrong_args_from_value(msg.params)),
                };
                let id = msg.id;
                Ok(Self { id, name, password })
            }
            None => Err(ParsingMethodError::not_array_from_value(msg.params)),
        }
    }
}

#[cfg(test)]
impl Arbitrary for Authorize {
    fn arbitrary(g: &mut Gen) -> Self {
        Authorize {
            name: String::arbitrary(g),
            password: String::arbitrary(g),
            id: u64::arbitrary(g),
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

// mining.capabilities (DRAFT) (incompatible with mining.configure)

/// _mining.extranonce.subscribe()_
/// Indicates to the server that the client supports the mining.set_extranonce method.
/// https://en.bitcoin.it/wiki/BIP_0310
#[derive(Debug, Clone, Copy)]
pub struct ExtranonceSubscribe();

// mining.get_transactions

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Submit<'a> {
    pub user_name: String,            // root
    pub job_id: String,               // 6
    pub extra_nonce2: Extranonce<'a>, // "8a.."
    pub time: HexU32Be,               //string
    pub nonce: HexU32Be,
    pub version_bits: Option<HexU32Be>,
    pub id: u64,
}
//"{"params": ["spotbtc1.m30s40x16", "2", "147a3f0000000000", "6436eddf", "41d5deb0", "00000000"],
//"{"params": "id": 2196, "method": "mining.submit"}"

impl Submit<'_> {
    pub fn respond(self, is_ok: bool) -> Response {
        // infallibel
        let result = serde_json::to_value(is_ok).unwrap();
        Response {
            id: self.id,
            result,
            error: None,
        }
    }
}

impl From<Submit<'_>> for Message {
    fn from(submit: Submit) -> Self {
        let ex: String = submit.extra_nonce2.0.inner_as_ref().to_hex();
        let mut params: Vec<Value> = vec![
            submit.user_name.into(),
            submit.job_id.into(),
            ex.into(),
            submit.time.into(),
            submit.nonce.into(),
        ];
        if let Some(a) = submit.version_bits {
            let a: String = a.into();
            params.push(a.into());
        };
        Message::StandardRequest(StandardRequest {
            id: submit.id,
            method: "mining.submit".into(),
            params: params.into(),
        })
    }
}

impl TryFrom<StandardRequest> for Submit<'_> {
    type Error = ParsingMethodError;

    #[allow(clippy::many_single_char_names)]
    fn try_from(msg: StandardRequest) -> Result<Self, Self::Error> {
        match msg.params.as_array() {
            Some(params) => {
                let (user_name, job_id, extra_nonce2, time, nonce, version_bits) = match &params[..]
                {
                    [JString(a), JString(b), JString(c), JNumber(d), JNumber(e), JString(f)] => (
                        a.into(),
                        b.into(),
                        Extranonce::try_from(hex::decode(c)?)?,
                        HexU32Be(d.as_u64().unwrap() as u32),
                        HexU32Be(e.as_u64().unwrap() as u32),
                        Some((f.as_str()).try_into()?),
                    ),
                    [JString(a), JString(b), JString(c), JString(d), JString(e), JString(f)] => (
                        a.into(),
                        b.into(),
                        Extranonce::try_from(hex::decode(c)?)?,
                        (d.as_str()).try_into()?,
                        (e.as_str()).try_into()?,
                        Some((f.as_str()).try_into()?),
                    ),
                    [JString(a), JString(b), JString(c), JNumber(d), JNumber(e)] => (
                        a.into(),
                        b.into(),
                        Extranonce::try_from(hex::decode(c)?)?,
                        HexU32Be(d.as_u64().unwrap() as u32),
                        HexU32Be(e.as_u64().unwrap() as u32),
                        None,
                    ),
                    [JString(a), JString(b), JString(c), JString(d), JString(e)] => (
                        a.into(),
                        b.into(),
                        Extranonce::try_from(hex::decode(c)?)?,
                        (d.as_str()).try_into()?,
                        (e.as_str()).try_into()?,
                        None,
                    ),
                    _ => return Err(ParsingMethodError::wrong_args_from_value(msg.params)),
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
            None => Err(ParsingMethodError::not_array_from_value(msg.params)),
        }
    }
}

#[cfg(test)]
impl Arbitrary for Submit<'static> {
    fn arbitrary(g: &mut Gen) -> Self {
        let mut extra = Vec::<u8>::arbitrary(g);
        extra.resize(32, 0);
        println!("\nEXTRA: {extra:?}\n");
        let bits = Option::<u32>::arbitrary(g);
        println!("\nBITS: {bits:?}\n");
        let extra: Extranonce = extra.try_into().unwrap();
        let bits = bits.map(HexU32Be);
        println!("\nBITS: {bits:?}\n");
        Submit {
            user_name: String::arbitrary(g),
            job_id: String::arbitrary(g),
            extra_nonce2: extra,
            time: HexU32Be(u32::arbitrary(g)),
            nonce: HexU32Be(u32::arbitrary(g)),
            version_bits: bits,
            id: u64::arbitrary(g),
        }
    }
}

#[cfg(test)]
#[quickcheck_macros::quickcheck]
fn submit_from_to_json_rpc(submit: Submit<'static>) -> bool {
    let message = Into::<Message>::into(submit.clone());
    println!("\nMESSAGE: {message:?}\n");
    let request = match message {
        Message::StandardRequest(s) => s,
        _ => panic!(),
    };
    println!("\nREQUEST: {request:?}\n");
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
#[derive(Debug, Clone)]
pub struct Subscribe<'a> {
    pub id: u64,
    pub agent_signature: String,
    pub extranonce1: Option<Extranonce<'a>>,
}

impl<'a> Subscribe<'a> {
    pub fn respond(
        self,
        subscriptions: Vec<(String, String)>,
        extra_nonce1: Extranonce<'a>,
        extra_nonce2_size: usize,
    ) -> Response {
        let response = crate::server_to_client::Subscribe {
            subscriptions,
            extra_nonce1,
            extra_nonce2_size,
            id: self.id,
        };
        match Message::from(response) {
            Message::OkResponse(r) => r,
            _ => unreachable!(),
        }
    }
}

impl<'a> TryFrom<Subscribe<'a>> for Message {
    type Error = Error<'a>;

    fn try_from(subscribe: Subscribe) -> Result<Self, Error> {
        let params = match (subscribe.agent_signature, subscribe.extranonce1) {
            (a, Some(b)) => vec![a, b.0.inner_as_ref().to_hex()],
            (a, None) => vec![a],
        };
        Ok(Message::StandardRequest(StandardRequest {
            id: subscribe.id,
            method: "mining.subscribe".into(),
            params: (&params[..]).into(),
        }))
    }
}

impl TryFrom<StandardRequest> for Subscribe<'_> {
    type Error = ParsingMethodError;

    fn try_from(msg: StandardRequest) -> Result<Self, Self::Error> {
        match msg.params.as_array() {
            Some(params) => {
                let (agent_signature, extranonce1) = match &params[..] {
                    // bosminer subscribe message
                    [JString(a), Null, JString(_), Null] => (a.into(), None),
                    // bosminer subscribe message
                    [JString(a), Null] => (a.into(), None),
                    [JString(a), JString(b)] => (a.into(), Some(Extranonce::try_from(b.as_str())?)),
                    [JString(a)] => (a.into(), None),
                    [] => ("".to_string(), None),
                    _ => return Err(ParsingMethodError::wrong_args_from_value(msg.params)),
                };
                let id = msg.id;
                let res = Subscribe {
                    id,
                    agent_signature,
                    extranonce1,
                };
                Ok(res)
            }
            None => Err(ParsingMethodError::not_array_from_value(msg.params)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Configure {
    extensions: Vec<ConfigureExtension>,
    id: u64,
}

impl Configure {
    pub fn new(id: u64, mask: Option<HexU32Be>, min_bit_count: Option<HexU32Be>) -> Self {
        let extension = ConfigureExtension::VersionRolling(VersionRollingParams {
            mask,
            min_bit_count,
        });
        Configure {
            extensions: vec![extension],
            id,
        }
    }

    pub fn void(id: u64) -> Self {
        Configure {
            extensions: vec![],
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
        match Message::from(response) {
            Message::OkResponse(r) => r,
            _ => unreachable!(),
        }
    }

    pub fn version_rolling_mask(&self) -> Option<HexU32Be> {
        let mut res = None;
        for ext in &self.extensions {
            if let ConfigureExtension::VersionRolling(p) = ext {
                res = Some(p.mask.clone().unwrap_or(HexU32Be(0x1FFFE000)));
            };
        }
        res
    }

    pub fn version_rolling_min_bit_count(&self) -> Option<HexU32Be> {
        let mut res = None;
        for ext in &self.extensions {
            if let ConfigureExtension::VersionRolling(p) = ext {
                // TODO check if 0 is the right default value
                res = Some(p.min_bit_count.clone().unwrap_or(HexU32Be(0)));
            };
        }
        res
    }
}

impl From<Configure> for Message {
    fn from(conf: Configure) -> Self {
        let mut params = serde_json::Map::new();
        let extension_names: Vec<Value> = conf
            .extensions
            .iter()
            .map(|x| x.get_extension_name())
            .collect();
        for parameter in conf.extensions {
            let mut parameter: serde_json::Map<String, Value> = parameter.into();
            params.append(&mut parameter);
        }
        Message::StandardRequest(StandardRequest {
            id: conf.id,
            method: "mining.configure".into(),
            params: vec![JArrary(extension_names), params.into()].into(),
        })
    }
}

impl TryFrom<StandardRequest> for Configure {
    type Error = ParsingMethodError;

    fn try_from(msg: StandardRequest) -> Result<Self, Self::Error> {
        let extensions = ConfigureExtension::from_value(&msg.params)?;
        let id = msg.id;
        Ok(Self { extensions, id })
    }
}

#[derive(Debug, Clone)]
pub enum ConfigureExtension {
    VersionRolling(VersionRollingParams),
    MinimumDifficulty(u64),
    SubcribeExtraNonce,
    Info(InfoParams),
}

#[allow(clippy::unnecessary_unwrap)]
impl ConfigureExtension {
    pub fn from_value(val: &Value) -> Result<Vec<ConfigureExtension>, ParsingMethodError> {
        let mut res = vec![];
        let root = val
            .as_array()
            .ok_or_else(|| ParsingMethodError::not_array_from_value(val.clone()))?;
        if root.is_empty() {
            return Err(ParsingMethodError::Todo);
        };

        let version_rolling_mask = val.pointer("/1/version-rolling.mask");
        let version_rolling_min_bit = val.pointer("/1/version-rolling.min-bit-count");
        let info_connection_url = val.pointer("/1/info.connection-url");
        let info_hw_version = val.pointer("/1/info.hw-version");
        let info_sw_version = val.pointer("/1/info.sw-version");
        let info_hw_id = val.pointer("/1/info.hw-id");
        let minimum_difficulty_value = val.pointer("/1/minimum-difficulty.value");

        if root[0]
            .as_array()
            .ok_or_else(|| ParsingMethodError::not_array_from_value(root[0].clone()))?
            .contains(&JString("subscribe-extranonce".to_string()))
        {
            res.push(ConfigureExtension::SubcribeExtraNonce)
        }
        let (mask, min_bit_count) = match (version_rolling_mask, version_rolling_min_bit) {
            (None, None) => (None, None),
            // WhatsMiner sent mask without min bit count
            (Some(JString(mask)), None) => {
                let mask: HexU32Be = mask.as_str().try_into()?;
                (Some(mask), None)
            }
            // Min bit can be a string cpuminer
            (Some(JString(mask)), Some(JString(min_bit))) => {
                let mask: HexU32Be = mask.as_str().try_into()?;
                let min_bit: HexU32Be = min_bit.as_str().try_into()?;
                (Some(mask), Some(min_bit))
            }
            // Min bit can be a number s9, s19
            (Some(JString(mask)), Some(JNumber(min_bit))) => {
                let mask: HexU32Be = mask.as_str().try_into()?;
                // min_bit is a json number checked above so as_u64 can not fail
                let min_bit: HexU32Be = HexU32Be(min_bit.as_u64().unwrap() as u32);
                (Some(mask), Some(min_bit))
            }
            // We can not have min bit count without a mask
            (None, Some(_)) => return Err(ParsingMethodError::Todo),
            // Mask need to be a JString
            (Some(_), None) => return Err(ParsingMethodError::Todo),
            // Min bit need to be a string or a number
            (Some(_), Some(_)) => return Err(ParsingMethodError::Todo),
        };
        if mask.is_some() || min_bit_count.is_some() {
            let params = VersionRollingParams {
                mask,
                min_bit_count,
            };
            res.push(ConfigureExtension::VersionRolling(params));
        }

        if let Some(minimum_difficulty_value) = minimum_difficulty_value {
            let min_diff = match minimum_difficulty_value {
                JNumber(a) => a
                    .as_u64()
                    .ok_or_else(|| ParsingMethodError::not_unsigned_from_value(a.clone()))?,
                _ => {
                    return Err(ParsingMethodError::unexpected_value_from_value(
                        minimum_difficulty_value.clone(),
                    ))
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
                // infallible
                && info_connection_url.unwrap().as_str().is_some()
            {
                // infallible
                Some(info_connection_url.unwrap().as_str().unwrap().to_string())
            } else if info_connection_url.is_some() {
                return Err(ParsingMethodError::Todo);
            } else {
                None
            };
            // infallible
            let hw_id = if info_hw_id.is_some() && info_hw_id.unwrap().as_str().is_some() {
                // infallible
                Some(info_hw_id.unwrap().as_str().unwrap().to_string())
            } else if info_hw_id.is_some() {
                return Err(ParsingMethodError::Todo);
            } else {
                None
            };
            // infallible
            let hw_version =
                if info_hw_version.is_some() && info_hw_version.unwrap().as_str().is_some() {
                    // infallible
                    Some(info_hw_version.unwrap().as_str().unwrap().to_string())
                } else if info_hw_version.is_some() {
                    return Err(ParsingMethodError::Todo);
                } else {
                    None
                };
            let sw_version =
                // infallible
                if info_sw_version.is_some() && info_sw_version.unwrap().as_str().is_some() {
                    // infallible
                    Some(info_sw_version.unwrap().as_str().unwrap().to_string())
                } else if info_sw_version.is_some() {
                    return Err(ParsingMethodError::Todo);
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

#[derive(Debug, Clone)]
pub struct VersionRollingParams {
    mask: Option<HexU32Be>,
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

#[derive(Debug, Clone)]
pub struct InfoParams {
    connection_url: Option<String>,
    #[allow(dead_code)]
    hw_id: Option<String>,
    #[allow(dead_code)]
    hw_version: Option<String>,
    #[allow(dead_code)]
    sw_version: Option<String>,
}

impl From<InfoParams> for serde_json::Map<String, Value> {
    fn from(info: InfoParams) -> Self {
        let mut params = serde_json::Map::new();
        if info.connection_url.is_some() {
            params.insert(
                "info.connection-url".to_string(),
                // infallible
                info.connection_url.unwrap().into(),
            );
        }
        params
    }
}

// mining.suggest_difficulty

// mining.suggest_target

// mining.minimum_difficulty (extension)
#[test]
fn test_version_extension_with_broken_bit_count() {
    let client_message = r#"{"id":0,
            "method": "mining.configure",
            "params":[
                ["version-rolling"],
                {"version-rolling.mask":"1fffe000",
                "version-rolling.min-bit-count":"16"}
            ]
        }"#;
    let client_message: StandardRequest = serde_json::from_str(client_message).unwrap();
    let server_configure = Configure::try_from(client_message).unwrap();
    match &server_configure.extensions[0] {
        ConfigureExtension::VersionRolling(params) => {
            assert!(params.min_bit_count.as_ref().unwrap().0 == 0x16)
        }
        _ => panic!(),
    };
}
#[test]
fn test_version_extension_with_non_string_bit_count() {
    let client_message = r#"{"id":0,
            "method": "mining.configure",
            "params":[
                ["version-rolling"],
                {"version-rolling.mask":"1fffe000",
                "version-rolling.min-bit-count":16}
            ]
        }"#;
    let client_message: StandardRequest = serde_json::from_str(client_message).unwrap();
    let server_configure = Configure::try_from(client_message).unwrap();
    match &server_configure.extensions[0] {
        ConfigureExtension::VersionRolling(params) => {
            assert!(params.min_bit_count.as_ref().unwrap().0 == 16)
        }
        _ => panic!(),
    };
}

#[test]
fn test_version_extension_with_no_bit_count() {
    let client_message = r#"{"id":0,
            "method": "mining.configure",
            "params":[
                ["version-rolling"],
                {"version-rolling.mask":"ffffffff"}
            ]
        }"#;
    let client_message: StandardRequest = serde_json::from_str(client_message).unwrap();
    let server_configure = Configure::try_from(client_message).unwrap();
    match &server_configure.extensions[0] {
        ConfigureExtension::VersionRolling(params) => {
            assert!(params.min_bit_count.as_ref().is_none());
        }
        _ => panic!(),
    };
}

#[test]
fn test_subscribe_with_odd_length_extranonce() {
    // Test that odd-length hex strings (with leading zeroes) are handled correctly
    let client_message = r#"{"id":1,
            "method": "mining.subscribe",
            "params":["test-agent", "abc"]
        }"#;
    let client_message: StandardRequest = serde_json::from_str(client_message).unwrap();
    let subscribe = Subscribe::try_from(client_message).unwrap();

    // Should successfully parse odd-length hex string by prepending "0"
    assert_eq!(subscribe.agent_signature, "test-agent");
    assert!(subscribe.extranonce1.is_some());
    let extranonce = subscribe.extranonce1.unwrap();
    assert_eq!(extranonce.0.inner_as_ref(), &[0x0a, 0xbc]); // "0abc" -> [10, 188]
}

#[test]
fn test_subscribe_with_even_length_extranonce() {
    // Test that even-length hex strings work as before
    let client_message = r#"{"id":1,
            "method": "mining.subscribe",
            "params":["test-agent", "abcd"]
        }"#;
    let client_message: StandardRequest = serde_json::from_str(client_message).unwrap();
    let subscribe = Subscribe::try_from(client_message).unwrap();

    assert_eq!(subscribe.agent_signature, "test-agent");
    assert!(subscribe.extranonce1.is_some());
    let extranonce = subscribe.extranonce1.unwrap();
    assert_eq!(extranonce.0.inner_as_ref(), &[0xab, 0xcd]); // "abcd" -> [171, 205]
}
