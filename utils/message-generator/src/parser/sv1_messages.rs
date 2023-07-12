use v1::server_to_client::{Notify, SetDifficulty, SetExtranonce, Submit, Subscribe, Authorize};
use v1::client_to_server::{Subscribe, Authorize, Submit};
use v1::methods::*;


#[derive(Debug, Clone, Serialize, Deserialize)]
struct Sv1ClientMessage<'a> {
    #[serde(borrow)]
    message: Sv1ClientMessages<'a>,
    id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Sv1ServerMessage<'a> {
    #[serde(borrow)]
    message: Sv1ServerMessages<'a>,
    id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Sv1ServerResponse<'a> {
    #[serde(borrow)]
    message: Sv1ServerResponses<'a>,
    id: String,
}

//CLIENT TO SERVER
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Sv1ClientMessages<'a> {
    #[serde(borrow)]
    Subscribe(client_to_server::Subscribe<'a>),
    Authorize(client_to_server::Authorize),
    #[serde(borrow)]
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
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Sv1ServerMessages<'a> {
    Notify(server_to_client::Notify),
    SetDifficulty(server_to_client::SetDifficulty),
    #[serde(borrow)]
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
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
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