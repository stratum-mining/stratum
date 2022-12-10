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
use tracing::{debug, info};

/// Handles the parsing, processing, and execution as prescribed by the `test.json` file. This is
/// broken into four stages:
///
/// 1. `Step1`: Searches the parsed `test.json` string for any keys with the name
///    `common_messages`, `mining_messages`, `template_provider_messages`, and/or
///    `job_negotiation_messages`. Each optional value contains an array of
///    [`PoolMessages`](https://github.com/stratum-mining/stratum/blob/3b0f53e072adb313a3d08a4e64dc394d4c8c270d/protocols/v2/roles-logic-sv2/src/parsers.rs#L968),
///    which are then parsed into their respective message type struct(s) and stored in the
///    `TestMessageParser` struct which is held in the `Step1` enum variant. These are the messages
///    to be sent to or from the mocked role.
///
///    `PoolMessages` defines four enum variants representing groups of message types. These enum
///    variants are:
///    1. [`CommonMessages`](https://github.com/stratum-mining/stratum/blob/3b0f53e072adb313a3d08a4e64dc394d4c8c270d/protocols/v2/roles-logic-sv2/src/parsers.rs#L88)
///    2. [`Mining`](https://github.com/stratum-mining/stratum/blob/3b0f53e072adb313a3d08a4e64dc394d4c8c270d/protocols/v2/roles-logic-sv2/src/parsers.rs#L138)
///    3. [`JobNegotiation`](https://github.com/stratum-mining/stratum/blob/3b0f53e072adb313a3d08a4e64dc394d4c8c270d/protocols/v2/roles-logic-sv2/src/parsers.rs#L116)
///    4. [`TemplateDistribution`](https://github.com/stratum-mining/stratum/blob/3b0f53e072adb313a3d08a4e64dc394d4c8c270d/protocols/v2/roles-logic-sv2/src/parsers.rs#L99)
///
///    Each `PoolMessages` key contains an array of one or dicts with two key-value pairs. The
///    first is `"id"` whose value is a snake case representation of the message type. The second
///    is `"message"` whose value is a dict which always contains the `"type"` key defining the
///    camel case message type, the remaining keys defined the message fields and associated values.
///
///    For example, if the user wants to defined the `CommonMessages` types of `SetupConnection`
///    and `SetupConnectionSuccess`, and the `Mining` types of `OpenStandardMiningChannel`, the
///    format is:
///
///    ```
///    // Represents an array of `CommonMessages`
///    "common_messages": [
///        {
///            "message": {
///                // Specifies a `SetupConnection` message
///                // (RR Q: why is this needed if we have `"id": "setup_connection"`)
///                "type": "SetupConnection",
///                // `SetupConnection` message fields
///                "protocol": "MiningProtocol",
///                "min_version": 2,
///                "max_version": 2,
///                "flags": 0,
///                "endpoint_host": "",
///                "endpoint_port": 0,
///                "vendor": "",
///                "hardware_version": "",
///                "firmware": "",
///                "device_id": ""
///            },
///            // `SetupConnection` message identifier
///            "id": "setup_connection"
///        },
///        {
///            "message": {
///                // Represents a `SetupConnectionSuccess` message
///                "type": "SetupConnectionSuccess",
///                // `SetupConnection` message fields
///                "flags": 0,
///                "used_version": 2
///            },
///            // `SetupConnectionSuccess` message identifier
///            "id": "setup_connection_success"
///        }
///    ],
///    // Represents an array of one or more `Mining` messages
///    "mining_messages": [
///        {
///            "message": {
///                // Represents an `OpenStandardMiningChannel` message
///                "type": "OpenStandardMiningChannel",
///                // `OpenStandardMiningChannel` message fields
///                "request_id": 89,
///                "user_identity": "",
///                "nominal_hash_rate": 10,
///                "max_target": [1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1]
///            },
///            // `OpenStandardMiningChannel` identifier
///            "id": "open_standard_mining_channel"
///        }
///    ]
///    ```
///
///    Notice that no `TemplateDistribution` of `JobNegotiation` messages are present, so these
///    fields are simply omitted. All fields are optional.
///
/// 2. `Step2`: Serializes each `PoolMessages` stored in `Step1` into a `Sv2Frame` as prescribed by
///     the `test.json` file (either `"automatic"` or `"manual"`).
///     The insertion of these messages into `Sv2Frame`s is separated from `Step1` to provide the
///     ability for the user to have control over the frames. Specifically, being able to create a
///     bad frame for any `PoolMessage` and check that the test target handles the bad frame
///     appropriately (either expects an error or closes the connection).
///
///     The behavior of the frame is specified in the `"frame_builders"` key value pair in the
///     `test.json` file. The `"frame_builders"` value is a array of dicts, where each dict has two
///     key value pairs. The first key is `"type"` which can be set to `"automatic"` if the user
///     wants to place the `PoolMessages` into a "correct" `Sv2Frame` (the most common use), or it
///     can be set to `"manual"` if the user wants to construct their own `Sv2Frame` (typically
///     used in the case where a forced error is desired). If `"manual"` is set, the user will need
///     to provide the frame headers (`extension_type`, `msg_type`, `msg_length`) in the json dict.
///     The second key is the message identifier string, `"message_id"`, which is the connection to
///     the `PoolMessage` identifier, `"id"`, discussed in `Step1`.
///
/// 3. `Step3`: Parses and stores all the shell commands and actions specified in the `test.json`
///     file. There are five key-value pairs being parsed and stored in this step:
///
///     1. The setup logic defined in the `"setup_commands"` key-value pairs.
///   
///        The `"setup_commands"` key value pair contains shell commands to be executed before any
///        messages are sent. This typically includes:
///          1. Starting up a bitcoind node on regtest
///          2. Mining some regtest blocks using `bitcoin-cli`
///          3. Starting up a SV2 role, like the SV2 pool
///   
///        The `"setup_commands"` key value is an array of dicts each with its own shell command
///        to be executed. This dict's keys are:
///          1. `"command"`: The first argument in the shell command
///          2. `"args"`: Any remaining arguments in the shell command
///          3. `"conditions"`: TODO Q
///   
///        For example, to create a command to initialize a `bitcoind` node on start up:
///        ```
///        "setup_commands": [
///          {
///              "command": "./test/bin/bitcoind",
///              "args": ["--regtest", "--datadir=./test/appdata/bitcoin_data/"],
///              "conditions": {
///                "WithConditions": {
///                    "conditions": [
///                        {
///                          "output_string": "sv2 thread start",
///                          "output_location": "StdOut",
///                          "condition": true
///                        },
///                        {
///                          "output_string": "",
///                          "output_location": "StdErr",
///                          "condition": false
///                        }
///                    ],
///                    "timer_secs": 10,
///                    "warn_no_panic": false
///                }
///            }
///          },
///          ...
///        ```
///    2. The `"execution_commands"` key-value pairs.
///
///       The `"execution_commands"` key value pair contains shell commands to TODO: ???.
///       It uses the same key-value format as the `"setup_commands"` key-value pair.
///
///    3. Parses and stores the `"role"` key-value pair and associated connection data. This
///       key-value pair defines which role is being mocked by `message-generator`. It can be one
///       of three roles:
///         1. `"client"`: Represents a downstream role to be mocked. If present, the
///            `"downstream"` key-value pair containing endpoint connection information must also
///            be present in the `test.json` configuration.
///    
///            For example:
///            ```
///            "role": "client",
///            "downstream": {
///                "ip": "0.0.0.0",
///                "port": 34254,
///                "pub_key": "2di19GHYQnAZJmEpoUeP7C3Eg9TCcksHr23rZCC83dvUiZgiDL"
///            }
///            ```
///         2. `"proxy"`: Represents a proxy role to be mocked. If present, both a `"downstream"`
///            and `"upstream"` key-pair containing the connection information for each endpoint
///            must also be present in the `test.json` configuration.
///    
///            For example:
///            ```
///            "role": "proxy",
///            "downstream": {
///                "ip": "0.0.0.0",
///                "port": 34254,
///                "pub_key": "2di19GHYQnAZJmEpoUeP7C3Eg9TCcksHr23rZCC83dvUiZgiDL"
///            }
///            "upstream": {
///                "ip": "18.196.32.109",
///                "port": 3336,
///                "pub_key": "2di19GHYQnAZJmEpoUeP7C3Eg9TCcksHr23rZCC83dvUiZgiDL"
///            }
///            ```
///         3. `"server"`: Represents an upstream server role to be mocked. If present, the
///            `"upstream"` key-value pair containing endpoint connection information must also
///            be present in the `test.json` configuration.
///    
///            For example:
///            ```
///            "role": "server",
///            "upstream": {
///                "ip": "18.196.32.109",
///                "port": 3336,
///                "pub_key": "2di19GHYQnAZJmEpoUeP7C3Eg9TCcksHr23rZCC83dvUiZgiDL"
///            }
///            ```
///
///    4. Parses and stores the `"cleanup_commands"` key-value pairs.
///
///      The `"cleanup_commands"` key value pair contains shell commands to be executed at the
///      end of the tests, after all the actions are executed. It uses the same key-value format
///      as the `"setup_commands"` key-value pair.
///    
///    5. Parses and stores the `"actions"` key-value pairs.
///
///       The `"actions"` key value pair contains and array of dicts containing the messages to be
///       sent from the mocked role (defined in the `"role"` key-pair).
///       RR TODO: FINISH
///       parse all the actions
///       we have message_ids: put all messages that you have setup to send.
///       i fyou want to send a message and recv + send and then send anothe rmessage, you have two
///       actions the first that sends the first message and the sedond action to do the second.
///       if i want to send two messages. if you expect to receive a message only after two
///       messages, you can put two messages int he message_ids array.
///       message_ids can be none if for ex the test is mocking an upstream and the first thing
///       that happens is when downstream connects it sends a setupconnection, so if you are
///       mocking the upstream , the first action is to expect to receive setupconn message, do not
///       send anything back. so you have an action w
///
///       you can have empty message id: usefeul because if you are mocking an upstream server, you
///       expect is that you are not sending any message, you expect the client to send the
///       setupconnection. you are saying to mg the first thing you do is to receive a message.
///       how does it know which message it is whating form?
///       1. wait for message wit mesage type 0x00. after that go to second action which will have
///          setupconnection success
///          restuls is a vector, but should be a vector of vectors
///          in some cases maybe want to check more than 1 property for message received, so what
///          result should really be is a vec of cev
///
/// 4. `Step4`: Stores all parsed values from the `test.json` configuration file as `Test` struct,
///    ready for execution.
#[derive(Debug)]
pub enum Parser<'a> {
    /// Stores any number or combination of `PoolMessages` (`CommonMessage`,
    /// `JobNegotiationMessage`, `MiningMessage`, and/or `TemplateDistributionMessage`) as
    /// specified by the `test.json` file to be later used by a specified action. These are the
    /// messages that will be sent to or from the mocked role.
    ///
    /// Each value is the `PoolMessages`, and each key the message identifier so later actions can
    /// find and use it.
    Step1(HashMap<String, AnyMessage<'a>>),
    /// Transforms the `PoolMessages` from `Step1` in a `Sv2Frame`.
    Step2 {
        /// `PoolMessages` message identifier and `PoolMessages` of the messages to be sent to or
        /// from the mocked role.
        messages: HashMap<String, AnyMessage<'a>>,
        /// `PoolMessages` message identifier and `PoolMessages` of the messages to be sent from to
        /// or from the mocked role, serialized as `Sv2Frame`s.
        frames: HashMap<String, Sv2Frame<AnyMessage<'a>, Slice>>,
    },
    /// Parses and stores the `"setup_commmands"`, `"execution_commands"`, `"cleanup_commands"`,
    /// `"role"`, and `"actions"` key-value pairs  all the actions specified in the `test.json`
    /// file.
    Step3 {
        /// Mapping of `PoolMessages` message identifer and the `PoolMessages` message to be sent
        /// to or from the mocked role.
        messages: HashMap<String, AnyMessage<'a>>,
        /// Mapping of `PoolMessages` message identifer and the `PoolMessages` message to be sent
        /// to or from the mocked role, serialized as `Sv2Frame`'s.
        frames: HashMap<String, Sv2Frame<AnyMessage<'a>, Slice>>,
        /// Vector of `Actions` containing the message identifiers of the messages to execute, the
        /// expected responses of each message, and the endpoint information of the role being
        /// mocked.
        actions: Vec<Action<'a>>,
    },
    /// Stores all parsed values from the `test.json` configuration file as `Test` struct, ready
    /// for execution.
    Step4(Test<'a>),
}

impl<'a> Parser<'a> {
    /// Progresses each step of `Parser` to the next.
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
        info!("Initialize test");
        let messages = TestMessageParser::from_str(test);
        let step1 = Self::Step1(messages.into_map());
        debug!("STEP 1: {:#?}", &step1);
        step1
    }

    /// Transforms each step of `Parser` to the next.
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
                // Serializes each `PoolMessages` stored in `Step1` into a `Sv2Frame`
                let actions = actions::ActionParser::from_step_3(test, frames.clone());
                Self::Step3 {
                    messages,
                    frames,
                    actions,
                }
            }
            // Progresses from `Step3` to `Step4`
            Self::Step3 {
                messages: _,
                frames: _,
                actions,
            } => {
                // Serializes the `test.json` configuration into a HashMap
                let test: Map<String, Value> = serde_json::from_str(test).unwrap();
                // `"setup_commands"` represents all the commands that need to be executed to
                // enable the tests to run (typically starting up a regtest bitcoin node)
                let setup_commands = test.get("setup_commands").unwrap().as_array().unwrap();
                // `"execution_commands"` represents all the commands that need to be executed to
                // ??? -> RR Q: what kinds of things go in `"execution_commands"`?
                let execution_commands =
                    test.get("execution_commands").unwrap().as_array().unwrap();
                // `"cleanup_commands"` represent the logic to be ran after all the tests are
                // complete (typically removing the bitcoin node `datadir` created in the
                // `"setup_commands"`
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

                // Gets the `"role"` key's value pair and .
                // If the value is `"client", looks for `"downstream"` key-value pair which
                // contains the connection information for a downstream client.
                // If the value is `"server", looks for `"upstream"` key-value pair which
                // contains the connection information for a upstream server.
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
