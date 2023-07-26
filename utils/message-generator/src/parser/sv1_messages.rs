use std::collections::HashMap;
use v1::json_rpc::*;
use serde::{Serialize, Deserialize};


#[derive(Debug, Clone, Serialize, Deserialize)]
struct Sv1Message {
    message: StandardRequest,
    id: String,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
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

    pub fn from_str(test: &str) -> Self {
        serde_json::from_str(test).unwrap()
    }
}

#[cfg(test)]
mod test{
    use serde_json::json;

    use super::*;
    #[test]
    fn it_parse_messages(){
        let data = r#"
            {
                "sv1_messages": [
                    {
                        "message": {
                            "id": 1,
                            "method": "mining.authorize",
                            "params": ["username", "password"]
                        },
                        "id": "authorize"
                    }
                ]
            }"#;
        let v: Sv1TestMessageParser = serde_json::from_str(data).unwrap();
        let m1 = &v.sv1_messages.unwrap()[0];
        let m2: Sv1Message = Sv1Message { 
            message: StandardRequest { 
                id: 1, 
                method: "mining.authorize".to_string(), 
                params: json!(["username", "password"]) }, 
            id: "authorize".to_string() 
        };
    
        assert_eq!(m1.message.params, m2.message.params);
        assert_eq!(m1.message.method, m2.message.method);
        assert_eq!(m1.message.id, m2.message.id);
        assert_eq!(m1.id, m2.id);
    }
}