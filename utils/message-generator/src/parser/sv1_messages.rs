use std::collections::HashMap;
use v1::methods::*;
use serde::{Serialize, Deserialize};


//#[derive(Debug, Clone, Serialize, Deserialize)]
struct Sv1ClientMessage<'a> {
    message: Sv1ClientMessages<'a>,
    id: String,
}

//#[derive(Debug, Clone, Serialize, Deserialize)]
struct Sv1ServerMessage<'a> {
    message: Sv1ServerMessages<'a>,
    id: String,
}

//#[derive(Debug, Clone, Serialize, Deserialize)]
struct Sv1ServerResponse<'a> {
    message: Sv1ServerResponses<'a>,
    id: String,
}

//CLIENT TO SERVER
//#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Sv1ClientMessages<'a> {
    Subscribe(client_to_server::Subscribe<'a>),
    Authorize(client_to_server::Authorize),
    Submit(client_to_server::Submit<'a>),
}

impl<'a> From<Sv1ClientMessages<'a>> for Client2Server<'a>{
    fn from(v: Sv1ClientMessages<'a>) -> Self {
        match v {
            Sv1ClientMessages::Subscribe(m) => Self::Subscribe(m),
            Sv1ClientMessages::Authorize(m) => Self::Authorize(m),
            Sv1ClientMessages::Submit(m) => Self::Submit(m)
        }
    }
}


//SERVER TO CLIENT
//#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Sv1ServerMessages<'a> {
    Notify(server_to_client::Notify<'a>),
    SetDifficulty(server_to_client::SetDifficulty),
    SetExtranonce(server_to_client::SetExtranonce<'a>),
}

impl<'a> From<Sv1ServerMessages<'a>> for Server2Client<'a> {
    fn from(v: Sv1ServerMessages<'a>) -> Self {
        match v {
            Sv1ServerMessages::Notify(m) => Self::Notify(m),
            Sv1ServerMessages::SetDifficulty(m) => Self::SetDifficulty(m),
            Sv1ServerMessages::SetExtranonce(m) => Self::SetExtranonce(m)
        }
    }
}


//SERVER TO CLIENT RESPONSES
//#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Sv1ServerResponses<'a> {
    Subscribe(server_to_client::Subscribe<'a>),
    Authorize(server_to_client::Authorize),
    Submit(server_to_client::Submit)
}

impl<'a> From<Sv1ServerResponses<'a>> for Server2ClientResponse<'a> {
    fn from(v: Sv1ServerResponses<'a>) -> Self {
        match v {
            Sv1ServerResponses::Subscribe(m) => Self::Subscribe(m),
            Sv1ServerResponses::Authorize(m) => Self::Authorize(m),
            Sv1ServerResponses::Submit(m) => Self::Submit(m)
        }
    }
}


//#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sv1TestMessageParser<'a>{
    sv1_client_messages: Option<Vec<Sv1ClientMessage<'a>>>,
    sv1_server_messages: Option<Vec<Sv1ServerMessage<'a>>>,
    sv1_server_responses: Option<Vec<Sv1ServerResponse<'a>>>
}

impl<'a> Sv1TestMessageParser<'a>{
    pub fn into_map(self) -> HashMap<String, Method<'a>> {
        let mut map = HashMap::new();
        if let Some(sv1_client_messages) = self.sv1_client_messages{
            for message in sv1_client_messages{
                let id = message.id;
                let message = message.message.into();
                let message = Method::Client2Server(message);
                map.insert(id, message);
            }
        };
        if let Some(sv1_server_messages) = self.sv1_server_messages{
            for message in sv1_server_messages{
                let id = message.id;
                let message = message.message.into();
                let message = Method::Server2Client(message);
                map.insert(id, message);
            }
        };
        if let Some(sv1_server_responses) = self.sv1_server_responses{
            for message in sv1_server_responses{
                let id = message.id;
                let message = message.message.into();
                let message = Method::Server2ClientResponse(message);
                map.insert(id, message);
            }
        };
        map
    }

    // pub fn from_str<'b: 'a>(test: &'b str) -> Self {
    //     serde_json::from_str(test).unwrap()
    // }
}