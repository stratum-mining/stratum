use crate::{
    external_commands::os_command,
    net::{setup_as_downstream, setup_as_upstream},
    Action, ActionResult, Command, Role, Test, Sv2Type
};
use async_channel::{Receiver, Sender};
use codec_sv2::{Frame, StandardEitherFrame as EitherFrame, Sv2Frame};
use roles_logic_sv2::parsers::AnyMessage;
use std::convert::TryInto;

use std::time::Duration;
use tokio::time::timeout;

pub struct Executor {
    send_to_down: Option<Sender<EitherFrame<AnyMessage<'static>>>>,
    recv_from_down: Option<Receiver<EitherFrame<AnyMessage<'static>>>>,
    send_to_up: Option<Sender<EitherFrame<AnyMessage<'static>>>>,
    recv_from_up: Option<Receiver<EitherFrame<AnyMessage<'static>>>>,
    actions: Vec<Action<'static>>,
    cleanup_commmands: Vec<Command>,
    process: Vec<Option<tokio::process::Child>>,
}

impl Executor {
    pub async fn new(test: Test<'static>) -> Executor {
        let mut process: Vec<Option<tokio::process::Child>> = vec![];
        for command in test.setup_commmands {
            if command.command == "kill" {
                let index: usize = command.args[0].parse().unwrap();
                let p = process[index].as_mut();
                let mut pid = p.as_ref().unwrap().id();
                // Kill process
                p.unwrap().kill().await;
                // Wait until the process is killed to move on
                while let Some(i) = pid {
                    let p = process[index].as_mut();
                    pid = p.as_ref().unwrap().id();
                    p.unwrap().kill().await;
                    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                }
                let p = process[index].as_mut();
            } else if command.command == "sleep" {
                let ms: u64 = command.args[0].parse().unwrap();
                tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
            } else {
                let p = os_command(
                    &command.command,
                    command.args.iter().map(String::as_str).collect(),
                    command.conditions,
                )
                .await;
                process.push(p);
            }
        }
        match (test.as_dowstream, test.as_upstream) {
            (Some(as_down), Some(as_up)) => {
                let (recv_from_down, send_to_down) = setup_as_upstream(
                    as_up.addr,
                    as_up.keys,
                    test.execution_commands,
                    &mut process,
                )
                .await;
                let (recv_from_up, send_to_up) =
                    setup_as_downstream(as_down.addr, as_down.key).await;
                Self {
                    send_to_down: Some(send_to_down),
                    recv_from_down: Some(recv_from_down),
                    send_to_up: Some(send_to_up),
                    recv_from_up: Some(recv_from_up),
                    actions: test.actions,
                    cleanup_commmands: test.cleanup_commmands,
                    process,
                }
            }
            (None, Some(as_up)) => {
                let (recv_from_down, send_to_down) = setup_as_upstream(
                    as_up.addr,
                    as_up.keys,
                    test.execution_commands,
                    &mut process,
                )
                .await;
                Self {
                    send_to_down: Some(send_to_down),
                    recv_from_down: Some(recv_from_down),
                    send_to_up: None,
                    recv_from_up: None,
                    actions: test.actions,
                    cleanup_commmands: test.cleanup_commmands,
                    process,
                }
            }
            (Some(as_down), None) => {
                let (recv_from_up, send_to_up) =
                    setup_as_downstream(as_down.addr, as_down.key).await;
                Self {
                    send_to_down: None,
                    recv_from_down: None,
                    send_to_up: Some(send_to_up),
                    recv_from_up: Some(recv_from_up),
                    actions: test.actions,
                    cleanup_commmands: test.cleanup_commmands,
                    process,
                }
            }
            (None, None) =>  {
                Self {
                    send_to_down: None,
                    recv_from_down: None,
                    send_to_up: None,
                    recv_from_up: None,
                    actions: test.actions,
                    cleanup_commmands: test.cleanup_commmands,
                    process,
                }
            }
        }
    }

    pub async fn execute(self) {
        for action in self.actions {
            if let Some(T) = action.actiondoc {
                println!("{}", T);
            }
            let (sender, recv) = match action.role {
                Role::Upstream => (
                    self.send_to_down
                        .as_ref()
                        .expect("Action require executor to act as upstream"),
                    self.recv_from_down
                        .as_ref()
                        .expect("Action require executor to act as upstream"),
                ),
                Role::Downstream => (
                    self.send_to_up
                        .as_ref()
                        .expect("Action require executor to act as downstream"),
                    self.recv_from_up
                        .as_ref()
                        .expect("Action require executor to act as downstream"),
                ),
                Role::Proxy => panic!("Action can be either executed as Downstream or Upstream"),
            };
            for message in action.messages {
                println!("SEND {:#?}", message);
                match sender.send(message).await {
                    Ok(_) => (),
                    Err(_) => panic!(),
                }
            }
            for result in &action.result {
                let message = recv.recv().await.unwrap();
                let mut message: Sv2Frame<AnyMessage<'static>, _> = message.try_into().unwrap();
                println!("RECV {:#?}", message);
                let header = message.get_header().unwrap();
                let payload = message.payload();
                match result {
                    ActionResult::MatchMessageType(message_type) => {
                        if header.msg_type() != *message_type {
                            println!(
                                "WRONG MESSAGE TYPE expected: {} received: {}",
                                message_type,
                                header.msg_type()
                            );
                            panic!()
                        }
                    }
                    ActionResult::MatchMessageField((
                        subprotocol,
                        message_type,
                        field_data // Vec<(String, Sv2Type)>
                    )) => {

                        if subprotocol.as_str() == "CommonMessage" {
                            match (header.msg_type(),payload).try_into() {
                                Ok(roles_logic_sv2::parsers::CommonMessages::SetupConnection(m)) => {
                                    if message_type.as_str() == "SetupConnection" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::CommonMessages::SetupConnectionError(m)) => {
                                    if message_type.as_str() == "SetupConnectionError" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::CommonMessages::SetupConnectionSuccess(m)) => {
                                    if message_type.as_str() == "SetupConnectionSuccess" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::CommonMessages::ChannelEndpointChanged(m)) => {
                                    if message_type.as_str() == "ChannelEndpointChanged" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Err(e) => panic!("{:?}", e),


                            }
                        } else if subprotocol.as_str() == "MiningProtocol" {
                            match (header.msg_type(),payload).try_into() {
                                Ok(roles_logic_sv2::parsers::Mining::OpenExtendedMiningChannel(m)) => {
                                    if message_type.as_str() == "OpenExtendedMiningChannel" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::OpenStandardMiningChannel(m)) => {
                                    if message_type.as_str() == "OpenStandardMiningChannel" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::OpenStandardMiningChannelSuccess(m)) => {
                                    if message_type.as_str() == "OpenStandardMiningChannelSuccess" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::CloseChannel(m)) => {
                                    if message_type.as_str() == "CloseChannel" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::NewMiningJob(m)) => {
                                    if message_type.as_str() == "NewMiningJob" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::NewExtendedMiningJob(m)) => {
                                    if message_type.as_str() == "NewExtendedMiningJob" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::SetTarget(m)) => {
                                    if message_type.as_str() == "SetTarget" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::SubmitSharesError(m)) => {
                                    if message_type.as_str() == "SubmitSharesError" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::SubmitSharesStandard(m)) => {
                                    if message_type.as_str() == "SubmitSharesStandard" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::SubmitSharesSuccess(m)) => {
                                    if message_type.as_str() == "SubmitSharesSuccess" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::SubmitSharesExtended(m)) => {
                                    if message_type.as_str() == "SubmitSharesExtended" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::SetCustomMiningJob(m)) => {
                                    if message_type.as_str() == "SetCustomMiningJob" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::SetCustomMiningJobError(m)) => {
                                    if message_type.as_str() == "SetCustomMiningJobError" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::OpenExtendedMiningChannelSuccess(m)) => {
                                    if message_type.as_str() == "OpenExtendedMiningChannelSuccess" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::OpenMiningChannelError(m)) => {
                                    if message_type.as_str() == "OpenMiningChannelError" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::Reconnect(m)) => {
                                    if message_type.as_str() == "Reconnect" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::SetCustomMiningJobSuccess(m)) => {
                                    if message_type.as_str() == "SetCustomMiningJobSuccess" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::SetExtranoncePrefix(m)) => {
                                    if message_type.as_str() == "SetExtranoncePrefix" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::SetGroupChannel(m)) => {
                                    if message_type.as_str() == "SetGroupChannel" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::SetNewPrevHash(m)) => {
                                    if message_type.as_str() == "SetNewPrevHash" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::UpdateChannel(m)) => {
                                    if message_type.as_str() == "UpdateChannel" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::UpdateChannelError(m)) => {
                                    if message_type.as_str() == "UpdateChannelError" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Err(e) => panic!("err {:?}",e),

                            }
                        } else if subprotocol.as_str() == "JobNegotiationProtocol" {
                            match (header.msg_type(), payload).try_into() {
                                Ok(roles_logic_sv2::parsers::JobNegotiation::SetCoinbase(m)) => {
                                    if message_type.as_str() == "SetCoinbase" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                }
                                Err(e) => panic!("err {:?}", e),
                            }
                        } else if subprotocol.as_str() == "TemplateDistributionProtocol" {
                            match (header.msg_type(),payload).try_into() {
                                Ok(roles_logic_sv2::parsers::TemplateDistribution::SubmitSolution(m)) => {
                                    if message_type.as_str() == "SubmitSolution" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::TemplateDistribution::NewTemplate(m)) => {
                                    if message_type.as_str() == "NewTemplate" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::TemplateDistribution::SetNewPrevHash(m)) => {
                                    if message_type.as_str() == "SetNewPrevHash" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::TemplateDistribution::CoinbaseOutputDataSize(m)) => {
                                    if message_type.as_str() == "CoinbaseOutputDataSize" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::TemplateDistribution::RequestTransactionData(m)) => {
                                    if message_type.as_str() == "RequestTransactionData" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::TemplateDistribution::RequestTransactionDataError(m)) => {
                                    if message_type.as_str() == "RequestTransactionDataError" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::TemplateDistribution::RequestTransactionDataSuccess(m)) => {
                                    if message_type.as_str() == "RequestTransactionDataSuccess" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg,field_data);
                                    }
                                },
                                Err(e) => panic!("err {:?}",e),
                            }
                        } else {
                            println!(
                                "match_message_field subprotocol not valid - received: {}",
                                subprotocol
                            );
                            panic!()
                        }
                        
                    }
                    ActionResult::MatchMessageLen(message_len) => {
                        if payload.len() != *message_len {
                            println!(
                                "WRONG MESSAGE len expected: {} received: {}",
                                message_len,
                                payload.len()
                            );
                            panic!()
                        }
                    }
                    ActionResult::MatchExtensionType(ext_type) => {
                        if header.ext_type() != *ext_type {
                            println!(
                                "WRONG EXTENSION TYPE expected: {} received: {}",
                                ext_type,
                                header.ext_type()
                            );
                            panic!()
                        }
                    }
                    ActionResult::CloseConnection => {
                        todo!()
                    }
                    ActionResult::None => todo!(),
                }
            }
        }
        for command in self.cleanup_commmands {
            os_command(
                &command.command,
                command.args.iter().map(String::as_str).collect(),
                command.conditions,
            )
            // Give time to the last cleanup command to return before exit from the process
            .await
            .unwrap()
            .wait()
            .await
            .unwrap();
        }
        for child in self.process {
            if let Some(mut child) = child {
                while let Some(i) = &child.id() {
                    // Sends kill signal and waits 1 second before checking to ensure child was killed
                    child.kill().await;
                    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                }
            }
        }
    }
}

fn check_msg_field(
    msg: serde_json::Value,
    field_name: &str,
    value_type: &str,
    field: &Sv2Type,
) {
    let msg = msg.as_object().unwrap();
    let value = msg.get(field_name).expect("match_message_field field name is not valid").clone();
    let value = serde_json::to_string(&value).unwrap();
    let value = format!(r#"{{"{}":{}}}"#, value_type, value);
    let value: crate::Sv2Type = serde_json::from_str(&value).unwrap();
    assert!(field == &value, "match_message_field value is incorrect. Expected = {:?}, Recieved = {:?}", field, value)
}

fn check_each_field(msg: serde_json::Value, field_info: &Vec<(String, Sv2Type)>) {

    for field in field_info {
        let value_type = serde_json::to_value(&field.1)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .next()
            .unwrap()
            .to_string();
        
        check_msg_field(msg.clone(),&field.0,&value_type,&field.1)
    }
}