use crate::{Action, ActionResult, Role, SaveField, Sv1Action, Sv1ActionResult, Sv2Type};
use codec_sv2::{buffer_sv2::Slice, StandardEitherFrame, Sv2Frame};
use roles_logic_sv2::parsers::AnyMessage;
use serde_json::{Map, Value};
use std::collections::HashMap;
use v1::json_rpc::StandardRequest;

use super::sv2_messages::ReplaceField;

pub struct Sv2ActionParser {}
pub struct Sv1ActionParser {}

impl Sv2ActionParser {
    pub fn from_step_2<'a, 'b: 'a>(
        test: &'b str,
        frames: HashMap<String, Sv2Frame<AnyMessage<'a>, Slice>>,
        //Action.messages: Vec<(EitherFrame<AnyMessage<'a>>,AnyMessage<'a>,Vec<(String,String)>)>
        messages: HashMap<String, (AnyMessage<'a>, Vec<ReplaceField>)>,
    ) -> Vec<Action<'a>> {
        let test: Map<String, Value> = serde_json::from_str(test).unwrap();
        let actions = test.get("actions").unwrap().as_array().unwrap();
        let mut result = vec![];
        for action in actions {
            let role = match action.get("role").unwrap().as_str().unwrap() {
                "client" => Role::Downstream,
                "server" => Role::Upstream,
                role => panic!("Unknown role: {}", role),
            };
            let mut action_frames = vec![];
            let ids = action.get("message_ids").unwrap().as_array().unwrap();
            for id in ids {
                let frame = frames
                    .get(id.as_str().unwrap())
                    .unwrap_or_else(|| {
                        panic!("Frame id not found: {} Impossible to parse action", id)
                    })
                    .clone();
                let frame = StandardEitherFrame::Sv2(frame);
                let message = messages.get(id.as_str().unwrap());
                let message = message
                    .unwrap_or_else(|| {
                        panic!("Message id not found: {} Impossible to parse action", id)
                    })
                    .clone();
                action_frames.push((frame, message.0, message.1));
            }

            let actiondoc = action.get("actiondoc").map(|t| t.to_string());
            let mut action_results = vec![];
            let results = action.get("results").unwrap().as_array().unwrap();
            for result in results {
                match result.get("type").unwrap().as_str().unwrap() {
                    "match_message_type" => {
                        let message_type = u8::from_str_radix(&result.get("value").unwrap().as_str().unwrap()[2..], 16).expect("Result message_type should be an hex value starting with 0x and not bigger than 0xff");
                        action_results.push(ActionResult::MatchMessageType(message_type));
                    }
                    "get_message_field" => {
                        let sv2_type = result.get("value").unwrap().clone();
                        let sv2_type: (String, String, Vec<SaveField>) =
                            serde_json::from_value(sv2_type)
                                .expect("match_message_field values not correct");
                        let get_message_field = ActionResult::GetMessageField {
                            subprotocol: sv2_type.0,
                            message_type: sv2_type.1,
                            fields: sv2_type.2,
                        };
                        action_results.push(get_message_field);
                    }
                    "match_message_field" => {
                        let sv2_type = result.get("value").unwrap().clone();
                        let sv2_type: (String, String, Vec<(String, Sv2Type)>) =
                            serde_json::from_value(sv2_type)
                                .expect("match_message_field values not correct");
                        action_results.push(ActionResult::MatchMessageField(sv2_type));
                    }
                    "match_message_len" => {
                        let message_len = result.get("value").unwrap().as_u64().unwrap() as usize;
                        action_results.push(ActionResult::MatchMessageLen(message_len));
                    }
                    "match_extension_type" => {
                        let extension_type = result
                            .get("extension_type")
                            .unwrap()
                            .as_str()
                            .unwrap()
                            .replace('_', "")
                            .parse::<u16>()
                            .unwrap();
                        action_results.push(ActionResult::MatchExtensionType(extension_type));
                    }
                    "close_connection" => {
                        action_results.push(ActionResult::CloseConnection);
                    }
                    "sustain_connection" => action_results.push(ActionResult::SustainConnection),
                    "none" => {
                        action_results.push(ActionResult::None);
                    }
                    type_ => panic!("Unknown result type {}", type_),
                }
            }

            let action = Action {
                messages: action_frames,
                result: action_results,
                role,
                actiondoc,
            };
            result.push(action);
        }
        result
    }
}

impl Sv1ActionParser {
    pub fn from_step_2(
        test: &'_ str,
        messages: HashMap<String, (StandardRequest, Vec<ReplaceField>)>,
    ) -> Vec<Sv1Action> {
        let test: Map<String, Value> = serde_json::from_str(test).unwrap();
        let actions = test.get("actions").unwrap().as_array().unwrap();
        let mut result = vec![];
        for action in actions {
            let mut action_requests = vec![];
            let ids = action.get("message_ids").unwrap().as_array().unwrap();
            for id in ids {
                let message = messages.get(id.as_str().unwrap());
                let message = message
                    .unwrap_or_else(|| {
                        panic!("Message id not found: {} Impossible to parse action", id)
                    })
                    .clone();
                action_requests.push(message);
            }
            let actiondoc = action.get("actiondoc").map(|t| t.to_string());
            let mut action_results = vec![];
            let results = action.get("results").unwrap().as_array().unwrap();
            for result in results {
                match result.get("type").unwrap().as_str().unwrap() {
                    "match_message_id" => {
                        let message_id = result.get("value").unwrap().as_u64().unwrap();
                        action_results.push(Sv1ActionResult::MatchMessageId(
                            serde_json::to_value(message_id).unwrap(),
                        ));
                    }
                    "match_message_field" => {
                        let sv1_value = result.get("value").unwrap().clone();
                        let sv1_value: (String, Vec<(String, Value)>) =
                            serde_json::from_value(sv1_value)
                                .expect("match_message_field values not correct");
                        action_results.push(Sv1ActionResult::MatchMessageField {
                            message_type: sv1_value.0,
                            fields: sv1_value.1,
                        });
                    }
                    "close_connection" => {
                        action_results.push(Sv1ActionResult::CloseConnection);
                    }
                    "none" => {
                        action_results.push(Sv1ActionResult::None);
                    }
                    type_ => panic!("Unknown result type {}", type_),
                }
            }

            let action = Sv1Action {
                messages: action_requests,
                result: action_results,
                actiondoc,
            };
            result.push(action);
        }
        result
    }
}
