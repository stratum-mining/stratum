use codec_sv2::{buffer_sv2::Slice, Frame as _Frame, Sv2Frame};
use roles_logic_sv2::parsers::AnyMessage;
use serde_json::{Map, Value};
use std::{collections::HashMap, convert::TryInto};

/// Stores any number or combination of a `PoolMessages` type (`CommonMessage`,
/// `JobNegotiationMessage`, `MiningMessage`, and/or `TemplateDistributionMessage`) as specified by
/// the `test.json` file serialized as a `Sv2Frame`, identified by the message identifier string.
pub struct Frames<'a> {
    /// Mapping of `PoolMessages` message identifier to the `PoolMessage` serialized in a
    /// `Sv2Frame`.
    pub frames: HashMap<String, Sv2Frame<AnyMessage<'a>, Slice>>,
}

impl<'a> Frames<'a> {
    /// Takes the `PoolMessages` stored in a hashmap from `Step1`, and serializes each message into
    /// a `Sv2Frame` then stores it in `Frames`.
    pub fn from_step_1<'b: 'a>(test: &'b str, messages: HashMap<String, AnyMessage<'a>>) -> Self {
        // Extract `"frame_builders"` from `test.json` contents
        let test: Map<String, Value> = serde_json::from_str(test).unwrap();
        let frames = test.get("frame_builders").unwrap().as_array().unwrap();

        // For each `PoolMessage`, locates it using the `"message_id"` and puts it into a
        // `Sv2Frame`. If `"automatic"` is specified, the message is put into a `Sv2Frame` in the
        // standard fashion. If `"manual"` is specified, it is expected that the `Sv2Frame` header
        // fields are specified in the `"frame_builders"` dict and a `Sv2Frame` is built using
        // those specified parameters. This is mostly typically done when the user wants to force a
        // frame error.
        let mut result = HashMap::new();
        for frame in frames {
            let id = frame
                .get("message_id")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string();
            let message = messages
                .get(&id)
                .unwrap_or_else(|| panic!("Missing messages message_id {}", id))
                .clone();
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
                        .replace("_", "")
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
        Frames { frames: result }
    }
}
