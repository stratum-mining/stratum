use std::collections::HashMap;
use v1::json_rpc::*;
use serde::{Serialize, Deserialize};

use super::sv2_messages::ReplaceField;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sv1Message {
    message: StandardRequest,
    id: String,
    replace_fields: Option<Vec<ReplaceField>>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sv1TestMessageParser{
    sv1_messages: Option<Vec<Sv1Message>>,
}

impl Sv1TestMessageParser {
    pub fn into_map(self) -> HashMap<String, (StandardRequest, Vec<ReplaceField>)> {
        let mut map = HashMap::new();
        if let Some(sv1_messages) = self.sv1_messages{
            for message in sv1_messages{
                let id = message.id;
                let replace_fields = match message.replace_fields {
                    Some(replace_fields) => replace_fields,
                    None => vec![],
                };
                let message = message.message;
                map.insert(id, (message, replace_fields));
            }
        };
        map
    }

    pub fn from_str(test: &str) -> Self {
        serde_json::from_str(test).unwrap()
    }
}