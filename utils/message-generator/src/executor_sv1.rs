use crate::{
    external_commands::os_command, net::setup_as_sv1_downstream, Command, Sv1Action,
    Sv1ActionResult, Test,
};
use async_channel::{Receiver, Sender};
use std::{collections::HashMap, sync::Arc};
use v1::Message;

use tracing::{debug, error, info};

pub struct Sv1Executor {
    #[allow(dead_code)]
    name: Arc<String>,
    send_to_up: Option<Sender<String>>,
    recv_from_up: Option<Receiver<String>>,
    actions: Vec<Sv1Action>,
    cleanup_commmands: Vec<Command>,
    process: Vec<Option<tokio::process::Child>>,
    #[allow(dead_code)] // TODO why we have it?
    save: HashMap<String, serde_json::Value>,
}

impl Sv1Executor {
    pub async fn new(test: Test<'static>, test_name: String) -> Sv1Executor {
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
                    p.unwrap().kill().await.expect("Failed to kill process");
                    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                }
                let _p = process[index].as_mut(); // TODO why we have it?
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
            (Some(as_down), None) => {
                let (recv_from_up, send_to_up) = setup_as_sv1_downstream(as_down.addr).await;
                Self {
                    name: Arc::new(test_name.clone()),
                    send_to_up: Some(send_to_up),
                    recv_from_up: Some(recv_from_up),
                    actions: test.sv1_actions.unwrap(),
                    cleanup_commmands: test.cleanup_commmands,
                    process,
                    save,
                }
            }
            _ => unreachable!(),
        }
    }

    pub async fn execute(self) {
        let mut success = true;
        for action in self.actions {
            if let Some(doc) = action.actiondoc {
                info!("actiondoc: {}", doc);
            }
            let (sender, recv) = (
                self.send_to_up
                    .as_ref()
                    .expect("Action require executor to act as downstream"),
                self.recv_from_up
                    .as_ref()
                    .expect("Action require executor to act as downstream"),
            );
            for message_ in action.messages {
                let replace_fields = message_.1.clone();
                let message = serde_json::to_string(&message_.0).unwrap();
                let message = message + "\n";
                if replace_fields.is_empty() {
                    debug!("SEND {:#?}", message);
                    match sender.send(message).await {
                        Ok(_) => (),
                        Err(_) => panic!(),
                    }
                } else {
                    //TODO: modified message
                }
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

                // If the connection should drop at this point then let's just break the loop
                // Can't do anything else after the connection drops.
                if *result == Sv1ActionResult::CloseConnection {
                    recv.recv()
                        .await
                        .expect_err("Expecting the connection to be closed: wasn't");
                    break;
                }

                let message = match recv.recv().await {
                    Ok(message) => message,
                    Err(_) => {
                        success = false;
                        error!("Connection closed before receiving the message");
                        break;
                    }
                };
                let message: Message = serde_json::from_str(&message).unwrap();
                debug!("RECV {:#?}", message);
                match message {
                    Message::OkResponse(response) | Message::ErrorResponse(response) => {
                        match result {
                            Sv1ActionResult::MatchMessageId(message_id) => {
                                if response.id != *message_id {
                                    error!(
                                        "WRONG MESSAGE ID expected: {} received: {}",
                                        message_id, response.id
                                    );
                                    success = false;
                                    break;
                                } else {
                                    info!("MATCHED MESSAGE ID {}", message_id);
                                }
                            }
                            Sv1ActionResult::MatchMessageField {
                                message_type: _,
                                fields,
                            } => {
                                let msg = serde_json::to_value(response).unwrap();
                                check_sv1_fields(msg, fields);
                            }
                            _ => todo!(),
                        }
                    }
                    _ => error!("WRONG MESSAGE TYPE RECEIVED: expected Response"),
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
                    child.kill().await.expect("Failed to kill process");
                    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                }
            }
        }
        if !success {
            panic!("test failed!!!");
        }
    }
}

fn check_sv1_fields(msg: serde_json::Value, field_info: &Vec<(String, serde_json::Value)>) {
    for field in field_info {
        let msg = msg.as_object().unwrap();
        let value = msg
            .get(&field.0)
            .expect("match_message_field field name is not valid")
            .clone();
        assert!(
            field.1 == value,
            "match_message_field value is incorrect. Expected = {:?}, Recieved = {:?}",
            field.1,
            value
        )
    }
}
