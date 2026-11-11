//! Wire forms that `sv1_api` accepts from peers today, and the JSON it emits.

use bitcoin::hashes::hex::DisplayHex;
use serde_json::{json, Value};
use std::convert::TryFrom;
use sv1_api::{
    client_to_server::{self, ConfigureExtension, SubmitError, SubmitOutcome},
    json_rpc::Message,
    methods::{Client2Server, Server2Client, Server2ClientResponse},
    server_to_client,
    utils::{Extranonce, HexU32Be},
};

fn decode(line: &str) -> Message {
    serde_json::from_str(line).unwrap_or_else(|e| panic!("{line}: {e}"))
}

fn client_request(line: &str) -> Client2Server {
    Client2Server::try_from(decode(line)).unwrap_or_else(|e| panic!("{line}: {e:?}"))
}

fn server_notification(line: &str) -> Server2Client {
    Server2Client::try_from(decode(line)).unwrap_or_else(|e| panic!("{line}: {e:?}"))
}

fn server_response(line: &str) -> Server2ClientResponse {
    Server2ClientResponse::try_from(decode(line)).unwrap_or_else(|e| panic!("{line}: {e:?}"))
}

fn kind(message: &Message) -> &'static str {
    match message {
        Message::StandardRequest(_) => "request",
        Message::Notification(_) => "notification",
        Message::OkResponse(_) => "ok",
        Message::ErrorResponse(_) => "error",
    }
}

fn hex(bytes: impl Into<Vec<u8>>) -> String {
    bytes.into().to_lower_hex_string()
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
        let Client2Server::Subscribe(subscribe) = client_request(line) else {
            panic!("{line}: not a subscribe");
        };
        assert_eq!(subscribe.id, 1, "{line}");
        assert_eq!(subscribe.agent_signature, agent, "{line}");
        assert_eq!(subscribe.extranonce1.map(hex).as_deref(), resume, "{line}");
    }
}

#[test]
fn authorize_form() {
    let line = r#"{"id":2,"method":"mining.authorize","params":["worker.1","x"]}"#;
    let Client2Server::Authorize(authorize) = client_request(line) else {
        panic!("not an authorize");
    };
    assert_eq!(authorize.id, 2);
    assert_eq!(authorize.name, "worker.1");
    assert_eq!(authorize.password, "x");
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
        let Client2Server::Submit(submit) = client_request(line) else {
            panic!("{line}: not a submit");
        };
        assert_eq!(submit.id, 4, "{line}");
        assert_eq!(submit.user_name, "w", "{line}");
        assert_eq!(submit.job_id, "bf", "{line}");
        assert_eq!(hex(submit.extra_nonce2), "0000abcd", "{line}");
        assert_eq!(submit.time, HexU32Be(0x647025b5), "{line}");
        assert_eq!(submit.nonce, HexU32Be(0x1d2e3f40), "{line}");
        assert_eq!(submit.version_bits, version_bits.map(HexU32Be), "{line}");
    }
}

#[test]
fn submit_short_hex_is_left_padded() {
    let line = r#"{"id":4,"method":"mining.submit","params":["w","bf","00","5f5e100","1"]}"#;
    let Client2Server::Submit(submit) = client_request(line) else {
        panic!("not a submit");
    };
    assert_eq!(submit.time, HexU32Be(0x05f5e100));
    assert_eq!(submit.nonce, HexU32Be(1));
}

#[test]
fn extranonce_subscribe_ignores_params() {
    for params in ["[]", "null", "{}", r#"["x"]"#] {
        let line =
            format!(r#"{{"id":5,"method":"mining.extranonce.subscribe","params":{params}}}"#);
        let Client2Server::ExtranonceSubscribe(subscribe) = client_request(&line) else {
            panic!("{line}: not an extranonce subscribe");
        };
        assert_eq!(subscribe.id, 5, "{line}");
    }
}

#[test]
fn suggest_difficulty_accepts_any_params() {
    for params in ["[512]", "[0.5]", "[]", r#"["1024"]"#, "[-1]", "null", "{}"] {
        let line = format!(r#"{{"id":6,"method":"mining.suggest_difficulty","params":{params}}}"#);
        assert!(
            matches!(client_request(&line), Client2Server::SuggestDifficulty()),
            "{line}"
        );
    }
}

fn configure_summary(configure: &client_to_server::Configure) -> Vec<String> {
    configure
        .extensions
        .iter()
        .map(|extension| match extension {
            ConfigureExtension::VersionRolling(params) => format!(
                "version-rolling mask={:?} min={:?}",
                params.get_mask().as_ref().map(|m| m.0),
                params.get_min_bit_count().as_ref().map(|m| m.0)
            ),
            ConfigureExtension::MinimumDifficulty(value) => format!("minimum-difficulty {value}"),
            ConfigureExtension::SubcribeExtraNonce => "subscribe-extranonce".to_string(),
            ConfigureExtension::Info(info) => format!("info {:?}", info.connection_url),
        })
        .collect()
}

#[test]
fn configure_forms() {
    let cases: [(&str, &[&str]); 13] = [
        (
            r#"[["version-rolling"],{"version-rolling.mask":"1fffe000","version-rolling.min-bit-count":2}]"#,
            &["version-rolling mask=Some(536862720) min=Some(2)"],
        ),
        (
            r#"[["version-rolling"],{"version-rolling.mask":"1fffe000"}]"#,
            &["version-rolling mask=Some(536862720) min=None"],
        ),
        (
            r#"[["version-rolling"],{"version-rolling.mask":"1fffe000","version-rolling.min-bit-count":"00000002"}]"#,
            &["version-rolling mask=Some(536862720) min=Some(2)"],
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
        let Client2Server::Configure(configure) = client_request(&line) else {
            panic!("{line}: not a configure");
        };
        assert_eq!(configure.id, 7, "{line}");
        assert_eq!(configure_summary(&configure), expected, "{line}");
    }
}

const NOTIFY_PARAMS: &str = r#"["bf","000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f","01000000010000","ffffffff",["2122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f40"],"20000000","1705ae3a","647025b5",true]"#;

#[test]
fn notify_form() {
    for id in [r#""id":null,"#, "", r#""id":"x","#] {
        let line = format!(r#"{{{id}"method":"mining.notify","params":{NOTIFY_PARAMS}}}"#);
        let Server2Client::Notify(notify) = server_notification(&line) else {
            panic!("{line}: not a notify");
        };
        assert_eq!(notify.job_id, "bf", "{line}");
        assert_eq!(
            hex(notify.prev_hash.clone()),
            "03020100070605040b0a09080f0e0d0c13121110171615141b1a19181f1e1d1c",
            "{line}"
        );
        assert_eq!(hex(notify.coin_base1.clone()), "01000000010000", "{line}");
        assert_eq!(hex(notify.coin_base2.clone()), "ffffffff", "{line}");
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
        assert_eq!(notify.bits, HexU32Be(0x1705ae3a), "{line}");
        assert_eq!(notify.time, HexU32Be(0x647025b5), "{line}");
        assert!(notify.clean_jobs, "{line}");
    }
}

#[test]
fn notify_encoding() {
    let line = format!(r#"{{"id":null,"method":"mining.notify","params":{NOTIFY_PARAMS}}}"#);
    let Server2Client::Notify(notify) = server_notification(&line) else {
        panic!("not a notify");
    };
    let params: Value = serde_json::from_str(NOTIFY_PARAMS).unwrap();
    assert_eq!(
        serde_json::to_value(Message::from(notify)).unwrap(),
        json!({"method": "mining.notify", "params": params})
    );
}

#[test]
fn other_notification_forms() {
    let difficulty = |line: &str| match server_notification(line) {
        Server2Client::SetDifficulty(set) => set.value,
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
    let Server2Client::SetExtranonce(set) = server_notification(line) else {
        panic!("not a set_extranonce");
    };
    assert_eq!(hex(set.extra_nonce1), "08000002");
    assert_eq!(set.extra_nonce2_size, 4);

    let line = r#"{"id":null,"method":"mining.set_version_mask","params":["1fffe000"]}"#;
    let Server2Client::SetVersionMask(set) = server_notification(line) else {
        panic!("not a set_version_mask");
    };
    assert_eq!(set.version_mask, HexU32Be(0x1fffe000));
}

#[test]
fn notification_encoding() {
    assert_eq!(
        serde_json::to_value(Message::from(server_to_client::SetDifficulty {
            value: 2.0
        }))
        .unwrap(),
        json!({"method": "mining.set_difficulty", "params": [2.0]})
    );
    assert_eq!(
        serde_json::to_value(Message::from(server_to_client::SetExtranonce {
            extra_nonce1: Extranonce::try_from(vec![8, 0, 0, 2]).unwrap(),
            extra_nonce2_size: 4,
        }))
        .unwrap(),
        json!({"method": "mining.set_extranonce", "params": ["08000002", 4]})
    );
}

#[test]
fn subscribe_result_form() {
    let line = r#"{"id":1,"result":[[["mining.set_difficulty","1"],["mining.notify","ae68"]],"08000002",4],"error":null}"#;
    let Server2ClientResponse::Subscribe(result) = server_response(line) else {
        panic!("not a subscribe result");
    };
    assert_eq!(result.id, 1);
    assert_eq!(
        result.subscriptions,
        [
            ("mining.set_difficulty".to_string(), "1".to_string()),
            ("mining.notify".to_string(), "ae68".to_string()),
        ]
    );
    assert_eq!(hex(result.extra_nonce1), "08000002");
    assert_eq!(result.extra_nonce2_size, 4);
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
            r#"{"id":7,"result":{"minimum-difficulty":true},"error":null}"#,
            None,
            Some(true),
        ),
    ];
    for (line, version_rolling, minimum_difficulty) in cases {
        let Server2ClientResponse::Configure(result) = server_response(line) else {
            panic!("{line}: not a configure result");
        };
        assert_eq!(result.id, 7, "{line}");
        assert_eq!(
            result.version_rolling.map(|v| (
                v.version_rolling,
                v.version_rolling_mask.0,
                v.version_rolling_min_bit_count.0
            )),
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
        let Server2ClientResponse::GeneralResponse(result) = server_response(line) else {
            panic!("{line}: not a boolean result");
        };
        assert_eq!(result.id, 3, "{line}");
        assert_eq!(result.result, expected, "{line}");
    }
}

#[test]
fn server_response_encoding() {
    let Client2Server::Subscribe(subscribe) =
        client_request(r#"{"id":1,"method":"mining.subscribe","params":["cgminer/4.10.0"]}"#)
    else {
        panic!("not a subscribe");
    };
    assert_eq!(
        serde_json::to_value(subscribe.respond(
            vec![("mining.notify".to_string(), "ae68".to_string())],
            Extranonce::try_from(vec![8, 0, 0, 2]).unwrap(),
            4,
        ))
        .unwrap(),
        json!({"id": 1, "error": null, "result": [[["mining.notify", "ae68"]], "08000002", 4]})
    );

    let Client2Server::Configure(configure) = client_request(
        r#"{"id":7,"method":"mining.configure","params":[["version-rolling"],{"version-rolling.mask":"ffffffff","version-rolling.min-bit-count":2}]}"#,
    ) else {
        panic!("not a configure");
    };
    assert_eq!(
        serde_json::to_value(configure.respond(
            Some(server_to_client::VersionRollingParams::new(
                HexU32Be(0xffffffff),
                HexU32Be(2),
            )),
            Some(false),
        ))
        .unwrap(),
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

    let Client2Server::Authorize(authorize) =
        client_request(r#"{"id":2,"method":"mining.authorize","params":["w","x"]}"#)
    else {
        panic!("not an authorize");
    };
    assert_eq!(
        serde_json::to_value(authorize.respond(true)).unwrap(),
        json!({"id": 2, "error": null, "result": true})
    );

    let Client2Server::ExtranonceSubscribe(subscribe) =
        client_request(r#"{"id":5,"method":"mining.extranonce.subscribe","params":[]}"#)
    else {
        panic!("not an extranonce subscribe");
    };
    assert_eq!(
        serde_json::to_value(subscribe.respond()).unwrap(),
        json!({"id": 5, "error": null, "result": true})
    );

    let submit_line =
        r#"{"id":4,"method":"mining.submit","params":["w","bf","0000abcd","647025b5","1d2e3f40"]}"#;
    let submit = |line: &str| match client_request(line) {
        Client2Server::Submit(submit) => submit,
        _ => panic!("not a submit"),
    };
    assert_eq!(
        serde_json::to_value(submit(submit_line).respond(SubmitOutcome::Accepted)).unwrap(),
        json!({"id": 4, "error": null, "result": true})
    );
    assert_eq!(
        serde_json::to_value(
            submit(submit_line).respond(SubmitOutcome::Rejected(SubmitError::LowDifficultyShare))
        )
        .unwrap(),
        json!({"id": 4, "error": [23, "Low difficulty share", null], "result": null})
    );
}
