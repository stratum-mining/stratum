use serde_json::Value;
use serde_json::Value::Array as JArrary;
use serde_json::Value::Bool as JBool;
use serde_json::Value::Number as JNumber;
use serde_json::Value::String as JString;
use std::convert::TryFrom;
use std::convert::TryInto;

use crate::json_rpc::{Message, Notification, Response};
use crate::methods::{MethodError, ParsingMethodError};
use crate::utils::{HexBytes, HexU32Be, PrevHash};

// client.get_version() TODO

// client.reconnect TODO

// client.show_message TODO

/// Fields in order:
///
/// * Job ID: This is included when miners submit a results so work can be matched with proper
///   transactions.
/// * Hash of previous block: Used to build the header.
/// * Generation transaction (part 1): The miner inserts ExtraNonce1 and ExtraNonce2 after this
///   section of the transaction data.
/// * Generation transaction (part 2): The miner appends this after the first part of the
///   transaction data and the two ExtraNonce values.
/// * List of merkle branches: The generation transaction is hashed against the merkle branches to
///   build the final merkle root.
/// * Bitcoin block version: Used in the block header.
///     * nBits: The encoded network difficulty. Used in the block header.
///     * nTime: The current time. nTime rolling should be supported, but should not increase faster
///     than actual time.
/// * Clean Jobs: If true, miners should abort their current work and immediately use the new job.
///   If false, they can still use the current job, but should move to the new one after exhausting
///   the current nonce range.
///
#[derive(Debug, Clone)]
pub struct Notify {
    pub job_id: String,
    pub prev_hash: PrevHash,
    pub coin_base1: HexBytes,
    pub coin_base2: HexBytes,
    pub merkle_branch: Vec<HexBytes>,
    pub version: HexU32Be,
    pub bits: HexU32Be,
    pub time: HexU32Be,
    pub clean_jobs: bool,
}

impl TryFrom<Notify> for Message {
    type Error = ();

    fn try_from(notify: Notify) -> Result<Self, ()> {
        let prev_hash: Value = notify.prev_hash.try_into().map_err(|_| ())?;
        let coin_base1: Value = notify.coin_base1.try_into().map_err(|_| ())?;
        let coin_base2: Value = notify.coin_base2.try_into().map_err(|_| ())?;
        let mut merkle_branch: Vec<Value> = vec![];
        for mb in notify.merkle_branch {
            let mb: Value = mb.try_into().map_err(|_| ())?;
            merkle_branch.push(mb);
        }
        let merkle_branch = JArrary(merkle_branch);
        let version: Value = notify.version.try_into().map_err(|_| ())?;
        let bits: Value = notify.bits.try_into().map_err(|_| ())?;
        let time: Value = notify.time.try_into().map_err(|_| ())?;
        Ok(Message::Notification(Notification {
            method: "mining.notify".to_string(),
            parameters: (&[
                notify.job_id.into(),
                prev_hash,
                coin_base1,
                coin_base2,
                merkle_branch,
                version,
                bits,
                time,
                notify.clean_jobs.into(),
            ][..])
                .into(),
        }))
    }
}

impl TryFrom<Notification> for Notify {
    type Error = MethodError;

    fn try_from(msg: Notification) -> Result<Self, Self::Error> {
        let params = msg
            .parameters
            .as_array()
            .ok_or_else(|| ParsingMethodError::not_array_from_value(msg.parameters.clone()))?;
        let (
            job_id,
            prev_hash,
            coin_base1,
            coin_base2,
            merkle_branch_,
            version,
            bits,
            time,
            clean_jobs,
        ) = match &params[..] {
            [JString(a), JString(b), JString(c), JString(d), JArrary(e), JString(f), JString(g), JString(h), JBool(i)] => {
                (
                    a.into(),
                    b.as_str().try_into()?,
                    c.as_str().try_into()?,
                    d.as_str().try_into()?,
                    e,
                    f.as_str().try_into()?,
                    g.as_str().try_into()?,
                    h.as_str().try_into()?,
                    *i,
                )
            }
            _ => return Err(ParsingMethodError::wrong_args_from_value(msg.parameters).into()),
        };
        let mut merkle_branch = vec![];
        for h in merkle_branch_ {
            let h: HexBytes = h
                .as_str()
                .ok_or_else(|| ParsingMethodError::not_string_from_value(h.clone()))?
                .try_into()?;
            merkle_branch.push(h);
        }
        Ok(Notify {
            job_id,
            prev_hash,
            coin_base1,
            coin_base2,
            merkle_branch,
            version,
            bits,
            time,
            clean_jobs,
        })
    }
}

/// mining.set_difficulty(difficulty)
///
/// The server can adjust the difficulty required for miner shares with the "mining.set_difficulty"
/// method. The miner should begin enforcing the new difficulty on the next job received. Some pools
/// may force a new job out when set_difficulty is sent, using clean_jobs to force the miner to
/// begin using the new difficulty immediately.
///
#[derive(Debug)]
pub struct SetDifficulty {
    value: f64,
}

impl From<SetDifficulty> for Message {
    fn from(sd: SetDifficulty) -> Self {
        let value: Value = sd.value.into();
        Message::Notification(Notification {
            method: "mining.set_difficulty".to_string(),
            parameters: (&[value][..]).into(),
        })
    }
}

impl TryFrom<Notification> for SetDifficulty {
    type Error = MethodError;

    fn try_from(msg: Notification) -> Result<Self, Self::Error> {
        let params = msg
            .parameters
            .as_array()
            .ok_or_else(|| ParsingMethodError::not_array_from_value(msg.parameters.clone()))?;
        let (value,) = match &params[..] {
            [a] => (a
                .as_f64()
                .ok_or_else(|| ParsingMethodError::not_float_from_value(a.clone()))?,),
            _ => return Err(ParsingMethodError::wrong_args_from_value(msg.parameters).into()),
        };
        Ok(SetDifficulty { value })
    }
}

/// SetExtranonce message (sent if we subscribed with `ExtranonceSubscribe`)
///
/// mining.set_extranonce("extranonce1", extranonce2_size)
///
/// These values, when provided, replace the initial subscription values beginning with the next
/// mining.notify job.
///
/// TODO check if it is a Notification or a StandardRequest this implementation assume that it is a
/// Notification
///
#[derive(Debug)]
pub struct SetExtranonce {
    pub extra_nonce1: HexBytes,
    pub extra_nonce2_size: usize,
}

impl TryFrom<SetExtranonce> for Message {
    type Error = ();

    fn try_from(se: SetExtranonce) -> Result<Self, ()> {
        let extra_nonce1: Value = se.extra_nonce1.try_into().map_err(|_| ())?;
        let extra_nonce2_size: Value = se.extra_nonce2_size.into();
        Ok(Message::Notification(Notification {
            method: "mining.set_extranonce".to_string(),
            parameters: (&[extra_nonce1, extra_nonce2_size][..]).into(),
        }))
    }
}

impl TryFrom<Notification> for SetExtranonce {
    type Error = MethodError;

    fn try_from(msg: Notification) -> Result<Self, Self::Error> {
        let params = msg
            .parameters
            .as_array()
            .ok_or_else(|| ParsingMethodError::not_array_from_value(msg.parameters.clone()))?;
        let (extra_nonce1, extra_nonce2_size) = match &params[..] {
            [JString(a), JNumber(b)] => (
                a.as_str().try_into()?,
                b.as_u64()
                    .ok_or_else(|| ParsingMethodError::not_unsigned_from_value(b.clone()))?
                    as usize,
            ),
            _ => return Err(ParsingMethodError::wrong_args_from_value(msg.parameters).into()),
        };
        Ok(SetExtranonce {
            extra_nonce1,
            extra_nonce2_size,
        })
    }
}

#[derive(Debug)]
/// Server may arbitrarily adjust version mask
pub struct SetVersionMask {
    version_mask: HexU32Be,
}

impl TryFrom<SetVersionMask> for Message {
    type Error = ();

    fn try_from(sv: SetVersionMask) -> Result<Self, ()> {
        let version_mask: Value = sv.version_mask.try_into().map_err(|_| ())?;
        Ok(Message::Notification(Notification {
            method: "mining.set_version".to_string(),
            parameters: (&[version_mask][..]).into(),
        }))
    }
}

impl TryFrom<Notification> for SetVersionMask {
    type Error = MethodError;

    fn try_from(msg: Notification) -> Result<Self, Self::Error> {
        let params = msg
            .parameters
            .as_array()
            .ok_or_else(|| ParsingMethodError::not_array_from_value(msg.parameters.clone()))?;
        let version_mask = match &params[..] {
            [JString(a)] => a.as_str().try_into()?,
            _ => return Err(ParsingMethodError::wrong_args_from_value(msg.parameters).into()),
        };
        Ok(SetVersionMask { version_mask })
    }
}

#[derive(Debug)]
pub struct Authorize(pub crate::json_rpc::Response, pub String);

impl Authorize {
    pub fn is_ok(&self) -> bool {
        match self.0.result.as_bool() {
            Some(a) => a,
            None => false,
        }
    }

    pub fn user_name(self) -> String {
        self.1
    }
}

#[derive(Debug)]
pub struct Submit(pub crate::json_rpc::Response);

impl Submit {
    pub fn is_ok(&self) -> bool {
        match self.0.result.as_bool() {
            Some(a) => a,
            None => false,
        }
    }
}

/// mining.subscribe
/// mining.subscribe("user agent/version", "extranonce1")
/// The optional second parameter specifies a mining.notify subscription id the client wishes to resume
/// working with (possibly due to a dropped connection). If provided, a server MAY (at its option)
/// issue the connection the same extranonce1. Note that the extranonce1 may be the same (allowing
/// a resumed connection) even if the subscription id is changed!
///
/// The client receives a result:
///
///
/// The result contains three items:
///
///    Subscriptions. - An array of 2-item tuples, each with a subscription type and id.
///
///    ExtraNonce1. - Hex-encoded, per-connection unique string which will be used for creating
///    generation transactions later.
///
///    ExtraNonce2_size. - The number of bytes that the miner users for its ExtraNonce2 counter.
///
#[derive(Debug)]
pub struct Subscribe {
    pub subscriptions: Vec<(String, String)>,
    pub extra_nonce1: HexBytes,
    pub extra_nonce2_size: usize,
    pub id: String,
}

impl TryFrom<Subscribe> for Message {
    type Error = ();

    fn try_from(su: Subscribe) -> Result<Self, Self::Error> {
        let extra_nonce1: Value = su.extra_nonce1.try_into().map_err(|_| ())?;
        let extra_nonce2_size: Value = su.extra_nonce2_size.into();
        let subscriptions: Vec<Value> = su
            .subscriptions
            .iter()
            .map(|x| JArrary(vec![JString(x.0.clone()), JString(x.1.clone())]))
            .collect();
        let subscriptions: Value = subscriptions.into();
        Ok(Message::Response(Response {
            id: su.id,
            error: None,
            result: (&[extra_nonce1, extra_nonce2_size, subscriptions][..]).into(),
        }))
    }
}

impl TryFrom<&Response> for Subscribe {
    type Error = ();

    fn try_from(msg: &Response) -> Result<Self, Self::Error> {
        let id = msg.id.clone();
        let params = msg.result.as_array().ok_or(())?;
        let (extra_nonce1, extra_nonce2_size, subscriptions_) = match &params[..] {
            [JString(a), JNumber(b), JArrary(d)] => (
                a.as_str().try_into().map_err(|_| ())?,
                b.as_u64().ok_or(())? as usize,
                d,
            ),
            _ => return Err(()),
        };
        let mut subscriptions: Vec<(String, String)> = vec![];
        for s in subscriptions_ {
            let s = s.as_array().ok_or(())?;
            if s.len() != 2 {
                return Err(());
            };
            let s = (
                s[0].as_str().ok_or(())?.to_string(),
                s[1].as_str().ok_or(())?.to_string(),
            );
            subscriptions.push(s);
        }
        Ok(Subscribe {
            id,
            extra_nonce1,
            extra_nonce2_size,
            subscriptions,
        })
    }
}

#[derive(Debug)]
pub struct Configure {
    pub version_rolling: Option<VersionRollingParams>,
    pub minimum_difficulty: Option<bool>,
    pub id: String,
}

impl Configure {
    pub fn version_rolling_mask(&self) -> Option<HexU32Be> {
        match &self.version_rolling {
            None => None,
            Some(a) => {
                if a.version_rolling {
                    Some(a.version_rolling_mask.clone())
                } else {
                    None
                }
            }
        }
    }
    pub fn version_rolling_min_bit(&self) -> Option<HexU32Be> {
        match &self.version_rolling {
            None => None,
            Some(a) => {
                if a.version_rolling {
                    Some(a.version_rolling_min_bit_count.clone())
                } else {
                    None
                }
            }
        }
    }
}

impl TryFrom<Configure> for Message {
    type Error = ();

    fn try_from(co: Configure) -> Result<Self, ()> {
        let mut params = serde_json::Map::new();
        if co.version_rolling.is_some() {
            let mut version_rolling: serde_json::Map<String, Value> =
                co.version_rolling.unwrap().try_into()?;
            params.append(&mut version_rolling);
        };
        if co.minimum_difficulty.is_some() {
            let minimum_difficulty: Value = co.minimum_difficulty.unwrap().into();
            params.insert("minimum-difficulty".to_string(), minimum_difficulty);
        };
        Ok(Message::Response(Response {
            id: co.id,
            error: None,
            result: serde_json::Value::Object(params),
        }))
    }
}

impl TryFrom<&Response> for Configure {
    type Error = ();

    fn try_from(msg: &Response) -> Result<Self, ()> {
        let id = msg.id.clone();
        let params = msg.result.as_object().ok_or(())?;

        let version_rolling_ = params.get("version-rolling");
        let version_rolling_mask = params.get("version-rolling.mask");
        let version_rolling_min_bit_count = params.get("version-rolling.min-bit-count");
        let minimum_difficulty = params.get("minimum-difficulty");

        // Deserialize version-rolling response.
        // Composed by 3 required fields
        let version_rolling: Option<VersionRollingParams>;
        if version_rolling_.is_some()
            && version_rolling_mask.is_some()
            && version_rolling_min_bit_count.is_some()
        {
            let vr: bool = version_rolling_.unwrap().as_bool().ok_or(())?;
            let version_rolling_mask: HexU32Be = version_rolling_mask
                .unwrap()
                .as_str()
                .ok_or(())?
                .try_into()
                .map_err(|_| ())?;
            let version_rolling_min_bit_count: HexU32Be = version_rolling_min_bit_count
                .unwrap()
                .as_str()
                .ok_or(())?
                .try_into()
                .map_err(|_| ())?;
            version_rolling = Some(VersionRollingParams {
                version_rolling: vr,
                version_rolling_mask,
                version_rolling_min_bit_count,
            });
        } else if version_rolling_.is_none()
            && version_rolling_mask.is_none()
            && version_rolling_min_bit_count.is_none()
        {
            version_rolling = None;
        } else {
            return Err(());
        };

        let minimum_difficulty = match minimum_difficulty {
            Some(a) => Some(a.as_bool().ok_or(())?),
            None => None,
        };

        Ok(Configure {
            id,
            version_rolling,
            minimum_difficulty,
        })
    }
}

#[derive(Debug)]
pub struct VersionRollingParams {
    version_rolling: bool,
    version_rolling_mask: HexU32Be,
    version_rolling_min_bit_count: HexU32Be,
}

impl VersionRollingParams {
    pub fn new(version_rolling_mask: HexU32Be, version_rolling_min_bit_count: HexU32Be) -> Self {
        VersionRollingParams {
            version_rolling: true,
            version_rolling_mask,
            version_rolling_min_bit_count,
        }
    }
}

impl TryFrom<VersionRollingParams> for serde_json::Map<String, Value> {
    type Error = ();

    fn try_from(vp: VersionRollingParams) -> Result<Self, ()> {
        let version_rolling: Value = vp.version_rolling.into();
        let version_rolling_mask: Value = vp.version_rolling_mask.try_into().map_err(|_| ())?;
        let version_rolling_min_bit_count: Value = vp
            .version_rolling_min_bit_count
            .try_into()
            .map_err(|_| ())?;
        let mut params = serde_json::Map::new();
        params.insert("version-rolling".to_string(), version_rolling);
        params.insert("version-rolling.mask".to_string(), version_rolling_mask);
        params.insert(
            "version-rolling.min-bit-count".to_string(),
            version_rolling_min_bit_count,
        );
        Ok(params)
    }
}
