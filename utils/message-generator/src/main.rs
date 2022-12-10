mod executor;
mod external_commands;
mod net;
mod parser;

#[macro_use]
extern crate load_file;

use binary_sv2::{Deserialize, GetSize, Serialize};
use codec_sv2::{
    noise_sv2::formats::{EncodedEd25519PublicKey, EncodedEd25519SecretKey},
    Frame, StandardEitherFrame as EitherFrame, Sv2Frame,
};
use external_commands::*;
use net::{setup_as_downstream, setup_as_upstream};
use roles_logic_sv2::{common_messages_sv2::SetupConnectionSuccess, parsers::AnyMessage};
use std::net::SocketAddr;
//use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};
use serde_json;
use tracing::{debug, info};

// TODO RR: Move to `actions.rs`
/// Represents a valid SV2 type used in the `"value"` key-pair of the `"results"` key value of the
/// `"actions"` key value. It is used when the `"results"`'s `"type"` is `"match_message_field"`.
/// For example:
///    ```
///    {
///      "type": "match_message_field",
///        "value": [
///          "MiningProtocol",
///          "OpenStandardMiningChannelSuccess",
///          "request_id",
///          { "U32": 89 }
///        ]
///     }
///    ```
///    where `{ "U32": 89 }` is serialized as `Sv2Type::U32(89)`.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
enum Sv2Type {
    /// Message field `bool` value
    Bool(bool),
    /// Message field `u8` value
    U8(u8),
    /// Message field `u16` value
    U16(u16),
    /// Message field `u24` value as a vector of `u8`'s
    /// TODO: Change variant to `U24Vec(Vec<u8>)`?
    U24(Vec<u8>),
    /// Message field `u32` value
    U32(u32),
    /// Message field `u256` value as a vector of `u8`'s
    /// TODO: Change variant to `U256Vec(Vec<u8>)`?
    U256(Vec<u8>),
    /// Message field `255` byte-len `String` value
    Str0255(String),
    /// Message field 8-bit byte array
    B0255(Vec<u8>),
    /// Message field 16-bit byte array
    B064K(Vec<u8>),
    /// Message field 16-bit byte array of `u24`s
    // TODO Q: should this be `Vec<u24>` as according to spec definition of `B0_16M`?
    B016m(Vec<u8>),
    /// Message field X coordinate of Secp256k1 public key (see BIP 340)
    Pubkey(Vec<u8>),
    /// Message field 8-bit byte array with length from 0 to 255 bits
    Seq0255(Vec<Vec<u8>>),
    /// Message field 16-bit byte array with length from 0 to 65535 bits
    Seq064k(Vec<Vec<u8>>),
}

// TODO RR: Move to `actions.rs`
/// Represents the result of a specified `Action` defined in `test.json` configuration file as the
/// `"results"` key-pair within the `"actions"` key value. It represents the expected response(s)
/// to a specified `PoolMessages` message.
/// For example, if the `SetupConnection` is present in the `"common_messages"` key, one will
/// expect a `SetupConnectionSuccess` in response. This expected response should be listed as an
/// array entry in the `"results"` key value.
///
/// A user can specify if they want the message in the `"results"` key's value to be a specific
/// message type, to have certain message fields, to have a specific byte length, or to have a
/// specific extension type. This is all specified in an array entry of the `"results"` key's
/// value. This value has two key-value pairs: `"type"` and `"value"`, and can take one of the
/// following six forms:
///
/// 1. `{ "type": "match_message_type", "value": "<hex representation of message>" }`
///     Used when the expected message should have a specific message type.
///     Hex representation of messages are found
///     [here](https://github.com/stratum-mining/sv2-spec/blob/main/08-Message-Types.md).
/// 2.
///    ```
///    { "type": "match_message_field",
///      "value": [
///        "<subprotocol>",
///        "<message type>",
///        "<field name>",
///        {"<field name Sv2Type data type>": <field name value> }
///        ]
///     }
///    ```
///     Used when the expected message should contain specific fields and values.
/// 3. `{ "type": "match_message_len", "value": "<expected byte length of message>" }`
///     Used when the expected message should have a specific byte len.
/// 4. `{ "type": "match_extension_type", "value": "<hex representation of extension>" }`
///     Used when the expected message should have a specific extension type.
///     Hex representation of extensions are found
///     [here](https://github.com/stratum-mining/sv2-spec/blob/main/09-Extensions.md).
/// 5. `{ "type": "close_connection" }`
///     Used when the expected message has a bad frame (or other error) and the connection is
///     expected to close.
/// 5. `{ "type": "none" }`
///    Used when no action or check should be made on a expected message. Not typically used.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
enum ActionResult {
    /// Present if the user wants the resulting message type to match that defined in the `"value"`
    /// key's value. The message type held by `"value"` is stored in this enum variant.
    ///
    /// For example, `{ "type": "match_message_type", "value": "0x01" }` indicates that
    /// the user expects the resulting message to be of message type `0x01`, which is a
    /// `SetupConnectionSuccess` message.
    MatchMessageType(u8),
    /// Present if the user wants the resulting message fields to match that defined in the
    /// `"value"` key pair. Here, the contents of `"value"` is a string array with the subprotocol,
    /// message type, field name, and `Sv2Type` value which are stored in this enum variant.
    ///
    /// For example:
    ///    ```
    ///    {
    ///      "type": "match_message_field",
    ///        "value": [
    ///          "MiningProtocol",
    ///          "OpenStandardMiningChannelSuccess",
    ///          "request_id",
    ///          { "U32": 89 }
    ///        ]
    ///     }
    ///    ```
    ///    where `{ "U32": 89 }` is serialized as `Sv2Type::U32(89)`.
    MatchMessageField((String, String, String, Sv2Type)),
    /// Present if the user wants the resulting message length to match that defined in the
    /// `"value"` key's value.
    MatchMessageLen(usize),
    /// Present if the user wants the resulting extension type to match that defined in the
    /// `"value"` key's value.
    MatchExtensionType(u16),
    /// Present if the user wants to check if the connection is closed. Used to test that the
    /// server closes the connection on a bad frame.
    CloseConnection,
    /// Present if the user expects to receive a message but does not want/need to check this
    /// message. Not typically used.
    /// For example, `{ "type": "none"}`.
    None,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Copy)]
enum Role {
    Upstream,
    Downstream,
    Proxy,
}

#[derive(Debug, Clone)]
struct Upstream {
    addr: SocketAddr,
    keys: Option<(EncodedEd25519PublicKey, EncodedEd25519SecretKey)>,
}

#[derive(Debug, Clone)]
struct Downstream {
    addr: SocketAddr,
    key: Option<EncodedEd25519PublicKey>,
}

#[derive(Debug)]
pub struct Action<'a> {
    messages: Vec<EitherFrame<AnyMessage<'a>>>,
    result: Vec<ActionResult>,
    role: Role,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Command {
    command: String,
    args: Vec<String>,
    conditions: ExternalCommandConditions,
}

#[derive(Debug)]
pub struct Test<'a> {
    actions: Vec<Action<'a>>,
    as_upstream: Option<Upstream>,
    as_dowstream: Option<Downstream>,
    setup_commmands: Vec<Command>,
    execution_commands: Vec<Command>,
    cleanup_commmands: Vec<Command>,
}

#[tokio::main]
async fn main() {
    // Initialize logger
    tracing_subscriber::fmt::init();

    info!("Starting Message Generator");

    let args: Vec<String> = std::env::args().collect();
    debug!("Input arguments: {:?}", &args);

    let test_path = &args[1];
    // Load contents of `test.json`, then parse
    let test = load_str!(test_path);
    let test = parser::Parser::parse_test(test);
    // execture takse message and executures everything
    // then execute commonds
    // then execute all actions -> checking if the the things are true
    // if false, test will fail
    // then execute clean commands
    let executor = executor::Executor::new(test).await;
    executor.execute().await;
    println!("TEST OK");
    std::process::exit(0);
}

#[cfg(test)]
mod test {
    use super::*;
    use roles_logic_sv2::{
        common_messages_sv2::{Protocol, SetupConnection},
        mining_sv2::{CloseChannel, SetTarget},
        parsers::{CommonMessages, Mining},
    };
    use std::{convert::TryInto, io::Write};
    use tokio::join;

    #[tokio::test]
    async fn it_send_and_receive() {
        let mut childs = vec![];
        let message = CloseChannel {
            channel_id: 78,
            reason_code: "no reason".to_string().try_into().unwrap(),
        };
        let frame = Sv2Frame::from_message(
            message.clone(),
            const_sv2::MESSAGE_TYPE_CLOSE_CHANNEL,
            0,
            true,
        )
        .unwrap();
        let server_socket = SocketAddr::new("127.0.0.1".parse().unwrap(), 54254);
        let client_socket = SocketAddr::new("127.0.0.1".parse().unwrap(), 54254);
        let ((server_recv, server_send), (client_recv, client_send)) = join!(
            setup_as_upstream(server_socket, None, vec![], &mut childs),
            setup_as_downstream(client_socket, None)
        );
        server_send
            .send(frame.clone().try_into().unwrap())
            .await
            .unwrap();
        client_send
            .send(frame.clone().try_into().unwrap())
            .await
            .unwrap();
        let server_received = server_recv.recv().await.unwrap();
        let client_received = client_recv.recv().await.unwrap();
        match (server_received, client_received) {
            (EitherFrame::Sv2(mut frame1), EitherFrame::Sv2(mut frame2)) => {
                let mt1 = frame1.get_header().unwrap().msg_type();
                let mt2 = frame2.get_header().unwrap().msg_type();
                let p1 = frame1.payload();
                let p2 = frame2.payload();
                let message1: Mining = (mt1, p1).try_into().unwrap();
                let message2: Mining = (mt2, p2).try_into().unwrap();
                match (message1, message2) {
                    (Mining::CloseChannel(message1), Mining::CloseChannel(message2)) => {
                        assert!(message1.channel_id == message2.channel_id);
                        assert!(message2.channel_id == message.channel_id);
                        assert!(message1.reason_code == message2.reason_code);
                        assert!(message2.reason_code == message.reason_code);
                    }
                    _ => assert!(false),
                }
            }
            _ => assert!(false),
        }
    }

    #[test]
    fn it_create_tests_with_different_messages() {
        let message1 = CloseChannel {
            channel_id: 78,
            reason_code: "no reason".to_string().try_into().unwrap(),
        };
        let maximum_target: binary_sv2::U256 = [0; 32].try_into().unwrap();
        let message2 = SetTarget {
            channel_id: 78,
            maximum_target,
        };
        let message1 = Mining::CloseChannel(message1);
        let message2 = Mining::SetTarget(message2);
        let frame = Sv2Frame::from_message(
            message1.clone(),
            const_sv2::MESSAGE_TYPE_CLOSE_CHANNEL,
            0,
            true,
        )
        .unwrap();
        let frame = EitherFrame::Sv2(frame);
        let frame2 = Sv2Frame::from_message(
            message2.clone(),
            const_sv2::MESSAGE_TYPE_CLOSE_CHANNEL,
            0,
            true,
        )
        .unwrap();
        let frame2 = EitherFrame::Sv2(frame2);
        let _ = vec![frame, frame2];
        assert!(true)
    }

    #[tokio::test]
    async fn it_initialize_a_pool_and_connect_to_it() {
        let mut bitcoind = os_command(
            "./test/bin/bitcoind",
            vec!["--regtest", "--datadir=./test/appdata/bitcoin_data/"],
            ExternalCommandConditions::new_with_timer_secs(10)
                .continue_if_std_out_have("sv2 thread start")
                .fail_if_anything_on_std_err(),
        )
        .await;
        let mut child = os_command(
            "./test/bin/bitcoin-cli",
            vec![
                "--regtest",
                "--datadir=./test/appdata/bitcoin_data/",
                "generatetoaddress",
                "16",
                "bcrt1qttuwhmpa7a0ls5kr3ye6pjc24ng685jvdrksxx",
            ],
            ExternalCommandConditions::None,
        )
        .await;
        child.unwrap().wait().await.unwrap();
        let mut pool = os_command(
            "cargo",
            vec![
                "run",
                "-p",
                "pool",
                "--",
                "-c",
                "./roles/v2/pool/pool-config.toml",
            ],
            ExternalCommandConditions::new_with_timer_secs(10)
                .continue_if_std_out_have("Listening on"),
        )
        .await;

        let setup_connection = CommonMessages::SetupConnection(SetupConnection {
            protocol: Protocol::MiningProtocol,
            min_version: 2,
            max_version: 2,
            flags: 0,
            endpoint_host: "".to_string().try_into().unwrap(),
            endpoint_port: 0,
            vendor: "".to_string().try_into().unwrap(),
            hardware_version: "".to_string().try_into().unwrap(),
            firmware: "".to_string().try_into().unwrap(),
            device_id: "".to_string().try_into().unwrap(),
        });

        let frame = Sv2Frame::from_message(
            setup_connection.clone(),
            const_sv2::MESSAGE_TYPE_SETUP_CONNECTION,
            0,
            true,
        )
        .unwrap();

        let frame = EitherFrame::Sv2(frame);

        let pool_address = SocketAddr::new("127.0.0.1".parse().unwrap(), 34254);
        let pub_key: EncodedEd25519PublicKey = "2di19GHYQnAZJmEpoUeP7C3Eg9TCcksHr23rZCC83dvUiZgiDL"
            .to_string()
            .try_into()
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let (recv_from_pool, send_to_pool) = setup_as_downstream(pool_address, Some(pub_key)).await;
        send_to_pool.send(frame.try_into().unwrap()).await.unwrap();
        match recv_from_pool.recv().await.unwrap() {
            EitherFrame::Sv2(a) => {
                assert!(true)
            }
            _ => assert!(false),
        }
        let mut child = os_command(
            "rm",
            vec!["-rf", "./test/appdata/bitcoin_data/regtest"],
            ExternalCommandConditions::None,
        )
        .await;
        child.unwrap().wait().await.unwrap();

        // TODO not panic in network utils but return an handler
        //pool.kill().unwrap();
        //bitcoind.kill().await.unwrap();
        assert!(true)
    }

    #[tokio::test]
    async fn it_test_against_remote_endpoint() {
        let proxy = match os_command(
            "cargo",
            vec![
                "run",
                "-p",
                "mining-proxy",
                "--",
                "-c",
                "./test/config/ant-pool-config.toml",
            ],
            ExternalCommandConditions::new_with_timer_secs(10)
                .continue_if_std_out_have("PROXY INITIALIZED")
                .warn_no_panic(),
        )
        .await
        {
            Some(child) => child,
            None => {
                write!(
                    &mut std::io::stdout(),
                    "WARNING: remote not avaiable it_test_against_remote_endpoint not executed"
                )
                .unwrap();
                return;
            }
        };
        //loop {}
        let _ = os_command(
            "cargo",
            vec!["run", "-p", "mining-device"],
            ExternalCommandConditions::new_with_timer_secs(10)
                .continue_if_std_out_have("channel opened with"),
        )
        .await;
        assert!(true)
    }
}
