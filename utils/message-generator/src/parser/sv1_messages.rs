use std::collections::HashMap;
use v1::json_rpc::*;
use serde::{Serialize, Deserialize};


//#[derive(Debug, Clone, Serialize, Deserialize)]
struct Sv1Message {
    message: StandardRequest,
    id: String,
}


//#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sv1TestMessageParser{
    sv1_messages: Option<Vec<Sv1Message>>,
}

impl Sv1TestMessageParser{
    pub fn into_map(self) -> HashMap<String, StandardRequest> {
        let mut map = HashMap::new();
        if let Some(sv1_messages) = self.sv1_messages{
            for message in sv1_messages{
                let id = message.id;
                let message = message.message;
                map.insert(id, message);
            }
        };
        map
    }

    // pub fn from_str<'b: 'a>(test: &'b str) -> Self {
    //     serde_json::from_str(test).unwrap()
    // }
}