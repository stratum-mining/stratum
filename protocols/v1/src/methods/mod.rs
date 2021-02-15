use bitcoin_hashes::Error as BTCHashError;
use hex::FromHexError;
use std::convert::TryFrom;
use std::convert::TryInto;

pub mod client_to_server;
pub mod server_to_client;

use crate::json_rpc::Message;

/// Errors encountered during conversion between valid json_rpc messages and Sv1 messages.
///
#[derive(Debug)]
pub enum MethodError {
    /// If the json_rpc message call a method not defined by Sv1. It contains the called method
    MethodNotFound(String),
    /// If the json_rpc Response contain an error in this case the error should just be reported
    ResponseIsAnError(Box<crate::json_rpc::Response>),
    /// Method can not be parsed
    ParsingMethodError(ParsingMethodError),
    // Method can not be serialized
    // SerializeError(Box<Method>),
    UnexpectedMethod(Method),
    // json_rpc message is not a request
    NotARequest,
}

impl From<ParsingMethodError> for MethodError {
    fn from(pars_err: ParsingMethodError) -> Self {
        MethodError::ParsingMethodError(pars_err)
    }
}

impl From<FromHexError> for MethodError {
    fn from(hex_err: FromHexError) -> Self {
        MethodError::ParsingMethodError(ParsingMethodError::HexError(Box::new(hex_err)))
    }
}

impl From<BTCHashError> for MethodError {
    fn from(btc_err: BTCHashError) -> Self {
        MethodError::ParsingMethodError(ParsingMethodError::BTCHashError(Box::new(btc_err)))
    }
}

#[derive(Debug)]
pub enum ParsingMethodError {
    HexError(Box<FromHexError>),
    BTCHashError(Box<BTCHashError>),
    ValueNotAnArray(Box<serde_json::Value>),
    WrongArgs(Box<serde_json::Value>),
    ValueNotAString(Box<serde_json::Value>),
    ValueNotAFloat(Box<serde_json::Value>),
    ValueNotAnUnsigned(Box<serde_json::value::Number>),
    ValueNotAnInt(Box<serde_json::value::Number>),
    UnexpectedValue(Box<serde_json::Value>),
    Todo,
}

impl ParsingMethodError {
    pub fn not_array_from_value(v: serde_json::Value) -> Self {
        ParsingMethodError::ValueNotAnArray(Box::new(v))
    }

    pub fn not_string_from_value(v: serde_json::Value) -> Self {
        ParsingMethodError::ValueNotAString(Box::new(v))
    }

    pub fn not_float_from_value(v: serde_json::Value) -> Self {
        ParsingMethodError::ValueNotAFloat(Box::new(v))
    }

    pub fn not_unsigned_from_value(v: serde_json::value::Number) -> Self {
        ParsingMethodError::ValueNotAnUnsigned(Box::new(v))
    }

    pub fn not_int_from_value(v: serde_json::value::Number) -> Self {
        ParsingMethodError::ValueNotAnInt(Box::new(v))
    }

    pub fn wrong_args_from_value(v: serde_json::Value) -> Self {
        ParsingMethodError::WrongArgs(Box::new(v))
    }

    pub fn unexpected_value_from_value(v: serde_json::Value) -> Self {
        ParsingMethodError::UnexpectedValue(Box::new(v))
    }
}

#[derive(Debug)]
pub enum Method {
    Client2Server(Client2Server),
    Server2Client(Server2Client),
    Server2ClientResponse(Server2ClientResponse),
}

#[derive(Debug)]
pub enum Client2Server {
    Subscribe(client_to_server::Subscribe),
    Authorize(client_to_server::Authorize),
    ExtranonceSubscribe(client_to_server::ExtranonceSubscribe),
    Submit(client_to_server::Submit),
    Configure(client_to_server::Configure),
}

impl From<Client2Server> for Method {
    fn from(a: Client2Server) -> Self {
        Method::Client2Server(a)
    }
}

impl TryFrom<Message> for Client2Server {
    type Error = MethodError;

    fn try_from(msg: Message) -> Result<Self, Self::Error> {
        let method: Method = msg.try_into()?;
        match method {
            Method::Client2Server(client_to_server) => Ok(client_to_server),
            Method::Server2Client(a) => Err(MethodError::UnexpectedMethod(a.into())),
            Method::Server2ClientResponse(a) => Err(MethodError::UnexpectedMethod(a.into())),
        }
    }
}

#[derive(Debug)]
pub enum Server2Client {
    Notify(server_to_client::Notify),
    SetDifficulty(server_to_client::SetDifficulty),
    SetExtranonce(server_to_client::SetExtranonce),
    SetVersionMask(server_to_client::SetVersionMask),
}

impl From<Server2Client> for Method {
    fn from(a: Server2Client) -> Self {
        Method::Server2Client(a)
    }
}

impl TryFrom<Message> for Server2Client {
    type Error = MethodError;

    fn try_from(msg: Message) -> Result<Self, Self::Error> {
        let method: Method = msg.try_into()?;
        match method {
            Method::Server2Client(client_to_server) => Ok(client_to_server),
            Method::Client2Server(a) => Err(MethodError::UnexpectedMethod(a.into())),
            Method::Server2ClientResponse(a) => Err(MethodError::UnexpectedMethod(a.into())),
        }
    }
}

#[derive(Debug)]
pub enum Server2ClientResponse {
    Configure(server_to_client::Configure),
    Subscribe(server_to_client::Subscribe),
    Authorize(server_to_client::Authorize),
    Submit(server_to_client::Submit),
}

impl From<Server2ClientResponse> for Method {
    fn from(a: Server2ClientResponse) -> Self {
        Method::Server2ClientResponse(a)
    }
}

impl TryFrom<Message> for Server2ClientResponse {
    type Error = MethodError;

    fn try_from(msg: Message) -> Result<Self, Self::Error> {
        let method: Method = msg.try_into()?;
        match method {
            Method::Server2ClientResponse(server_to_client) => Ok(server_to_client),
            Method::Client2Server(a) => Err(MethodError::UnexpectedMethod(a.into())),
            Method::Server2Client(a) => Err(MethodError::UnexpectedMethod(a.into())),
        }
    }
}

impl TryFrom<Message> for Method {
    type Error = MethodError;

    fn try_from(msg: Message) -> Result<Self, Self::Error> {
        match msg {
            Message::StandardRequest(msg) => match &msg.method[..] {
                "mining.subscribe" => {
                    let method = msg.try_into()?;
                    Ok(Method::Client2Server(Client2Server::Subscribe(method)))
                }
                "mining.authorize" => {
                    let method = msg.try_into()?;
                    Ok(Method::Client2Server(Client2Server::Authorize(method)))
                }
                "mining.extranonce.subscribe" => Ok(Method::Client2Server(
                    Client2Server::ExtranonceSubscribe(client_to_server::ExtranonceSubscribe()),
                )),
                "mining.submit" => {
                    let method = msg.try_into()?;
                    Ok(Method::Client2Server(Client2Server::Submit(method)))
                }
                "mining.configure" => {
                    let method = msg.try_into()?;
                    Ok(Method::Client2Server(Client2Server::Configure(method)))
                }
                _ => Err(MethodError::MethodNotFound(msg.method)),
            },
            Message::Notification(msg) => match &msg.method[..] {
                "mining.notify" => {
                    let method = msg.try_into()?;
                    Ok(Method::Server2Client(Server2Client::Notify(method)))
                }
                "mining.set_version_mask" => {
                    let method = msg.try_into()?;
                    Ok(Method::Server2Client(Server2Client::SetVersionMask(method)))
                }
                "mining.set_difficulty" => {
                    let method = msg.try_into()?;
                    Ok(Method::Server2Client(Server2Client::SetDifficulty(method)))
                }
                "mining.set_extranonce" => {
                    let method = msg.try_into()?;
                    Ok(Method::Server2Client(Server2Client::SetExtranonce(method)))
                }
                _ => Err(MethodError::MethodNotFound(msg.method)),
            },
            Message::Response(msg) => {
                if msg.error.is_some() {
                    Err(MethodError::ResponseIsAnError(Box::new(msg)))
                } else {
                    let subscribe: Result<server_to_client::Subscribe, ()> = (&msg).try_into();
                    let configure: Result<server_to_client::Configure, ()> = (&msg).try_into();
                    match (subscribe, configure) {
                        (Ok(a), Err(_)) => Ok(Method::Server2ClientResponse(
                            Server2ClientResponse::Subscribe(a),
                        )),
                        (Err(_), Ok(a)) => Ok(Method::Server2ClientResponse(
                            Server2ClientResponse::Configure(a),
                        )),
                        (Ok(_), Ok(_)) => panic!(), // is safe to panic here
                        (Err(_), Err(_)) => {
                            Err(MethodError::ParsingMethodError(ParsingMethodError::Todo))
                        }
                    }
                }
            }
        }
        //res
    }
}
