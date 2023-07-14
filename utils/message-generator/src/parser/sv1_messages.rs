use v1::methods::*;


struct Sv1ClientMessage<'a> {
    message: Sv1ClientMessages<'a>,
    id: String,
}

struct Sv1ServerMessage<'a> {
    message: Sv1ServerMessages<'a>,
    id: String,
}

struct Sv1ServerResponse<'a> {
    message: Sv1ServerResponses<'a>,
    id: String,
}

//CLIENT TO SERVER
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