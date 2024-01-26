mod actions;
mod frames;
pub mod sv1_messages;
pub mod sv2_messages;

use crate::{parser::sv2_messages::ReplaceField, Action, Command, Sv1Action, Test, TestVersion};
use codec_sv2::{buffer_sv2::Slice, Sv2Frame};
use frames::Frames;
use roles_logic_sv2::parsers::AnyMessage;
use serde_json::{Map, Value};
use std::{collections::HashMap, convert::TryInto};
use sv2_messages::TestMessageParser;
use v1::json_rpc::StandardRequest;

use self::sv1_messages::Sv1TestMessageParser;

#[derive(Debug, Clone)]
pub enum MessageMap<'a> {
    V1MessageMap(HashMap<String, (StandardRequest, Vec<ReplaceField>)>),
    V2MessageMap(HashMap<String, (AnyMessage<'a>, Vec<ReplaceField>)>),
}

#[derive(Debug)]
pub enum ActionVec<'a> {
    Sv1Action(Vec<Sv1Action>),
    Sv2Action(Vec<Action<'a>>),
}

#[derive(Debug)]
pub enum Parser<'a> {
    /// Parses any number or combination of messages to be later used by an action identified by
    /// message id.
    /// they are saved as (field_name, keyword)
    Step1 {
        version: TestVersion,
        messages: MessageMap<'a>,
    },
    /// Serializes messages into `Sv2Frames` identified by message id.
    Step2 {
        version: TestVersion,
        messages: MessageMap<'a>,
        frames: Option<HashMap<String, Sv2Frame<AnyMessage<'a>, Slice>>>,
    },
    /// Parses the setup, execution, and cleanup shell commands, roles, and actions.
    Step3 {
        version: TestVersion,
        messages: MessageMap<'a>,
        frames: Option<HashMap<String, Sv2Frame<AnyMessage<'a>, Slice>>>,
        actions: ActionVec<'a>,
    },
    /// Prepare for execution.
    Step4(Test<'a>),
}

impl<'a> Parser<'a> {
    pub fn parse_test<'b: 'a>(test: &'b str) -> Test<'a> {
        let step1 = Self::initialize(test);
        let step2 = step1.next_step(test);
        let step3 = step2.next_step(test);
        let step4 = step3.next_step(test);
        match step4 {
            Self::Step4(test) => test,
            _ => unreachable!(),
        }
    }

    fn initialize<'b: 'a>(test: &'b str) -> Self {
        let test_map: Map<String, Value> = serde_json::from_str(test).unwrap();
        let version: TestVersion = match test_map.get("version").unwrap().as_str().unwrap() {
            "1" => TestVersion::V1,
            "2" => TestVersion::V2,
            _ => panic!("no version specified"),
        };
        let messages = match version {
            TestVersion::V1 => {
                MessageMap::V1MessageMap(Sv1TestMessageParser::from_str(test).into_map())
            }
            TestVersion::V2 => {
                MessageMap::V2MessageMap(TestMessageParser::from_str(test).into_map())
            }
        };

        Self::Step1 { version, messages }
    }

    fn next_step<'b: 'a>(self, test: &'b str) -> Self {
        match self {
            Self::Step1 { version, messages } => match messages {
                MessageMap::V1MessageMap(_) => Self::Step2 {
                    version,
                    messages,
                    frames: None,
                },
                MessageMap::V2MessageMap(m) => {
                    let (frames, messages) = Frames::from_step_1(test, m.clone());
                    Self::Step2 {
                        version,
                        messages: MessageMap::V2MessageMap(messages),
                        frames: Some(frames.frames),
                    }
                }
            },
            Self::Step2 {
                version,
                messages,
                frames,
            } => match messages {
                MessageMap::V1MessageMap(m) => {
                    let actions = actions::Sv1ActionParser::from_step_2(test, m.clone());
                    Self::Step3 {
                        version,
                        messages: MessageMap::V1MessageMap(m),
                        frames: None,
                        actions: ActionVec::Sv1Action(actions),
                    }
                }
                MessageMap::V2MessageMap(m) => {
                    let actions = actions::Sv2ActionParser::from_step_2(
                        test,
                        frames.clone().unwrap(),
                        m.clone(),
                    );
                    Self::Step3 {
                        version,
                        messages: MessageMap::V2MessageMap(m),
                        frames,
                        actions: ActionVec::Sv2Action(actions),
                    }
                }
            },
            Self::Step3 {
                version,
                messages: _,
                frames: _,
                actions,
            } => {
                let test: Map<String, Value> = serde_json::from_str(test).unwrap();
                let setup_commands = test.get("setup_commands").unwrap().as_array().unwrap();
                let execution_commands =
                    test.get("execution_commands").unwrap().as_array().unwrap();
                let cleanup_commands = test.get("cleanup_commands").unwrap().as_array().unwrap();

                let setup_commmands: Vec<Command> = setup_commands
                    .iter()
                    .map(|s| serde_json::from_value(s.clone()).unwrap())
                    .collect();
                let execution_commands: Vec<Command> = execution_commands
                    .iter()
                    .map(|s| serde_json::from_value(s.clone()).unwrap())
                    .collect();
                let cleanup_commmands: Vec<Command> = cleanup_commands
                    .iter()
                    .map(|s| serde_json::from_value(s.clone()).unwrap())
                    .collect();

                let (as_upstream, as_dowstream) = match test.get("role").unwrap().as_str().unwrap()
                {
                    "client" => {
                        let downstream = test.get("downstream").unwrap();
                        let ip = downstream.get("ip").unwrap().as_str().unwrap();
                        let port = downstream.get("port").unwrap().as_u64().unwrap() as u16;
                        let pub_key = downstream
                            .get("pub_key")
                            .map(|a| a.as_str().unwrap().to_string());
                        (
                            None,
                            Some(crate::Downstream {
                                addr: std::net::SocketAddr::new(ip.parse().unwrap(), port),
                                key: pub_key.map(|k| k.to_string().try_into().unwrap()),
                            }),
                        )
                    }
                    "server" => {
                        let upstream = test.get("upstream").unwrap();
                        let ip = upstream.get("ip").unwrap().as_str().unwrap();
                        let port = upstream.get("port").unwrap().as_u64().unwrap() as u16;
                        let pub_key = upstream
                            .get("pub_key")
                            .map(|a| a.as_str().unwrap().to_string());
                        let secret_key = upstream
                            .get("secret_key")
                            .map(|a| a.as_str().unwrap().to_string());
                        let keys = match (pub_key, secret_key) {
                            (Some(p), Some(s)) => Some((
                                p.to_string().try_into().unwrap(),
                                s.to_string().try_into().unwrap(),
                            )),
                            (None, None) => None,
                            _ => panic!(),
                        };
                        (
                            Some(crate::Upstream {
                                addr: std::net::SocketAddr::new(ip.parse().unwrap(), port),
                                keys,
                            }),
                            None,
                        )
                    }
                    "proxy" => {
                        let downstream = test.get("downstream").unwrap();
                        let ip = downstream.get("ip").unwrap().as_str().unwrap();
                        let port = downstream.get("port").unwrap().as_u64().unwrap() as u16;
                        let pub_key = downstream
                            .get("pub_key")
                            .map(|a| a.as_str().unwrap().to_string());
                        let downstream = crate::Downstream {
                            addr: std::net::SocketAddr::new(ip.parse().unwrap(), port),
                            key: pub_key.map(|k| k.to_string().try_into().unwrap()),
                        };

                        let upstream = test.get("upstream").unwrap();
                        let ip = upstream.get("ip").unwrap().as_str().unwrap();
                        let port = upstream.get("port").unwrap().as_u64().unwrap() as u16;
                        let pub_key = upstream
                            .get("pub_key")
                            .map(|a| a.as_str().unwrap().to_string());
                        let secret_key = upstream
                            .get("secret_key")
                            .map(|a| a.as_str().unwrap().to_string());
                        let keys = match (pub_key, secret_key) {
                            (Some(p), Some(s)) => Some((
                                p.to_string().try_into().unwrap(),
                                s.to_string().try_into().unwrap(),
                            )),
                            (None, None) => None,
                            _ => panic!(),
                        };
                        let upstream = crate::Upstream {
                            addr: std::net::SocketAddr::new(ip.parse().unwrap(), port),
                            keys,
                        };
                        (Some(upstream), Some(downstream))
                    }
                    "none" => (None, None),
                    role => panic!("Unknown role: {}", role),
                };

                let test = match actions {
                    ActionVec::Sv1Action(a) => Test {
                        version,
                        actions: None,
                        sv1_actions: Some(a),
                        as_upstream,
                        as_dowstream,
                        setup_commmands,
                        execution_commands,
                        cleanup_commmands,
                    },
                    ActionVec::Sv2Action(a) => Test {
                        version,
                        actions: Some(a),
                        sv1_actions: None,
                        as_upstream,
                        as_dowstream,
                        setup_commmands,
                        execution_commands,
                        cleanup_commmands,
                    },
                };

                Self::Step4(test)
            }
            Parser::Step4(test) => Parser::Step4(test),
        }
    }
}

#[cfg(test)]
mod test {
    use std::hash::Hash;

    use crate::Sv1ActionResult;

    use super::*;
    use binary_sv2::*;
    use serde_json::json;

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
    struct TestStruct<'decoder> {
        #[serde(borrow)]
        test_b016m: B016M<'decoder>,
        #[serde(borrow)]
        test_b0255: B0255<'decoder>,
        #[serde(borrow)]
        test_b032: B032<'decoder>,
        #[serde(borrow)]
        test_b064: B064K<'decoder>,
        #[serde(borrow)]
        test_seq_064k_bool: Seq064K<'decoder, bool>,
        #[serde(borrow)]
        test_seq_064k_b064k: Seq064K<'decoder, B064K<'decoder>>,
    }

    #[test]
    fn it_parse_test() {
        let test = std::fs::read_to_string("./test.json").unwrap();
        let step1 = Parser::initialize(&test);

        let step2 = step1.next_step(&test);
        let step3 = step2.next_step(&test);
        let step4 = step3.next_step(&test);
        match step4 {
            Parser::Step4(test) => {
                assert!(test.actions.unwrap().len() == 2);
                assert_eq!(test.version, TestVersion::V2);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn it_parse_sequences() {
        let test_json = r#"
        {
            "test_b016m": [1,1],
            "test_b0255": [1,1],
            "test_b032": [1,1],
            "test_b064": [1,1],
            "test_seq_064k_bool": [true,false],
            "test_seq_064k_b064k": [[1,2],[3,4]]
        }
        "#;
        let test_struct: TestStruct = serde_json::from_str(test_json).unwrap();
        assert!(test_struct.test_b016m == vec![1, 1].try_into().unwrap());
        assert!(test_struct.test_b0255 == vec![1, 1].try_into().unwrap());
        assert!(test_struct.test_b032 == vec![1, 1].try_into().unwrap());
        assert!(test_struct.test_b064 == vec![1, 1].try_into().unwrap());
        assert!(test_struct.test_b064 == vec![1, 1].try_into().unwrap());
        assert!(test_struct.test_seq_064k_bool.into_inner() == vec![true, false]);
        assert!(
            test_struct.test_seq_064k_b064k.into_inner()
                == vec![
                    vec![1, 2].try_into().unwrap(),
                    vec![3, 4].try_into().unwrap(),
                ]
        );
    }

    #[test]
    fn it_parse_sv1_messages() {
        let test_json = r#"
        {
            "version": "1",
            "sv1_messages": [
                {
                    "message": {
                        "id": 1,
                        "method": "mining.subscribe",
                        "params": ["cpuminer/1.0.0"]
                    },
                    "id": "mining.subscribe"
                },
                {
                    "message": {
                        "id": 2,
                        "method": "mining.authorize",
                        "params": ["username", "password"]
                    },
                    "id": "mining.authorize"
                }
            ]
        }
        "#;
        let step1 = Parser::initialize(test_json);
        let step2 = step1.next_step(test_json);

        let message1 = StandardRequest {
            id: 1,
            method: "mining.subscribe".to_string(),
            params: json!(["cpuminer/1.0.0".to_string()]),
        };
        let message2 = StandardRequest {
            id: 2,
            method: "mining.authorize".to_string(),
            params: json!(["username".to_string(), "password".to_string()]),
        };

        match step2 {
            Parser::Step2 {
                version,
                messages,
                frames: _,
            } => {
                assert_eq!(version, TestVersion::V1);
                match messages {
                    MessageMap::V1MessageMap(m) => {
                        assert_eq!(m.get("mining.subscribe").unwrap().0, message1);
                        assert_eq!(m.get("mining.authorize").unwrap().0, message2);
                    }
                    MessageMap::V2MessageMap(_) => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn it_parse_sv1_actions() {
        let test_json = r#"
        {
            "version": "1",
            "sv1_messages": [
                {
                    "message": {
                        "id": 1,
                        "method": "mining.subscribe",
                        "params": ["cpuminer/1.0.0"]
                    },
                    "id": "mining.subscribe"
                }
            ],
            "actions": [
                {
                    "message_ids": ["mining.subscribe"],
                    "results": [
                        {
                            "type": "match_message_id",
                            "value": 1
                        }
                    ],
                    "actiondoc": ""
                }
            ]
        }
        "#;

        let message = StandardRequest {
            id: 1,
            method: "mining.subscribe".to_string(),
            params: json!(["cpuminer/1.0.0".to_string()]),
        };
        let result = Sv1ActionResult::MatchMessageId(serde_json::to_value(1).unwrap());
        let step1 = Parser::initialize(test_json);
        let step2 = step1.next_step(test_json);
        let step3 = step2.next_step(test_json);

        match step3 {
            Parser::Step3 {
                version,
                messages: _,
                frames,
                actions,
            } => {
                assert_eq!(version, TestVersion::V1);
                assert!(frames.is_none());
                match actions {
                    ActionVec::Sv1Action(a) => {
                        assert_eq!(a.get(0).unwrap().messages.get(0).unwrap().0, message);
                        assert_eq!(a.get(0).unwrap().result.get(0).unwrap(), &result);
                    }
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
    }
}
