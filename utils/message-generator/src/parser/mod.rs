mod actions;
mod frames;
mod sv2_messages;

use crate::{Action, Command, Test};
use codec_sv2::{buffer_sv2::Slice, Frame, Sv2Frame};
use frames::Frames;
use roles_logic_sv2::parsers::AnyMessage;
use serde_json::{Map, Value};
use std::{collections::HashMap, convert::TryInto};
use sv2_messages::TestMessageParser;
use tracing::debug;

/// Handles the parsing, processing, and execution as prescribed by the `test.json` file. This is
/// broken into four stages:
/// 1. `Step1`: Searches the parsed `test.json` `str` for any keys with the name `common_messages`,
///    `mining_messages`, `template_provider_messages`, and/or `job_negotiation_messages`. Takes
///    the message(s) values and converts them into their respective message type (i.e. if a
///    `common_messages` key is present, takes the values and creates a `CommonMessage` struct).
///    The formatted message struct(s) is then stored in the `TestMessageParser` struct.
/// 2.
#[derive(Debug)]
pub enum Parser<'a> {
    /// Stores any number or combination of `CommonMessage`, `JobNegotiationMessage`,
    /// `MiningMessage`, and/or `TemplateDistributionMessage` as specified by the `test.json` file.
    Step1(HashMap<String, AnyMessage<'a>>),
    /// transforming the messages into frames
    /// you define the messages in Step1 and build frames in Step2
    /// this is becuase sometimes you maybe want to create an invalid frame and see if the
    /// connection closes, just how the sw restpondes, the frame can build in an automatic fastion
    /// with Automatic type + libs take message build right frame for this message. or you can
    /// force a different frame using manual, and you have to put all the fields for it so yo have
    /// a frame with a headr that you can decide here
    /// you say which is the frame header
    /// header.rs -> extention, type
    /// (already have payload which is the message, specified by message in test.json)
    /// give me the right header for this payload, or i wnat to specify an inforrect header to test
    /// herrors
    Step2 {
        messages: HashMap<String, AnyMessage<'a>>,
        frames: HashMap<String, Sv2Frame<AnyMessage<'a>, Slice>>,
    },
    /// parse all the actions
    /// we have message_ids: put all messages that you have setup to send.
    /// i fyou want to send a message and recv + send and then send anothe rmessage, you have two
    /// actions the first that sends the first message and the sedond action to do the second.
    /// if i want to send two messages. if you expect to receive a message only after two messages,
    /// you can put two messages int he message_ids array.
    /// message_ids can be none if for ex the test is mocking an upstream and the first thing that
    /// happens is when downstream connects it sends a setupconnection, so if you are mocking the
    /// upstream , the first action is to expect to receive setupconn message, do not send anything
    /// back. so you have an action w
    ///
    /// you can have empty message id: usefeul because if you are mocking an upstream server, you
    /// expect is that you are not sending any message, you expect the client to send the
    /// setupconnection. you are saying to mg the first thing you do is to receive a message.
    /// how does it know which message it is whating form?
    /// 1. wait for message wit mesage type 0x00. after that go to second action which will have
    ///    setupconnection success
    ///    restuls is a vector, but should be a vector of vectors
    ///    in some cases maybe want to check more than 1 property for message received, so what
    ///    result should really be is a vec of cev
    Step3 {
        messages: HashMap<String, AnyMessage<'a>>,
        frames: HashMap<String, Sv2Frame<AnyMessage<'a>, Slice>>,
        actions: Vec<Action<'a>>,
    },
    /// parse the test + execute.
    /// parse all bash commands
    /// role: client, proxy, or server
    /// if you are a client: need to have a downstream w connection infomation
    /// if you put pubkey it iwll setup noise conn w server, if not will setup plain connection
    /// if you have client=server, you need upstream fields, if proxy need both up and down
    Step4(Test<'a>),
}

impl<'a> Parser<'a> {
    /// when you parse test with Parer you execute in main

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

    /// Initializes the first step, `Parser::Step1`. Accepts a `str` of the `test.json` file, and
    /// creates and stores the specified messages (any number or combination os `CommonMessage`,
    /// `JobNegotiationMessage`, `MiningMessage`, and/or `TemplateDistributionMessage`) and stores
    /// them in a hashmap in the `Step1` enum variant.
    fn initialize<'b: 'a>(test: &'b str) -> Self {
        debug!("INITIALIZE TEST: {:?}", &test);
        let messages = TestMessageParser::from_str(test);
        let step1 = Self::Step1(messages.into_map());
        debug!("STEP 1: {:?}", &step1);
        step1
    }

    /// Progresses each step of `Parser` to the next.
    fn next_step<'b: 'a>(self, test: &'b str) -> Self {
        match self {
            // Progresses from `Step1` to `Step2`
            Self::Step1(messages) => {
                // Puts the messages stored in the `Step1` variant and converts them into framed
                // messages which are stored in the `Step2` variant
                let frames = Frames::from_step_1(test, messages.clone());
                Self::Step2 {
                    messages,
                    frames: frames.frames,
                }
            }
            // Progresses from `Step2` to `Step3`
            Self::Step2 { messages, frames } => {
                let actions = actions::ActionParser::from_step_3(test, frames.clone());
                Self::Step3 {
                    messages,
                    frames,
                    actions,
                }
            }
            Parser::Step3 {
                messages: _,
                frames: _,
                actions,
            } => {
                let test: Map<String, Value> = serde_json::from_str(&test).unwrap();
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
                    role @ _ => panic!("Unknown role: {}", role),
                };

                let test = Test {
                    actions,
                    as_upstream,
                    as_dowstream,
                    setup_commmands,
                    execution_commands,
                    cleanup_commmands,
                };
                Self::Step4(test)
            }
            Parser::Step4(test) => Parser::Step4(test),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn it_parse_test() {
        let test = std::fs::read_to_string("./test.json").unwrap();
        let step1 = Parser::initialize(&test);

        let step2 = step1.next_step(&test);
        let step3 = step2.next_step(&test);
        let step4 = step3.next_step(&test);
        match step4 {
            Parser::Step4(test) => {
                assert!(test.actions.len() == 2);
            }
            _ => unreachable!(),
        }
    }
}
