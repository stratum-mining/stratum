//! Wire forms that `sv1_api` accepts from peers, and the JSON it emits.

use bitcoin::hashes::hex::DisplayHex;
use serde_json::{json, Value};
use std::convert::TryFrom;
use sv1_api::{
    error::{DecodeError, DecodeErrorKind},
    utils::{Extranonce, HexU32Be},
    Authorize, ClientRequest, Configure, ConfigureExtension, ConfigureResult, Id, Message, Notify,
    Response, ServerNotification, SetDifficulty, SetExtranonce, SetVersionMask, SubmitError,
    SubmitOutcome, SubscribeResult, VersionRollingResult,
};

fn decode(line: &str) -> Message {
    Message::decode(line).unwrap_or_else(|e| panic!("{line}: {e}"))
}

fn try_client_request(line: &str) -> Result<ClientRequest, DecodeError> {
    match decode(line) {
        Message::Request(request) => ClientRequest::try_from(request),
        other => panic!("{line}: not a request: {other:?}"),
    }
}

fn client_request(line: &str) -> ClientRequest {
    try_client_request(line).unwrap_or_else(|e| panic!("{line}: {e}"))
}

fn server_notification(line: &str) -> ServerNotification {
    match decode(line) {
        Message::Request(request) => {
            ServerNotification::try_from(request).unwrap_or_else(|e| panic!("{line}: {e}"))
        }
        other => panic!("{line}: not a notification: {other:?}"),
    }
}

fn server_response(line: &str) -> Response {
    match decode(line) {
        Message::Response(response) => response,
        other => panic!("{line}: not a response: {other:?}"),
    }
}

fn kind(message: &Message) -> &'static str {
    match message {
        Message::Request(request) if request.id.is_some() => "request",
        Message::Request(_) => "notification",
        Message::Response(response) if response.result.is_ok() => "ok",
        Message::Response(_) => "error",
    }
}

fn hex(bytes: impl Into<Vec<u8>>) -> String {
    bytes.into().to_lower_hex_string()
}

fn to_json(message: impl Into<Message>) -> Value {
    serde_json::to_value(message.into()).unwrap()
}

#[test]
fn envelope_kinds() {
    let cases = [
        (
            r#"{"id":1,"method":"mining.subscribe","params":[]}"#,
            "request",
        ),
        (
            r#"{"jsonrpc":"2.0","id":1,"method":"mining.subscribe","params":[]}"#,
            "request",
        ),
        (
            r#"{"id":1,"method":"mining.subscribe","params":[],"error":null}"#,
            "request",
        ),
        (
            r#"{"id":1,"method":"mining.extranonce.subscribe","params":null}"#,
            "request",
        ),
        (
            r#"{"method":"mining.set_difficulty","params":[2]}"#,
            "notification",
        ),
        (
            r#"{"id":null,"method":"mining.set_difficulty","params":[2]}"#,
            "notification",
        ),
        (
            r#"{"id":null,"method":"mining.set_difficulty","params":[2],"result":null,"error":null}"#,
            "notification",
        ),
        (r#"{"id":1,"result":true,"error":null}"#, "ok"),
        (r#"{"id":1,"result":true}"#, "ok"),
        (
            r#"{"id":1,"result":true,"error":null,"method":null,"params":null}"#,
            "ok",
        ),
        (
            r#"{"id":1,"result":null,"error":[22,"Duplicate share",null]}"#,
            "error",
        ),
        (
            r#"{"id":1,"result":null,"error":[22,"Duplicate share"]}"#,
            "error",
        ),
        (
            r#"{"id":1,"result":null,"error":{"code":22,"message":"Duplicate share","data":null}}"#,
            "error",
        ),
    ];
    for (line, expected) in cases {
        assert_eq!(kind(&decode(line)), expected, "{line}");
    }
}

#[test]
fn request_ids() {
    for (line, id) in [
        (
            r#"{"id":1,"method":"mining.subscribe","params":[]}"#,
            Id::from(1u64),
        ),
        (
            r#"{"id":"a1","method":"mining.subscribe","params":[]}"#,
            Id::from("a1"),
        ),
    ] {
        let decoded = client_request(line);
        assert!(matches!(decoded, ClientRequest::Subscribe(..)), "{line}");
        assert_eq!(decoded.id(), &id, "{line}");
    }
}

#[test]
fn request_errors() {
    let kind = |line: &str| try_client_request(line).unwrap_err().kind;
    assert!(matches!(
        kind(r#"{"id":null,"method":"mining.submit","params":[]}"#),
        DecodeErrorKind::MissingId
    ));
    assert!(matches!(
        kind(r#"{"id":1,"method":"client.reconnect","params":[]}"#),
        DecodeErrorKind::UnknownMethod
    ));
    assert!(matches!(
        kind(r#"{"id":1,"method":"mining.notify","params":[]}"#),
        DecodeErrorKind::UnknownMethod
    ));
    assert!(matches!(
        kind(r#"{"id":1,"method":"mining.submit","params":["w","bf","00",4294967296,1]}"#),
        DecodeErrorKind::InvalidParams { field: "ntime", .. }
    ));
}

#[test]
fn responses_are_not_requests() {
    let response = decode(r#"{"id":1,"result":true,"error":null}"#);
    assert!(matches!(
        ClientRequest::try_from(response.clone()).unwrap_err().kind,
        DecodeErrorKind::UnexpectedResponse
    ));
    assert!(matches!(
        ServerNotification::try_from(response).unwrap_err().kind,
        DecodeErrorKind::UnexpectedResponse
    ));
}

#[test]
fn subscribe_forms() {
    let cases = [
        (
            r#"{"id":1,"method":"mining.subscribe","params":[]}"#,
            "",
            None,
        ),
        (
            r#"{"id":1,"method":"mining.subscribe","params":["cgminer/4.10.0"]}"#,
            "cgminer/4.10.0",
            None,
        ),
        (
            r#"{"id":1,"method":"mining.subscribe","params":["cgminer/4.10.0","08000002"]}"#,
            "cgminer/4.10.0",
            Some("08000002"),
        ),
        (
            r#"{"id":1,"method":"mining.subscribe","params":["cgminer/4.10.0","session-zz"]}"#,
            "cgminer/4.10.0",
            Some("session-zz"),
        ),
        (
            r#"{"id":1,"method":"mining.subscribe","params":["bosminer/1.0",null]}"#,
            "bosminer/1.0",
            None,
        ),
        (
            r#"{"id":1,"method":"mining.subscribe","params":["bosminer/1.0",null,"pool.example",null]}"#,
            "bosminer/1.0",
            None,
        ),
    ];
    for (line, agent, resume) in cases {
        let ClientRequest::Subscribe(id, subscribe) = client_request(line) else {
            panic!("{line}: not a subscribe");
        };
        assert_eq!(id, Id::from(1u64), "{line}");
        assert_eq!(subscribe.agent, agent, "{line}");
        assert_eq!(subscribe.subscription_id.as_deref(), resume, "{line}");
    }
}

#[test]
fn authorize_forms() {
    for (params, password) in [
        (r#"["worker.1","x"]"#, "x"),
        (r#"["worker.1"]"#, ""),
        (r#"["worker.1",null]"#, ""),
    ] {
        let line = format!(r#"{{"id":2,"method":"mining.authorize","params":{params}}}"#);
        let ClientRequest::Authorize(id, authorize) = client_request(&line) else {
            panic!("{line}: not an authorize");
        };
        assert_eq!(id, Id::from(2u64), "{line}");
        assert_eq!(authorize.worker_name, "worker.1", "{line}");
        assert_eq!(authorize.password, password, "{line}");
    }
}

#[test]
fn authorize_password_is_redacted() {
    let authorize = Authorize {
        worker_name: "w".to_string(),
        password: "hunter2".to_string(),
    };
    assert!(!format!("{authorize:?}").contains("hunter2"));
}

#[test]
fn submit_forms() {
    let cases = [
        (
            r#"{"id":4,"method":"mining.submit","params":["w","bf","0000abcd","647025b5","1d2e3f40"]}"#,
            None,
        ),
        (
            r#"{"id":4,"method":"mining.submit","params":["w","bf","0000abcd","647025b5","1d2e3f40","00002000"]}"#,
            Some(0x2000),
        ),
        (
            r#"{"id":4,"method":"mining.submit","params":["w","bf","0000abcd",1685071285,489570112]}"#,
            None,
        ),
        (
            r#"{"id":4,"method":"mining.submit","params":["w","bf","0000abcd",1685071285,489570112,"00002000"]}"#,
            Some(0x2000),
        ),
        (
            r#"{"id":4,"method":"mining.submit","params":["w","bf","0000ABCD","647025b5","1d2e3f40"]}"#,
            None,
        ),
    ];
    for (line, version_bits) in cases {
        let ClientRequest::Submit(id, submit) = client_request(line) else {
            panic!("{line}: not a submit");
        };
        assert_eq!(id, Id::from(4u64), "{line}");
        assert_eq!(submit.worker_name, "w", "{line}");
        assert_eq!(submit.job_id, "bf", "{line}");
        assert_eq!(hex(submit.extranonce2), "0000abcd", "{line}");
        assert_eq!(submit.ntime, HexU32Be(0x647025b5), "{line}");
        assert_eq!(submit.nonce, HexU32Be(0x1d2e3f40), "{line}");
        assert_eq!(submit.version_bits, version_bits.map(HexU32Be), "{line}");
    }
}

#[test]
fn submit_short_hex_is_left_padded() {
    let line = r#"{"id":4,"method":"mining.submit","params":["w","bf","00","5f5e100","1"]}"#;
    let ClientRequest::Submit(_, submit) = client_request(line) else {
        panic!("not a submit");
    };
    assert_eq!(submit.ntime, HexU32Be(0x05f5e100));
    assert_eq!(submit.nonce, HexU32Be(1));
}

#[test]
fn extranonce_subscribe_ignores_params() {
    for params in ["[]", "null", "{}", r#"["x"]"#] {
        let line =
            format!(r#"{{"id":5,"method":"mining.extranonce.subscribe","params":{params}}}"#);
        let ClientRequest::ExtranonceSubscribe(id) = client_request(&line) else {
            panic!("{line}: not an extranonce subscribe");
        };
        assert_eq!(id, Id::from(5u64), "{line}");
    }
}

#[test]
fn suggest_difficulty_accepts_any_params() {
    for (params, difficulty) in [
        ("[512]", Some(512.0)),
        ("[0.5]", Some(0.5)),
        ("[]", None),
        (r#"["1024"]"#, None),
        ("[-1]", None),
        ("null", None),
        ("{}", None),
    ] {
        let line = format!(r#"{{"id":6,"method":"mining.suggest_difficulty","params":{params}}}"#);
        let ClientRequest::SuggestDifficulty(id, suggest) = client_request(&line) else {
            panic!("{line}: not a suggest_difficulty");
        };
        assert_eq!(id, Id::from(6u64), "{line}");
        assert_eq!(suggest.difficulty, difficulty, "{line}");
    }
}

fn configure_summary(configure: &Configure) -> Vec<String> {
    configure
        .extensions
        .iter()
        .map(|extension| match extension {
            ConfigureExtension::VersionRolling(params) => format!(
                "version-rolling mask={:?} min={:?}",
                params.mask.as_ref().map(|m| m.0),
                params.min_bit_count
            ),
            ConfigureExtension::MinimumDifficulty(value) => format!("minimum-difficulty {value}"),
            ConfigureExtension::SubscribeExtranonce => "subscribe-extranonce".to_string(),
            ConfigureExtension::Info(info) => format!("info {:?}", info.connection_url),
        })
        .collect()
}

#[test]
fn configure_forms() {
    let cases: [(&str, &[&str]); 14] = [
        (
            r#"[["version-rolling"],{"version-rolling.mask":"1fffe000","version-rolling.min-bit-count":2}]"#,
            &["version-rolling mask=Some(536862720) min=Some(2)"],
        ),
        (
            r#"[["version-rolling"],{"version-rolling.mask":"1fffe000"}]"#,
            &["version-rolling mask=Some(536862720) min=None"],
        ),
        (
            r#"[["version-rolling"],{"version-rolling.mask":"1fffe000","version-rolling.min-bit-count":"00000010"}]"#,
            &["version-rolling mask=Some(536862720) min=Some(16)"],
        ),
        (
            r#"[["version-rolling"],{"version-rolling.mask":"1fffe000","version-rolling.min-bit-count":"16"}]"#,
            &["version-rolling mask=Some(536862720) min=Some(16)"],
        ),
        (r#"[["version-rolling"]]"#, &[]),
        (r#"[["version-rolling"],null]"#, &[]),
        (
            r#"[["version-rolling"],{"version-rolling.mask":"1fffe000"},"extra"]"#,
            &["version-rolling mask=Some(536862720) min=None"],
        ),
        (
            r#"[["version-rolling","version-rolling"],{"version-rolling.mask":"1fffe000","version-rolling.min-bit-count":2}]"#,
            &["version-rolling mask=Some(536862720) min=Some(2)"],
        ),
        (r#"[[""],{}]"#, &[]),
        (
            r#"[[],{"version-rolling.mask":"1fffe000"}]"#,
            &["version-rolling mask=Some(536862720) min=None"],
        ),
        (r#"[["subscribe-extranonce"]]"#, &["subscribe-extranonce"]),
        (
            r#"[["minimum-difficulty"],{"minimum-difficulty.value":1024}]"#,
            &["minimum-difficulty 1024"],
        ),
        (
            r#"[["info"],{"info.connection-url":"stratum+tcp://pool.example:3333","info.hw-version":"S19"}]"#,
            &["info Some(\"stratum+tcp://pool.example:3333\")"],
        ),
        (
            r#"[["version-rolling","minimum-difficulty","subscribe-extranonce"],{"version-rolling.mask":"ffffffff","version-rolling.min-bit-count":16,"minimum-difficulty.value":2048}]"#,
            &[
                "subscribe-extranonce",
                "version-rolling mask=Some(4294967295) min=Some(16)",
                "minimum-difficulty 2048",
            ],
        ),
    ];
    for (params, expected) in cases {
        let line = format!(r#"{{"id":7,"method":"mining.configure","params":{params}}}"#);
        let ClientRequest::Configure(id, configure) = client_request(&line) else {
            panic!("{line}: not a configure");
        };
        assert_eq!(id, Id::from(7u64), "{line}");
        assert_eq!(configure_summary(&configure), expected, "{line}");
    }
}

#[test]
fn configure_connection_url_is_redacted() {
    let line = r#"{"id":7,"method":"mining.configure","params":[["info"],{"info.connection-url":"stratum+tcp://user:secret@pool.example"}]}"#;
    assert!(!format!("{:?}", client_request(line)).contains("secret"));
}

#[test]
fn client_requests_encode_canonically() {
    for (line, canonical) in [
        (
            r#"{"id":1,"method":"mining.subscribe","params":[]}"#,
            json!({"id": 1, "method": "mining.subscribe", "params": []}),
        ),
        (
            r#"{"id":1,"method":"mining.subscribe","params":["bosminer/1.0",null,"pool.example",null]}"#,
            json!({"id": 1, "method": "mining.subscribe", "params": ["bosminer/1.0"]}),
        ),
        (
            r#"{"id":"a","method":"mining.subscribe","params":["cgminer/4.10.0","08000002"]}"#,
            json!({"id": "a", "method": "mining.subscribe", "params": ["cgminer/4.10.0", "08000002"]}),
        ),
        (
            r#"{"id":2,"method":"mining.authorize","params":["w"]}"#,
            json!({"id": 2, "method": "mining.authorize", "params": ["w", ""]}),
        ),
        (
            r#"{"id":4,"method":"mining.submit","params":["w","bf","0000ABCD",1685071285,489570112,"00002000"]}"#,
            json!({"id": 4, "method": "mining.submit", "params": ["w", "bf", "0000abcd", "647025b5", "1d2e3f40", "00002000"]}),
        ),
        (
            r#"{"id":5,"method":"mining.extranonce.subscribe","params":null}"#,
            json!({"id": 5, "method": "mining.extranonce.subscribe", "params": []}),
        ),
        (
            r#"{"id":6,"method":"mining.suggest_difficulty","params":[512]}"#,
            json!({"id": 6, "method": "mining.suggest_difficulty", "params": [512.0]}),
        ),
        (
            r#"{"id":7,"method":"mining.configure","params":[["version-rolling","minimum-difficulty"],{"version-rolling.mask":"1fffe000","version-rolling.min-bit-count":"00000002","minimum-difficulty.value":2048}]}"#,
            json!({"id": 7, "method": "mining.configure", "params": [["version-rolling", "minimum-difficulty"], {"version-rolling.mask": "1fffe000", "version-rolling.min-bit-count": 2, "minimum-difficulty.value": 2048}]}),
        ),
    ] {
        let encoded = to_json(client_request(line));
        assert_eq!(encoded, canonical, "{line}");
        client_request(&encoded.to_string());
    }
}

const NOTIFY_PARAMS: &str = r#"["bf","000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f","01000000010000","ffffffff",["2122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f40"],"20000000","1705ae3a","647025b5",true]"#;

fn notify_line() -> String {
    format!(r#"{{"id":null,"method":"mining.notify","params":{NOTIFY_PARAMS}}}"#)
}

#[test]
fn notify_form() {
    for id in [r#""id":null,"#, "", r#""id":"x","#] {
        let line = format!(r#"{{{id}"method":"mining.notify","params":{NOTIFY_PARAMS}}}"#);
        let ServerNotification::Notify(notify) = server_notification(&line) else {
            panic!("{line}: not a notify");
        };
        assert_eq!(notify.job_id, "bf", "{line}");
        assert_eq!(
            hex(notify.prev_hash.clone()),
            "03020100070605040b0a09080f0e0d0c13121110171615141b1a19181f1e1d1c",
            "{line}"
        );
        assert_eq!(hex(notify.coinbase1.clone()), "01000000010000", "{line}");
        assert_eq!(hex(notify.coinbase2.clone()), "ffffffff", "{line}");
        assert_eq!(
            notify
                .merkle_branch
                .iter()
                .cloned()
                .map(hex)
                .collect::<Vec<_>>(),
            ["2122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f40"],
            "{line}"
        );
        assert_eq!(notify.version, HexU32Be(0x20000000), "{line}");
        assert_eq!(notify.nbits, HexU32Be(0x1705ae3a), "{line}");
        assert_eq!(notify.ntime, HexU32Be(0x647025b5), "{line}");
        assert!(notify.clean_jobs, "{line}");
    }
}

fn notify() -> Notify {
    match server_notification(&notify_line()) {
        ServerNotification::Notify(notify) => notify,
        _ => panic!("not a notify"),
    }
}

#[test]
fn notify_encoding() {
    let params: Value = serde_json::from_str(NOTIFY_PARAMS).unwrap();
    assert_eq!(
        to_json(notify()),
        json!({"id": null, "method": "mining.notify", "params": params})
    );
}

#[test]
fn other_notification_forms() {
    let difficulty = |line: &str| match server_notification(line) {
        ServerNotification::SetDifficulty(set) => set.difficulty,
        _ => panic!("{line}: not a set_difficulty"),
    };
    assert_eq!(
        difficulty(r#"{"id":null,"method":"mining.set_difficulty","params":[2]}"#),
        2.0
    );
    assert_eq!(
        difficulty(r#"{"id":null,"method":"mining.set_difficulty","params":[0.5]}"#),
        0.5
    );

    let line = r#"{"id":null,"method":"mining.set_extranonce","params":["08000002",4]}"#;
    let ServerNotification::SetExtranonce(set) = server_notification(line) else {
        panic!("not a set_extranonce");
    };
    assert_eq!(hex(set.extranonce1), "08000002");
    assert_eq!(set.extranonce2_size, 4);

    let line = r#"{"id":null,"method":"mining.set_version_mask","params":["1fffe000"]}"#;
    let ServerNotification::SetVersionMask(set) = server_notification(line) else {
        panic!("not a set_version_mask");
    };
    assert_eq!(set.mask, HexU32Be(0x1fffe000));
}

#[test]
fn notification_encoding() {
    assert_eq!(
        to_json(SetDifficulty { difficulty: 2.0 }),
        json!({"id": null, "method": "mining.set_difficulty", "params": [2.0]})
    );
    assert_eq!(
        to_json(SetExtranonce {
            extranonce1: Extranonce::try_from(vec![8, 0, 0, 2]).unwrap(),
            extranonce2_size: 4,
        }),
        json!({"id": null, "method": "mining.set_extranonce", "params": ["08000002", 4]})
    );
    assert_eq!(
        to_json(SetVersionMask {
            mask: HexU32Be(0x1fffe000)
        }),
        json!({"id": null, "method": "mining.set_version_mask", "params": ["1fffe000"]})
    );
    for notification in [
        ServerNotification::Notify(notify()),
        ServerNotification::SetDifficulty(SetDifficulty { difficulty: 0.5 }),
        ServerNotification::SetVersionMask(SetVersionMask {
            mask: HexU32Be(0x1fffe000),
        }),
    ] {
        let line = to_json(notification.clone()).to_string();
        assert_eq!(server_notification(&line), notification, "{line}");
    }
}

#[test]
#[should_panic(expected = "finite and positive")]
fn set_difficulty_rejects_infinity() {
    to_json(SetDifficulty {
        difficulty: f64::INFINITY,
    });
}

#[test]
#[should_panic(expected = "finite and positive")]
fn set_difficulty_rejects_zero() {
    to_json(SetDifficulty { difficulty: 0.0 });
}

#[test]
fn subscribe_result_form() {
    let line = r#"{"id":1,"result":[[["mining.set_difficulty","1"],["mining.notify","ae68"]],"08000002",4],"error":null}"#;
    let response = server_response(line);
    assert_eq!(response.id, Some(Id::from(1u64)));
    let result: SubscribeResult = response.result_as().unwrap();
    assert_eq!(
        result.subscriptions,
        [
            ("mining.set_difficulty".to_string(), "1".to_string()),
            ("mining.notify".to_string(), "ae68".to_string()),
        ]
    );
    assert_eq!(hex(result.extranonce1), "08000002");
    assert_eq!(result.extranonce2_size, 4);
}

#[test]
fn configure_result_forms() {
    let cases = [
        (
            r#"{"id":7,"result":{"version-rolling":true,"version-rolling.mask":"1fffe000"},"error":null}"#,
            Some((true, 0x1fffe000, 0)),
            None,
        ),
        (
            r#"{"id":7,"result":{"version-rolling":true,"version-rolling.mask":"1fffe000","version-rolling.min-bit-count":"00000002","minimum-difficulty":false},"error":null}"#,
            Some((true, 0x1fffe000, 2)),
            Some(false),
        ),
        (
            r#"{"id":7,"result":{"version-rolling":true,"version-rolling.mask":"1fffe000","version-rolling.min-bit-count":2},"error":null}"#,
            Some((true, 0x1fffe000, 2)),
            None,
        ),
        (
            r#"{"id":7,"result":{"minimum-difficulty":true},"error":null}"#,
            None,
            Some(true),
        ),
    ];
    for (line, version_rolling, minimum_difficulty) in cases {
        let result: ConfigureResult = server_response(line).result_as().unwrap();
        assert_eq!(
            result
                .version_rolling
                .map(|v| (v.enabled, v.mask.0, v.min_bit_count)),
            version_rolling,
            "{line}"
        );
        assert_eq!(result.minimum_difficulty, minimum_difficulty, "{line}");
    }
}

#[test]
fn boolean_result_form() {
    for (line, expected) in [
        (r#"{"id":3,"result":true,"error":null}"#, true),
        (r#"{"id":3,"result":false,"error":null}"#, false),
    ] {
        let response = server_response(line);
        assert_eq!(response.id, Some(Id::from(3u64)), "{line}");
        assert_eq!(response.result_as::<bool>().unwrap(), expected, "{line}");
    }
}

#[test]
fn server_response_encoding() {
    let response = |response: Response| serde_json::to_value(Message::Response(response)).unwrap();

    assert_eq!(
        response(Response::success(
            Id::from(1u64),
            SubscribeResult {
                subscriptions: vec![("mining.notify".to_string(), "ae68".to_string())],
                extranonce1: Extranonce::try_from(vec![8, 0, 0, 2]).unwrap(),
                extranonce2_size: 4,
            },
        )),
        json!({"id": 1, "error": null, "result": [[["mining.notify", "ae68"]], "08000002", 4]})
    );

    assert_eq!(
        response(Response::success(
            Id::from(7u64),
            ConfigureResult {
                version_rolling: Some(VersionRollingResult::new(HexU32Be(0xffffffff), 2)),
                minimum_difficulty: Some(false),
            },
        )),
        json!({
            "id": 7,
            "error": null,
            "result": {
                "version-rolling": true,
                "version-rolling.mask": "1fffffe0",
                "version-rolling.min-bit-count": "00000002",
                "minimum-difficulty": false,
            },
        })
    );

    assert_eq!(
        response(Response::success(Id::from(2u64), true)),
        json!({"id": 2, "error": null, "result": true})
    );

    assert_eq!(
        response(SubmitOutcome::Accepted.into_response(Id::from(4u64))),
        json!({"id": 4, "error": null, "result": true})
    );
    assert_eq!(
        response(
            SubmitOutcome::Rejected(SubmitError::LowDifficultyShare).into_response(Id::from("s4"))
        ),
        json!({"id": "s4", "error": [23, "Low difficulty share", null], "result": null})
    );
}

#[test]
fn configure_result_mask_is_clamped() {
    let line = r#"{"id":7,"result":{"version-rolling":true,"version-rolling.mask":"ffffffff"},"error":null}"#;
    let result: ConfigureResult = server_response(line).result_as().unwrap();
    assert_eq!(result.version_rolling.unwrap().mask, HexU32Be(0x1fffffe0));
}

#[test]
fn raw_configure_hides_the_connection_url() {
    let line = r#"{"id":7,"method":"mining.configure","params":[["info"],{"info.connection-url":"stratum+tcp://user:secret@pool.example","info.hw-version":"S19"}]}"#;
    let message = decode(line);
    for formatted in [message.to_string(), format!("{message:?}")] {
        assert!(!formatted.contains("secret"), "{formatted}");
        assert!(formatted.contains("S19"), "{formatted}");
    }
}

#[test]
fn peer_strings_are_escaped_in_display() {
    let line = r#"{"id":1,"method":"evil\nmethod","params":["a\nb"]}"#;
    let message = decode(line);
    assert!(!message.to_string().contains('\n'));
    let error = try_client_request(line).unwrap_err();
    assert!(!error.to_string().contains('\n'));
    let line = r#"{"id":"x\ny","method":"mining.submit","params":[]}"#;
    assert!(!try_client_request(line)
        .unwrap_err()
        .to_string()
        .contains('\n'));
}

#[test]
fn decode_errors_do_not_grow_with_the_message() {
    let long = "m".repeat(1000);
    let error = try_client_request(&format!(
        r#"{{"id":"{long}","method":"{long}","params":[]}}"#
    ))
    .unwrap_err();
    assert!(matches!(error.kind, DecodeErrorKind::UnknownMethod));
    assert_eq!(error.id, None);
    assert_eq!(error.method, None);
}

#[test]
fn extranonce2_size_fits_a_submit() {
    for (size, accepted) in [(32, true), (33, false), (u64::MAX, false)] {
        let line = format!(
            r#"{{"id":null,"method":"mining.set_extranonce","params":["08000002",{size}]}}"#
        );
        let Message::Request(request) = decode(&line) else {
            panic!("not a request");
        };
        assert_eq!(
            ServerNotification::try_from(request).is_ok(),
            accepted,
            "{line}"
        );

        let line = format!(r#"{{"id":1,"result":[[],"08000002",{size}],"error":null}}"#);
        assert_eq!(
            server_response(&line)
                .result_as::<SubscribeResult>()
                .is_ok(),
            accepted,
            "{line}"
        );
    }
}

#[test]
fn merkle_branch_is_at_most_32_entries() {
    let node = format!("\"{}\"", "00".repeat(32));
    for (entries, accepted) in [(32, true), (33, false)] {
        let branch = vec![node.as_str(); entries].join(",");
        let line = format!(
            r#"{{"id":null,"method":"mining.notify","params":["bf","{prev}","01","02",[{branch}],"20000000","1705ae3a","647025b5",true]}}"#,
            prev = "00".repeat(32)
        );
        let Message::Request(request) = decode(&line) else {
            panic!("not a request");
        };
        assert_eq!(
            ServerNotification::try_from(request).is_ok(),
            accepted,
            "{entries} entries"
        );
    }
}
