#![no_main]

mod common;

use std::convert::TryFrom;
use std::ops::RangeInclusive;

use arbitrary::{Arbitrary, Unstructured};
use libfuzzer_sys::fuzz_target;
use serde_json::Value;
use sv1_api::{
    json_rpc::Request, ClientRequest, ConfigureResult, Id, Method, Response, ServerNotification,
    SubscribeResult,
};

fn gen_edge_number(u: &mut Unstructured<'_>) -> arbitrary::Result<serde_json::Number> {
    match u.int_in_range(0..=9)? {
        0..=4 => {
            let n: i64 = u.arbitrary()?;
            Ok(serde_json::Number::from(n))
        }
        5..=7 => {
            let n: u64 = u.arbitrary()?;
            Ok(serde_json::Number::from(n))
        }
        _ => serde_json::Number::from_f64(u.arbitrary::<f64>()?)
            .ok_or(arbitrary::Error::NotEnoughData),
    }
}

fn gen_valid_hex(
    u: &mut Unstructured<'_>,
    len_range: RangeInclusive<usize>,
) -> arbitrary::Result<String> {
    let len = u.int_in_range(len_range)?;
    let mut nibbles = String::with_capacity(len);
    for _ in 0..len {
        match u.int_in_range(0..=15)? {
            0..=9 => nibbles.push((b'0' + u.int_in_range::<u8>(0..=9)?) as char),
            _ => nibbles.push((b'a' + u.int_in_range::<u8>(0..=5)?) as char),
        }
    }
    Ok(nibbles)
}

fn gen_submit_params(u: &mut Unstructured<'_>) -> arbitrary::Result<Value> {
    let n = u.int_in_range(5..=6)?;
    let mut arr = Vec::with_capacity(n);
    arr.push(Value::String(u.arbitrary()?));
    arr.push(Value::String(u.arbitrary()?));
    arr.push(Value::String(gen_valid_hex(u, 1..=8)?));
    arr.push(Value::Number(gen_edge_number(u)?));
    arr.push(Value::Number(gen_edge_number(u)?));
    if n == 6 {
        arr.push(Value::String(gen_valid_hex(u, 1..=8)?));
    }
    Ok(Value::Array(arr))
}

fn gen_subscribe_params(u: &mut Unstructured<'_>) -> arbitrary::Result<Value> {
    match u.int_in_range(0..=4)? {
        0 => Ok(Value::Array(vec![
            Value::String(u.arbitrary()?),
            Value::Null,
            Value::String(u.arbitrary()?),
            Value::Null,
        ])),
        1 => Ok(Value::Array(vec![
            Value::String(u.arbitrary()?),
            Value::Null,
        ])),
        2 => Ok(Value::Array(vec![
            Value::String(u.arbitrary()?),
            Value::String(gen_valid_hex(u, 1..=8)?),
        ])),
        3 => Ok(Value::Array(vec![Value::String(u.arbitrary()?)])),
        4 | _ => Ok(Value::Array(vec![])),
    }
}

fn gen_authorize_params(u: &mut Unstructured<'_>) -> arbitrary::Result<Value> {
    Ok(Value::Array(vec![
        Value::String(u.arbitrary()?),
        Value::String(u.arbitrary()?),
    ]))
}

fn gen_configure_params(u: &mut Unstructured<'_>) -> arbitrary::Result<Value> {
    let mask = gen_valid_hex(u, 1..=8)?;
    let min_bit = gen_edge_number(u)?;
    Ok(serde_json::json!([
        ["version-rolling"],
        {"version-rolling.mask": mask, "version-rolling.min-bit-count": min_bit}
    ]))
}

fn gen_notify_params(u: &mut Unstructured<'_>) -> arbitrary::Result<Value> {
    let branch_len = u.int_in_range::<usize>(0..=8)?;
    let branch: Vec<Value> = (0..branch_len)
        .map(|_| Value::String(gen_valid_hex(u, 64..=64).unwrap_or_default()))
        .collect();
    Ok(Value::Array(vec![
        Value::String(u.arbitrary()?),
        Value::String(gen_valid_hex(u, 64..=64)?),
        Value::String(gen_valid_hex(u, 1..=8)?),
        Value::String(gen_valid_hex(u, 1..=8)?),
        Value::Array(branch),
        Value::String(gen_valid_hex(u, 1..=8)?),
        Value::String(gen_valid_hex(u, 1..=8)?),
        Value::String(gen_valid_hex(u, 1..=8)?),
        Value::Bool(u.arbitrary()?),
    ]))
}

fn gen_set_difficulty_params(u: &mut Unstructured<'_>) -> arbitrary::Result<Value> {
    Ok(Value::Array(vec![Value::Number(gen_edge_number(u)?)]))
}

fn gen_set_extranonce_params(u: &mut Unstructured<'_>) -> arbitrary::Result<Value> {
    Ok(Value::Array(vec![
        Value::String(gen_valid_hex(u, 1..=8)?),
        Value::Number(gen_edge_number(u)?),
    ]))
}

fn gen_set_version_mask_params(u: &mut Unstructured<'_>) -> arbitrary::Result<Value> {
    Ok(Value::Array(vec![Value::String(gen_valid_hex(u, 1..=8)?)]))
}

#[derive(Arbitrary, Debug)]
enum ParserTarget {
    Submit,
    Authorize,
    Subscribe,
    Configure,
    Notify,
    SetDifficulty,
    SetExtranonce,
    SetVersionMask,
    Generic,
}

fn client_request(id: u64, method: Method, params: Value) {
    let _ = ClientRequest::try_from(Request {
        id: Some(Id::from(id)),
        method,
        params,
    });
}

fn notification(method: Method, params: Value) {
    let _ = ServerNotification::try_from(Request {
        id: None,
        method,
        params,
    });
}

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);
    let Ok(target) = u.arbitrary::<ParserTarget>() else {
        return;
    };
    let Ok(id) = u.arbitrary::<u64>() else {
        return;
    };

    match target {
        ParserTarget::Submit => {
            if let Ok(params) = gen_submit_params(&mut u) {
                client_request(id, Method::Submit, params);
            }
        }
        ParserTarget::Authorize => {
            if let Ok(params) = gen_authorize_params(&mut u) {
                client_request(id, Method::Authorize, params);
            }
        }
        ParserTarget::Subscribe => {
            if let Ok(params) = gen_subscribe_params(&mut u) {
                client_request(id, Method::Subscribe, params);
            }
        }
        ParserTarget::Configure => {
            if let Ok(params) = gen_configure_params(&mut u) {
                client_request(id, Method::Configure, params);
            }
        }
        ParserTarget::Notify => {
            if let Ok(params) = gen_notify_params(&mut u) {
                notification(Method::Notify, params);
            }
        }
        ParserTarget::SetDifficulty => {
            if let Ok(params) = gen_set_difficulty_params(&mut u) {
                notification(Method::SetDifficulty, params);
            }
        }
        ParserTarget::SetExtranonce => {
            if let Ok(params) = gen_set_extranonce_params(&mut u) {
                notification(Method::SetExtranonce, params);
            }
        }
        ParserTarget::SetVersionMask => {
            if let Ok(params) = gen_set_version_mask_params(&mut u) {
                notification(Method::SetVersionMask, params);
            }
        }
        ParserTarget::Generic => {
            if let Ok(params) = common::gen_json_value(&mut u, 5) {
                for method in [
                    Method::Submit,
                    Method::Authorize,
                    Method::Subscribe,
                    Method::Configure,
                    Method::ExtranonceSubscribe,
                    Method::SuggestDifficulty,
                ] {
                    client_request(id, method, params.clone());
                }
                for method in [
                    Method::Notify,
                    Method::SetDifficulty,
                    Method::SetExtranonce,
                    Method::SetVersionMask,
                ] {
                    notification(method, params.clone());
                }
                let response = Response::success(Id::from(id), params);
                let _ = response.result_as::<SubscribeResult>();
                let _ = response.result_as::<ConfigureResult>();
                let _ = response.result_as::<bool>();
            }
        }
    }
});
