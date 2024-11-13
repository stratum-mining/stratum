use crate::{
    external_commands::os_command,
    into_static::into_static,
    net::{setup_as_downstream, setup_as_upstream},
    parser::sv2_messages::ReplaceField,
    Action, ActionResult, Command, Role, SaveField, Sv2Type, Test,
};
use async_channel::{Receiver, Sender};
use binary_sv2::Serialize;
use codec_sv2::{StandardEitherFrame as EitherFrame, Sv2Frame};
use roles_logic_sv2::parsers::{self, AnyMessage};
use std::{collections::HashMap, convert::TryInto, sync::Arc};

use tracing::{debug, error, info};

pub struct Executor {
    #[allow(dead_code)]
    name: Arc<String>,
    send_to_down: Option<Sender<EitherFrame<AnyMessage<'static>>>>,
    recv_from_down: Option<Receiver<EitherFrame<AnyMessage<'static>>>>,
    send_to_up: Option<Sender<EitherFrame<AnyMessage<'static>>>>,
    recv_from_up: Option<Receiver<EitherFrame<AnyMessage<'static>>>>,
    actions: Vec<Action<'static>>,
    cleanup_commmands: Vec<Command>,
    process: Vec<Option<tokio::process::Child>>,
    save: HashMap<String, serde_json::Value>,
}

impl Executor {
    pub async fn new(test: Test<'static>, test_name: String) -> Executor {
        let save: HashMap<String, serde_json::Value> = HashMap::new();
        let mut process: Vec<Option<tokio::process::Child>> = vec![];
        for command in test.setup_commmands {
            if command.command == "kill" {
                let index: usize = command.args[0].parse().unwrap();
                let p = process[index].as_mut();
                let mut pid = p.as_ref().unwrap().id();
                // Kill process
                p.unwrap().kill().await.expect("Failed to kill process");
                // Wait until the process is killed to move on
                while pid.is_some() {
                    let p = process[index].as_mut();
                    pid = p.as_ref().unwrap().id();
                    if p.unwrap().kill().await.is_err() {
                        error!("Process already dead");
                    };
                    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                }
                let _p = process[index].as_mut();
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
                    name: Arc::new(test_name.clone()),
                    send_to_down: Some(send_to_down),
                    recv_from_down: Some(recv_from_down),
                    send_to_up: Some(send_to_up),
                    recv_from_up: Some(recv_from_up),
                    actions: test.actions.unwrap(),
                    cleanup_commmands: test.cleanup_commmands,
                    process,
                    save,
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
                    name: Arc::new(test_name.clone()),
                    send_to_down: Some(send_to_down),
                    recv_from_down: Some(recv_from_down),
                    send_to_up: None,
                    recv_from_up: None,
                    actions: test.actions.unwrap(),
                    cleanup_commmands: test.cleanup_commmands,
                    process,
                    save,
                }
            }
            (Some(as_down), None) => {
                let (recv_from_up, send_to_up) =
                    setup_as_downstream(as_down.addr, as_down.key).await;
                Self {
                    name: Arc::new(test_name.clone()),
                    send_to_down: None,
                    recv_from_down: None,
                    send_to_up: Some(send_to_up),
                    recv_from_up: Some(recv_from_up),
                    actions: test.actions.unwrap(),
                    cleanup_commmands: test.cleanup_commmands,
                    process,
                    save,
                }
            }
            (None, None) => Self {
                name: Arc::new(test_name.clone()),
                send_to_down: None,
                recv_from_down: None,
                send_to_up: None,
                recv_from_up: None,
                actions: test.actions.unwrap(),
                cleanup_commmands: test.cleanup_commmands,
                process,
                save,
            },
        }
    }

    pub async fn execute(mut self) {
        let mut success = true;
        for action in self.actions {
            if let Some(doc) = action.actiondoc {
                info!("actiondoc: {}", doc);
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
            for message_ in action.messages {
                let replace_fields = message_.2.clone();
                let message = message_.1.clone();
                let arbitrary_fields: Vec<ReplaceField> = replace_fields
                    .clone()
                    .into_iter()
                    .filter(|s| s.keyword == "ARBITRARY")
                    .collect();
                let replace_fields: Vec<ReplaceField> = replace_fields
                    .clone()
                    .into_iter()
                    .filter(|s| s.keyword != "ARBITRARY")
                    .collect();

                let message = if !arbitrary_fields.is_empty() {
                    let message = change_fields_with_arbitrary_value(message, arbitrary_fields);
                    message
                } else {
                    message
                };
                let message = if !replace_fields.is_empty() {
                    change_fields(message.clone(), replace_fields, self.save.clone())
                } else {
                    message
                };
                let frame = EitherFrame::Sv2(message.clone().try_into().unwrap());
                debug!("SEND {:#?}", message);
                match sender.send(frame).await {
                    Ok(_) => (),
                    Err(_) => panic!(),
                };
            }
            let mut rs = 0;
            for result in &action.result {
                rs += 1;
                info!(
                    "Working on result {}/{}: {}",
                    rs,
                    action.result.len(),
                    result
                );

                match result {
                    ActionResult::MatchMessageType(message_type) => {
                        let message = match recv.recv().await {
                            Ok(message) => message,
                            Err(_) => {
                                success = false;
                                error!("Connection closed before receiving the message");
                                break;
                            }
                        };

                        let message: Sv2Frame<AnyMessage<'static>, _> = message.try_into().unwrap();
                        debug!("RECV {:#?}", message);
                        let header = message.get_header().unwrap();

                        if header.msg_type() != *message_type {
                            error!(
                                "WRONG MESSAGE TYPE expected: {} received: {}",
                                message_type,
                                header.msg_type()
                            );
                            success = false;
                            break;
                        } else {
                            info!("MATCHED MESSAGE TYPE {}", message_type);
                        }
                    }
                    ActionResult::MatchMessageField((
                        subprotocol,
                        message_type,
                        field_data, // Vec<(String, Sv2Type)>
                    )) => {
                        let message = match recv.recv().await {
                            Ok(message) => message,
                            Err(_) => {
                                success = false;
                                error!("Connection closed before receiving the message");
                                break;
                            }
                        };

                        let mut message: Sv2Frame<AnyMessage<'static>, _> =
                            message.try_into().unwrap();
                        debug!("RECV {:#?}", message);
                        let header = message.get_header().unwrap();
                        let payload = message.payload();
                        if subprotocol.as_str() == "CommonMessages" {
                            match (header.msg_type(), payload).try_into() {
                                Ok(roles_logic_sv2::parsers::CommonMessages::SetupConnection(m)) => {
                                    if message_type.as_str() == "SetupConnection" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::CommonMessages::SetupConnectionError(m)) => {
                                    if message_type.as_str() == "SetupConnectionError" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::CommonMessages::SetupConnectionSuccess(m)) => {
                                    if message_type.as_str() == "SetupConnectionSuccess" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::CommonMessages::ChannelEndpointChanged(m)) => {
                                    if message_type.as_str() == "ChannelEndpointChanged" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Err(e) => panic!("{:?}", e),
                            }
                        } else if subprotocol.as_str() == "MiningProtocol" {
                            match (header.msg_type(), payload).try_into() {
                                Ok(roles_logic_sv2::parsers::Mining::OpenExtendedMiningChannel(m)) => {
                                    if message_type.as_str() == "OpenExtendedMiningChannel" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::OpenStandardMiningChannel(m)) => {
                                    if message_type.as_str() == "OpenStandardMiningChannel" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::OpenStandardMiningChannelSuccess(m)) => {
                                    if message_type.as_str() == "OpenStandardMiningChannelSuccess" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::CloseChannel(m)) => {
                                    if message_type.as_str() == "CloseChannel" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::NewMiningJob(m)) => {
                                    if message_type.as_str() == "NewMiningJob" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::NewExtendedMiningJob(m)) => {
                                    if message_type.as_str() == "NewExtendedMiningJob" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::SetTarget(m)) => {
                                    if message_type.as_str() == "SetTarget" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::SubmitSharesError(m)) => {
                                    if message_type.as_str() == "SubmitSharesError" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::SubmitSharesStandard(m)) => {
                                    if message_type.as_str() == "SubmitSharesStandard" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::SubmitSharesSuccess(m)) => {
                                    if message_type.as_str() == "SubmitSharesSuccess" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::SubmitSharesExtended(m)) => {
                                    if message_type.as_str() == "SubmitSharesExtended" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::SetCustomMiningJob(m)) => {
                                    if message_type.as_str() == "SetCustomMiningJob" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::SetCustomMiningJobError(m)) => {
                                    if message_type.as_str() == "SetCustomMiningJobError" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::OpenExtendedMiningChannelSuccess(m)) => {
                                    if message_type.as_str() == "OpenExtendedMiningChannelSuccess" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::OpenMiningChannelError(m)) => {
                                    if message_type.as_str() == "OpenMiningChannelError" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::Reconnect(m)) => {
                                    if message_type.as_str() == "Reconnect" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::SetCustomMiningJobSuccess(m)) => {
                                    if message_type.as_str() == "SetCustomMiningJobSuccess" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::SetExtranoncePrefix(m)) => {
                                    if message_type.as_str() == "SetExtranoncePrefix" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::SetGroupChannel(m)) => {
                                    if message_type.as_str() == "SetGroupChannel" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::SetNewPrevHash(m)) => {
                                    if message_type.as_str() == "SetNewPrevHash" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::UpdateChannel(m)) => {
                                    if message_type.as_str() == "UpdateChannel" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::Mining::UpdateChannelError(m)) => {
                                    if message_type.as_str() == "UpdateChannelError" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Err(e) => panic!("err {:?}", e),
                            }
                        } else if subprotocol.as_str() == "JobDeclarationProtocol" {
                            match (header.msg_type(), payload).try_into() {
                                Ok(roles_logic_sv2::parsers::JobDeclaration::AllocateMiningJobTokenSuccess(m)) => {
                                    if message_type.as_str() == "AllocateMiningJobTokenSuccess" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                }
                                Ok(roles_logic_sv2::parsers::JobDeclaration::AllocateMiningJobToken(m)) => {
                                    if message_type.as_str() == "AllocateMiningJobToken" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                }
                                Ok(roles_logic_sv2::parsers::JobDeclaration::DeclareMiningJob(m)) => {
                                    if message_type.as_str() == "DeclareMiningJob" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                }
                                Ok(roles_logic_sv2::parsers::JobDeclaration::DeclareMiningJobSuccess(m)) => {
                                    if message_type.as_str() == "DeclareMiningJobSuccess" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                }
                                Ok(roles_logic_sv2::parsers::JobDeclaration::DeclareMiningJobError(m)) => {
                                    if message_type.as_str() == "DeclareMiningJobSuccess" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                }
                                Ok(roles_logic_sv2::parsers::JobDeclaration::IdentifyTransactions(m)) => {
                                    if message_type.as_str() == "AllocateMiningJobTokenSuccess" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                }
                                Ok(roles_logic_sv2::parsers::JobDeclaration::IdentifyTransactionsSuccess(m)) => {
                                    if message_type.as_str() == "AllocateMiningJobTokenSuccess" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                }
                                Ok(roles_logic_sv2::parsers::JobDeclaration::ProvideMissingTransactions(m)) => {
                                    if message_type.as_str() == "AllocateMiningJobTokenSuccess" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                }
                                Ok(roles_logic_sv2::parsers::JobDeclaration::ProvideMissingTransactionsSuccess(m)) => {
                                    if message_type.as_str() == "AllocateMiningJobTokenSuccess" {
                                        let msg = serde_json::to_value(&m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                }
                                Ok(roles_logic_sv2::parsers::JobDeclaration::SubmitSolution(m)) => {
                                    if message_type.as_str() == "SubmitSolution" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Err(e) => panic!("err {:?}", e),
                            }
                        } else if subprotocol.as_str() == "TemplateDistributionProtocol" {
                            match (header.msg_type(), payload).try_into() {
                                Ok(roles_logic_sv2::parsers::TemplateDistribution::SubmitSolution(m)) => {
                                    if message_type.as_str() == "SubmitSolution" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::TemplateDistribution::NewTemplate(m)) => {
                                    if message_type.as_str() == "NewTemplate" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::TemplateDistribution::SetNewPrevHash(m)) => {
                                    if message_type.as_str() == "SetNewPrevHash" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::TemplateDistribution::CoinbaseOutputDataSize(m)) => {
                                    if message_type.as_str() == "CoinbaseOutputDataSize" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::TemplateDistribution::RequestTransactionData(m)) => {
                                    if message_type.as_str() == "RequestTransactionData" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::TemplateDistribution::RequestTransactionDataError(m)) => {
                                    if message_type.as_str() == "RequestTransactionDataError" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Ok(roles_logic_sv2::parsers::TemplateDistribution::RequestTransactionDataSuccess(m)) => {
                                    if message_type.as_str() == "RequestTransactionDataSuccess" {
                                        let msg = serde_json::to_value(m).unwrap();
                                        check_each_field(msg, field_data);
                                    }
                                },
                                Err(e) => panic!("err {:?}", e),
                            }
                        } else {
                            info!(
                                "match_message_field subprotocol not valid - received: {}",
                                subprotocol
                            );
                            panic!()
                        }
                    }
                    ActionResult::GetMessageField {
                        subprotocol,
                        message_type: _,
                        fields,
                    } => {
                        let message = match recv.recv().await {
                            Ok(message) => message,
                            Err(_) => {
                                success = false;
                                error!("Connection closed before receiving the message");
                                break;
                            }
                        };

                        let mut message: Sv2Frame<AnyMessage<'static>, _> =
                            message.try_into().unwrap();
                        debug!("RECV {:#?}", message);
                        let header = message.get_header().unwrap();
                        let payload = message.payload();
                        if subprotocol.as_str() == "CommonMessages" {
                            match (header.msg_type(), payload).try_into() {
                                Ok(parsers::CommonMessages::SetupConnection(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::CommonMessages::SetupConnectionError(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::CommonMessages::ChannelEndpointChanged(m)) => {
                                    let mess = serde_json::to_value(m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::CommonMessages::SetupConnectionSuccess(m)) => {
                                    let mess = serde_json::to_value(m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Err(e) => panic!("err {:?}", e),
                            }
                        } else if subprotocol.as_str() == "MiningProtocol" {
                            match (header.msg_type(), payload).try_into() {
                                Ok(parsers::Mining::OpenExtendedMiningChannel(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::Mining::OpenExtendedMiningChannelSuccess(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::Mining::OpenStandardMiningChannel(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::Mining::OpenStandardMiningChannelSuccess(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::Mining::CloseChannel(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::Mining::NewMiningJob(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::Mining::NewExtendedMiningJob(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::Mining::SetTarget(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::Mining::SubmitSharesError(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::Mining::SubmitSharesStandard(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::Mining::SubmitSharesSuccess(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::Mining::SubmitSharesExtended(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::Mining::OpenMiningChannelError(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::Mining::Reconnect(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::Mining::SetCustomMiningJobSuccess(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::Mining::SetExtranoncePrefix(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::Mining::SetGroupChannel(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::Mining::SetNewPrevHash(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::Mining::UpdateChannel(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::Mining::UpdateChannelError(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::Mining::SetCustomMiningJob(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::Mining::SetCustomMiningJobError(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Err(e) => panic!("err {:?}", e),
                            }
                        } else if subprotocol.as_str() == "JobDeclarationProtocol" {
                            match (header.msg_type(), payload).try_into() {
                                Ok(parsers::JobDeclaration::AllocateMiningJobTokenSuccess(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::JobDeclaration::AllocateMiningJobToken(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::JobDeclaration::DeclareMiningJob(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::JobDeclaration::DeclareMiningJobSuccess(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(roles_logic_sv2::parsers::JobDeclaration::DeclareMiningJobError(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(roles_logic_sv2::parsers::JobDeclaration::IdentifyTransactions(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(roles_logic_sv2::parsers::JobDeclaration::IdentifyTransactionsSuccess(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(roles_logic_sv2::parsers::JobDeclaration::ProvideMissingTransactions(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(roles_logic_sv2::parsers::JobDeclaration::ProvideMissingTransactionsSuccess(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::JobDeclaration::SubmitSolution(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Err(e) => panic!("err {:?}", e),
                            }
                        } else if subprotocol.as_str() == "TemplateDistributionProtocol" {
                            match (header.msg_type(), payload).try_into() {
                                Ok(parsers::TemplateDistribution::SubmitSolution(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::TemplateDistribution::NewTemplate(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::TemplateDistribution::SetNewPrevHash(m)) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::TemplateDistribution::CoinbaseOutputDataSize(m)) => {
                                    let mess = serde_json::to_value(m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::TemplateDistribution::RequestTransactionData(m)) => {
                                    let mess = serde_json::to_value(m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(parsers::TemplateDistribution::RequestTransactionDataError(
                                    m,
                                )) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Ok(
                                    parsers::TemplateDistribution::RequestTransactionDataSuccess(m),
                                ) => {
                                    let mess = serde_json::to_value(&m).unwrap();
                                    self.save = save_message_field(mess, self.save.clone(), fields);
                                }
                                Err(e) => panic!("err {:?}", e),
                            }
                        } else {
                            error!("GetMessageField not implemented for this protocol",);
                            panic!()
                        };
                    }
                    ActionResult::MatchMessageLen(message_len) => {
                        let message = match recv.recv().await {
                            Ok(message) => message,
                            Err(_) => {
                                success = false;
                                error!("Connection closed before receiving the message");
                                break;
                            }
                        };

                        let mut message: Sv2Frame<AnyMessage<'static>, _> =
                            message.try_into().unwrap();
                        debug!("RECV {:#?}", message);
                        let payload = message.payload();
                        if payload.len() != *message_len {
                            error!(
                                "WRONG MESSAGE len expected: {} received: {}",
                                message_len,
                                payload.len()
                            );
                            success = false;
                            break;
                        }
                    }
                    ActionResult::MatchExtensionType(ext_type) => {
                        let message = match recv.recv().await {
                            Ok(message) => message,
                            Err(_) => {
                                success = false;
                                error!("Connection closed before receiving the message");
                                break;
                            }
                        };

                        let message: Sv2Frame<AnyMessage<'static>, _> = message.try_into().unwrap();
                        debug!("RECV {:#?}", message);
                        let header = message.get_header().unwrap();
                        if header.ext_type() != *ext_type {
                            error!(
                                "WRONG EXTENSION TYPE expected: {} received: {}",
                                ext_type,
                                header.ext_type()
                            );
                            success = false;
                            break;
                        }
                    }
                    ActionResult::CloseConnection => {
                        info!(
                            "Waiting 1 sec to make sure that remote has time to close the connection"
                        );
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        if !recv.is_closed() {
                            error!("Expected connection to close, but it didn't. Test failed.");
                            success = false;
                            break;
                        }
                    }
                    ActionResult::SustainConnection => {
                        info!(
                            "Waiting 1 sec to make sure that remote has time to close the connection"
                        );
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        if recv.is_closed() {
                            error!("Expected connection to sustain, but it didn't. Test failed.");
                            success = false;
                            break;
                        }
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

        #[allow(clippy::manual_flatten)]
        for child in self.process {
            if let Some(mut child) = child {
                while child.id().is_some() {
                    // Sends kill signal and waits 1 second before checking to ensure child was
                    // killed
                    child.kill().await.expect("Failed to kill child process");
                    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                }
            }
        }
        if !success {
            panic!("test failed!!!");
        }
    }
}

fn change_fields(
    m: AnyMessage<'_>,
    replace_fields: Vec<ReplaceField>,
    values: HashMap<String, serde_json::Value>,
) -> AnyMessage<'static> {
    let mut replace_fields = replace_fields;
    let next = replace_fields
        .pop()
        .expect("replace_fields cannot be empty");
    let keyword = next.keyword;
    let field_name = next.field_name;
    let value = values
        .get(&keyword)
        .expect("value not found for the keyword");

    match m.clone() {
        AnyMessage::Common(m) => {
            let m_ = change_value_of_serde_field(m, value, field_name);
            let m_ = into_static(AnyMessage::Common(serde_json::from_str(&m_).unwrap()));
            if replace_fields.is_empty() {
                m_
            } else {
                change_fields(m_, replace_fields, values)
            }
        }
        AnyMessage::Mining(m) => {
            let m_ = change_value_of_serde_field(m, value, field_name);
            let m_ = into_static(AnyMessage::Mining(serde_json::from_str(&m_).unwrap()));
            if replace_fields.is_empty() {
                m_
            } else {
                change_fields(m_, replace_fields, values)
            }
        }
        AnyMessage::JobDeclaration(m) => {
            let m_ = change_value_of_serde_field(m, value, field_name);
            let m_ = into_static(AnyMessage::JobDeclaration(
                serde_json::from_str(&m_).unwrap(),
            ));
            if replace_fields.is_empty() {
                m_
            } else {
                change_fields(m_, replace_fields, values)
            }
        }
        AnyMessage::TemplateDistribution(m) => {
            let m_ = change_value_of_serde_field(m, value, field_name);
            let m_ = into_static(AnyMessage::TemplateDistribution(
                serde_json::from_str(&m_).unwrap(),
            ));
            if replace_fields.is_empty() {
                m_
            } else {
                change_fields(m_, replace_fields, values)
            }
        }
    }
}

fn change_value_of_serde_field<T: Serialize>(
    message: T,
    value: &serde_json::Value,
    field_name: String,
) -> String {
    let mut message_as_serde_value = serde_json::to_value(&message).unwrap();
    let path = message_as_serde_value
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    *message_as_serde_value
        .pointer_mut(&format!("/{}/{}", path, field_name.as_str()))
        .unwrap() = value.clone();
    serde_json::to_string(&message_as_serde_value).unwrap()
}

fn change_fields_with_arbitrary_value(
    m: AnyMessage<'_>,
    arbitrary_fields: Vec<ReplaceField>,
) -> AnyMessage<'_> {
    let mut replace_fields: Vec<ReplaceField> = Vec::new();
    let mut save: HashMap<String, serde_json::Value> = HashMap::new();

    for field_to_be_replaced in arbitrary_fields.iter() {
        let replace_field = ReplaceField {
            field_name: field_to_be_replaced.clone().field_name,
            keyword: field_to_be_replaced.clone().field_name,
        };
        replace_fields.push(replace_field);
        let value = get_arbitrary_message_value_from_string_id(
            m.clone(),
            field_to_be_replaced.field_name.clone(),
        );
        save.insert(field_to_be_replaced.clone().field_name, value);
    }
    change_fields(m, replace_fields, save)
}
fn save_message_field(
    mess: serde_json::Value,
    mut save: HashMap<String, serde_json::Value>,
    fields: &Vec<SaveField>,
) -> HashMap<String, serde_json::Value> {
    for field in fields {
        let key = field.keyword.clone();
        let field_name = &field.field_name;
        let to_save = message_to_value(&mess, field_name);
        save.insert(key, to_save.clone());
    }
    save
}

fn check_msg_field(msg: serde_json::Value, field_name: &str, value_type: &str, field: &Sv2Type) {
    let msg = msg.as_object().unwrap();
    let value = msg
        .get(field_name)
        .expect("match_message_field field name is not valid")
        .clone();
    let value = serde_json::to_string(&value).unwrap();
    let value = format!(r#"{{"{}":{}}}"#, value_type, value);
    let value: crate::Sv2Type = serde_json::from_str(&value).unwrap();
    assert!(
        field == &value,
        "match_message_field value is incorrect. Expected = {:?}, Recieved = {:?}",
        field,
        value
    )
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

        check_msg_field(msg.clone(), &field.0, &value_type, &field.1)
    }
}
fn message_to_value<'a>(m: &'a serde_json::Value, field: &str) -> &'a serde_json::Value {
    let msg = m.as_object().unwrap();
    let value = msg.get(field).unwrap_or_else(|| {
        panic!(
            "{}",
            format!("Fail with message {:?} for field {}", m, field).to_string()
        )
    });
    value
}

// to be unified with GetMessageField logic
fn get_arbitrary_message_value_from_string_id(
    message: AnyMessage<'_>,
    field_id: String,
) -> serde_json::Value {
    match message {
        roles_logic_sv2::parsers::PoolMessages::Common(m) => match m {
            roles_logic_sv2::parsers::CommonMessages::ChannelEndpointChanged(message) => {
                let field_id = field_id.as_str();
                if field_id == "channel_id" {
                    let value_new = Sv2Type::U32(message.channel_id).arbitrary();
                    if let Sv2Type::U32(inner) = value_new {
                        serde_json::to_value(inner).unwrap()
                    } else {
                        todo!()
                    }
                } else {
                    panic!("unknown message field");
                }
            }
            roles_logic_sv2::parsers::CommonMessages::SetupConnection(message) => {
                let field_id = field_id.as_str();
                if field_id == "flags" {
                    let value_new = Sv2Type::U32(message.flags).arbitrary();
                    if let Sv2Type::U32(inner) = value_new {
                        serde_json::to_value(inner).unwrap()
                    } else {
                        todo!()
                    }
                } else if field_id == "protocol" {
                    let value_new = Sv2Type::U8(message.protocol.into()).arbitrary();
                    if let Sv2Type::U8(inner) = value_new {
                        serde_json::to_value(inner).unwrap()
                    } else {
                        todo!()
                    }
                } else if field_id == "max_version" {
                    let value_new = Sv2Type::U16(message.max_version).arbitrary();
                    if let Sv2Type::U16(inner) = value_new {
                        serde_json::to_value(inner).unwrap()
                    } else {
                        todo!()
                    }
                } else if field_id == "min_version" {
                    let value_new = Sv2Type::U16(message.min_version).arbitrary();
                    if let Sv2Type::U16(inner) = value_new {
                        serde_json::to_value(inner).unwrap()
                    } else {
                        todo!()
                    }
                } else if field_id == "endpoint_host" {
                    let value_new = Sv2Type::B0255(message.endpoint_host.to_vec()).arbitrary();
                    if let Sv2Type::Str0255(inner) = value_new {
                        serde_json::to_value(inner).unwrap()
                    } else {
                        todo!()
                    }
                } else if field_id == "endpoint_port" {
                    let value_new = Sv2Type::U16(message.endpoint_port).arbitrary();
                    if let Sv2Type::U16(inner) = value_new {
                        serde_json::to_value(inner).unwrap()
                    } else {
                        todo!()
                    }
                } else if field_id == "vendor" {
                    let value_new = Sv2Type::B0255(message.vendor.to_vec()).arbitrary();
                    if let Sv2Type::Str0255(inner) = value_new {
                        serde_json::to_value(inner).unwrap()
                    } else {
                        todo!()
                    }
                } else if field_id == "hardware_version" {
                    let value_new = Sv2Type::B0255(message.hardware_version.to_vec()).arbitrary();
                    if let Sv2Type::Str0255(inner) = value_new {
                        serde_json::to_value(inner).unwrap()
                    } else {
                        todo!()
                    }
                } else if field_id == "firmware" {
                    let value_new = Sv2Type::B0255(message.firmware.to_vec()).arbitrary();
                    if let Sv2Type::Str0255(inner) = value_new {
                        serde_json::to_value(inner).unwrap()
                    } else {
                        todo!()
                    }
                } else if field_id == "device_id" {
                    let value_new = Sv2Type::B0255(message.device_id.to_vec()).arbitrary();
                    if let Sv2Type::Str0255(inner) = value_new {
                        serde_json::to_value(inner).unwrap()
                    } else {
                        todo!()
                    }
                } else {
                    panic!("unknown message field");
                }
            }
            roles_logic_sv2::parsers::CommonMessages::SetupConnectionError(message) => {
                let field_id = field_id.as_str();
                if field_id == "flags" {
                    let value_new = Sv2Type::U32(message.flags).arbitrary();
                    if let Sv2Type::U32(inner) = value_new {
                        serde_json::to_value(inner).unwrap()
                    } else {
                        todo!()
                    }
                } else if field_id == "error_code" {
                    let value_new = Sv2Type::B0255(message.error_code.to_vec()).arbitrary();
                    if let Sv2Type::Str0255(inner) = value_new {
                        serde_json::to_value(inner).unwrap()
                    } else {
                        todo!()
                    }
                } else {
                    panic!("unknown message field");
                }
            }
            roles_logic_sv2::parsers::CommonMessages::SetupConnectionSuccess(message) => {
                let field_id = field_id.as_str();
                if field_id == "flags" {
                    let value_new = Sv2Type::U32(message.flags).arbitrary();
                    if let Sv2Type::U32(inner) = value_new {
                        serde_json::to_value(inner).unwrap()
                    } else {
                        todo!()
                    }
                } else if field_id == "used_version" {
                    let value_new = Sv2Type::U16(message.used_version).arbitrary();
                    if let Sv2Type::U16(inner) = value_new {
                        serde_json::to_value(inner).unwrap()
                    } else {
                        todo!()
                    }
                } else {
                    panic!("unknown message field");
                }
            }
        },
        roles_logic_sv2::parsers::PoolMessages::Mining(m) => match m {
            roles_logic_sv2::parsers::Mining::CloseChannel(_) => todo!(),
            roles_logic_sv2::parsers::Mining::NewExtendedMiningJob(_) => todo!(),
            roles_logic_sv2::parsers::Mining::NewMiningJob(_) => todo!(),
            roles_logic_sv2::parsers::Mining::OpenExtendedMiningChannel(message) => {
                let field_id = field_id.as_str();
                if field_id == "request_id" {
                    let value_new = Sv2Type::U32(message.request_id).arbitrary();
                    if let Sv2Type::U32(inner) = value_new {
                        serde_json::to_value(inner).unwrap()
                    } else {
                        todo!()
                    }
                } else if field_id == "user_identity" {
                    let value_new = Sv2Type::B0255(message.user_identity.to_vec()).arbitrary();
                    if let Sv2Type::B0255(inner) = value_new {
                        serde_json::to_value(inner).unwrap()
                    } else {
                        todo!()
                    }
                } else if field_id == "nominal_hashrate" {
                    panic!("f32 not implemented yet as Sv2Type for the message generator")
                } else if field_id == "max_target" {
                    let value_new = Sv2Type::U256(message.max_target.to_vec()).arbitrary();
                    if let Sv2Type::U256(inner) = value_new {
                        serde_json::to_value(inner).unwrap()
                    } else {
                        todo!()
                    }
                } else if field_id == "min_extranonce_size" {
                    let value_new = Sv2Type::U16(message.min_extranonce_size).arbitrary();
                    if let Sv2Type::U256(inner) = value_new {
                        serde_json::to_value(inner).unwrap()
                    } else {
                        todo!()
                    }
                } else {
                    panic!("unknown message field");
                }
            }
            roles_logic_sv2::parsers::Mining::OpenExtendedMiningChannelSuccess(message) => {
                let field_id = field_id.as_str();
                if field_id == "channel_id" {
                    let value_new = Sv2Type::U32(message.channel_id).arbitrary();
                    if let Sv2Type::U32(inner) = value_new {
                        serde_json::to_value(inner).unwrap()
                    } else {
                        todo!()
                    }
                } else if field_id == "request_id" {
                    let value_new = Sv2Type::U32(message.request_id).arbitrary();
                    if let Sv2Type::U32(inner) = value_new {
                        serde_json::to_value(inner).unwrap()
                    } else {
                        todo!()
                    }
                } else if field_id == "target" {
                    let value_new = Sv2Type::U256(message.target.to_vec()).arbitrary();
                    if let Sv2Type::U256(inner) = value_new {
                        serde_json::to_value(inner).unwrap()
                    } else {
                        todo!()
                    }
                } else if field_id == "extranonce_prefix" {
                    let value_new = Sv2Type::B032(message.extranonce_prefix.to_vec()).arbitrary();
                    if let Sv2Type::U256(inner) = value_new {
                        serde_json::to_value(inner).unwrap()
                    } else {
                        todo!()
                    }
                } else {
                    panic!("unknown message field");
                }
            }
            roles_logic_sv2::parsers::Mining::OpenMiningChannelError(_) => todo!(),
            roles_logic_sv2::parsers::Mining::OpenStandardMiningChannel(_) => todo!(),
            roles_logic_sv2::parsers::Mining::OpenStandardMiningChannelSuccess(_) => todo!(),
            roles_logic_sv2::parsers::Mining::Reconnect(_) => todo!(),
            roles_logic_sv2::parsers::Mining::SetCustomMiningJob(_) => todo!(),
            roles_logic_sv2::parsers::Mining::SetCustomMiningJobError(_) => todo!(),
            roles_logic_sv2::parsers::Mining::SetCustomMiningJobSuccess(_) => todo!(),
            roles_logic_sv2::parsers::Mining::SetExtranoncePrefix(_) => todo!(),
            roles_logic_sv2::parsers::Mining::SetGroupChannel(_) => todo!(),
            roles_logic_sv2::parsers::Mining::SetNewPrevHash(_) => todo!(),
            roles_logic_sv2::parsers::Mining::SetTarget(_) => todo!(),
            roles_logic_sv2::parsers::Mining::SubmitSharesError(_) => todo!(),
            roles_logic_sv2::parsers::Mining::SubmitSharesExtended(_) => todo!(),
            roles_logic_sv2::parsers::Mining::SubmitSharesStandard(_) => todo!(),
            roles_logic_sv2::parsers::Mining::SubmitSharesSuccess(_) => todo!(),
            roles_logic_sv2::parsers::Mining::UpdateChannel(_) => todo!(),
            roles_logic_sv2::parsers::Mining::UpdateChannelError(_) => todo!(),
        },
        roles_logic_sv2::parsers::PoolMessages::JobDeclaration(_) => todo!(),
        roles_logic_sv2::parsers::PoolMessages::TemplateDistribution(_) => todo!(),
    }
}
