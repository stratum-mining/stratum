use super::sv2_messages::{message_from_path, ReplaceField};
use codec_sv2::{buffer_sv2::Slice, Sv2Frame};
use roles_logic_sv2::parsers::AnyMessage;
use serde_json::{Map, Value};
use std::{collections::HashMap, convert::TryInto};

#[derive(Debug, Clone)]
pub struct Frames<'a> {
    pub frames: HashMap<String, Sv2Frame<AnyMessage<'a>, Slice>>,
}

impl<'a> Frames<'a> {
    pub fn from_step_1<'b: 'a>(
        test: &'b str,
        messages: HashMap<String, (AnyMessage<'a>, Vec<ReplaceField>)>,
    ) -> (Self, HashMap<String, (AnyMessage<'a>, Vec<ReplaceField>)>) {
        let test: Map<String, Value> = serde_json::from_str(test).unwrap();
        let frames = test.get("frame_builders").unwrap().as_array().unwrap();
        let mut messages = messages.clone();

        let mut result = HashMap::new();
        for frame in frames {
            let id: Vec<String> = frame
                .get("message_id")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string()
                .split("::")
                .map(|s| s.to_string())
                .collect();
            // If id consists of a single element, the it is the id of a message contained in the
            // present file, otherwise returs [path, id_], where the message appears in the file
            // indicated by path and appears under the label "id_"
            //the length of id is at most 2
            let (message, id) = match &id.len() {
                1 => (
                    messages
                        .get(&id[0])
                        .unwrap_or_else(|| panic!("Missing messages message_id {}", id[0]))
                        .0
                        .clone(),
                    id[0].clone(),
                ),
                2 => {
                    // the function "message_from_id" returns a an AnyMessage from the path in
                    // input
                    let mut path = id[0].clone();
                    std::string::String::insert_str(&mut path, 0, "../../../../");
                    let message = message_from_path(&[path, id[1].clone()]);
                    // TODO: if a message is taken from a module, should it be allowed to have a
                    // replace_fields? perhaps not. In this case, check that no replace_field
                    // appears in message
                    messages.insert(id[1].clone(), (message.clone(), vec![]));
                    (message, id[1].clone())
                }
                _ => panic!("The length if id vector should have length equal or less than 2"),
            };
            let type_ = frame.get("type").unwrap().as_str().unwrap();
            match type_ {
                "automatic" => {
                    let frame: Sv2Frame<AnyMessage<'a>, Slice> = message.try_into().unwrap();
                    result.insert(id, frame);
                }
                "manual" => {
                    let message_type = u8::from_str_radix(&frame.get("message_type").unwrap().as_str().unwrap()[2..], 16).expect("Frame  message_type should be an hex value starting with 0x and not bigger than 0xff");
                    let extension_type = frame
                        .get("extension_type")
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .replace('_', "")
                        .parse::<u16>()
                        .unwrap();
                    let channel_msg = frame.get("channel_msg").unwrap().as_bool().unwrap();
                    let frame =
                        Sv2Frame::from_message(message, message_type, extension_type, channel_msg)
                            .unwrap();
                    result.insert(id, frame);
                }
                _ => panic!("Unrecognized frames parsing type {}", type_),
            }
        }
        (Frames { frames: result }, messages)
    }
}
