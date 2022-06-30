use bitcoin_hashes::Error as BTCHashError;
use hex::FromHexError;
use std::convert::{TryFrom, TryInto};

pub mod client_to_server;
pub mod server_to_client;

use crate::json_rpc::{Response,Message};

/// Errors encountered during conversion between valid json_rpc messages and Sv1 messages.
///
#[derive(Debug)]
pub enum MethodError {
    /// If the json_rpc message call a method not defined by Sv1. It contains the called method
    MethodNotFound(String),
    /// If the json_rpc Response co"ntain an error in this case the error should just be reported
    ResponseIsAnError(Box<crate::json_rpc::Response>),
    /// Method can not be parsed
    ParsingMethodError((ParsingMethodError, Message)),
    // Method can not be serialized
    // SerializeError(Box<Method>),
    UnexpectedMethod(Method),
    // json_rpc message is not a request
    NotARequest,
}

//~:wimplimpl From<ParsingMethodError> for MethodError {
//~:wimpl    fn from(pars_err: ParsingMethodError) -> Self {
//~:wimpl        MethodError::ParsingMethodError(pars_err)
//~:wimpl    }
//~:wimpl}

impl From<FromHexError> for ParsingMethodError {
    fn from(hex_err: FromHexError) -> Self {
        ParsingMethodError::HexError(Box::new(hex_err))
    }
}

impl From<BTCHashError> for ParsingMethodError {
    fn from(btc_err: BTCHashError) -> Self {
        ParsingMethodError::BTCHashError(Box::new(btc_err))
    }
}


#[derive(Debug)]
pub enum ParsingMethodError {
    HexError(Box<FromHexError>),
    #[allow(clippy::upper_case_acronyms)]
    BTCHashError(Box<BTCHashError>),
    ValueNotAnArray(Box<serde_json::Value>),
    WrongArgs(Box<serde_json::Value>),
    ValueNotAString(Box<serde_json::Value>),
    ValueNotAFloat(Box<serde_json::Value>),
    ValueNotAnUnsigned(Box<serde_json::value::Number>),
    ValueNotAnInt(Box<serde_json::value::Number>),
    UnexpectedValue(Box<serde_json::Value>),
    ImpossibleToParseResultField(Box<Response>),
    ImpossibleToParseAsU64(Box<serde_json::Number>),
    UnexpectedArrayParams(Vec<serde_json::Value>),
    UnexpectedObjectParams(serde_json::Map<String,serde_json::Value>),
    MultipleError(Vec<ParsingMethodError>),
    Todo,
}

impl ParsingMethodError {
    pub fn as_method_error(self, msg: Message) -> MethodError {
        MethodError::ParsingMethodError((self,msg))
    }
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
    ErrorMessage(Message),
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
            Method::ErrorMessage(_) => Err(MethodError::UnexpectedMethod(method)),
            Method::Server2Client(_) => Err(MethodError::UnexpectedMethod(method)),
            Method::Server2ClientResponse(_) => Err(MethodError::UnexpectedMethod(method)),
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
            Method::ErrorMessage(_) => Err(MethodError::UnexpectedMethod(method)),
            Method::Client2Server(_) => Err(MethodError::UnexpectedMethod(method)),
            Method::Server2ClientResponse(_) => Err(MethodError::UnexpectedMethod(method)),
        }
    }
}


#[derive(Debug)]
pub enum Server2ClientResponse {
    Configure(server_to_client::Configure),
    Subscribe(server_to_client::Subscribe),
    GeneralResponse(server_to_client::GeneralResponse),
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
            Method::Client2Server(_) => Err(MethodError::UnexpectedMethod(method)),
            Method::Server2Client(_) => Err(MethodError::UnexpectedMethod(method)),
            Method::ErrorMessage(_) => Err(MethodError::UnexpectedMethod(method)),
        }
    }
}

impl TryFrom<Message> for Method {
    type Error = MethodError;

    fn try_from(msg: Message) -> Result<Self, MethodError> {
        match &msg {
            Message::StandardRequest(request) => match &request.method[..] {
                "mining.subscribe" => {
                    let method = request.clone().try_into().map_err(|e: ParsingMethodError| e.as_method_error(msg))?;
                    Ok(Method::Client2Server(Client2Server::Subscribe(method)))
                }
                "mining.authorize" => {
                    let method = request.clone().try_into().map_err(|e: ParsingMethodError| e.as_method_error(msg))?;
                    Ok(Method::Client2Server(Client2Server::Authorize(method)))
                }
                "mining.extranonce.subscribe" => Ok(Method::Client2Server(
                    Client2Server::ExtranonceSubscribe(client_to_server::ExtranonceSubscribe()),
                )),
                "mining.submit" => {
                    let method = request.clone().try_into().map_err(|e: ParsingMethodError| e.as_method_error(msg))?;
                    Ok(Method::Client2Server(Client2Server::Submit(method)))
                }
                "mining.configure" => {
                    let method = request.clone().try_into().map_err(|e: ParsingMethodError| e.as_method_error(msg))?;
                    Ok(Method::Client2Server(Client2Server::Configure(method)))
                }
                _ => Err(MethodError::MethodNotFound(request.clone().method)),
            },
            Message::Notification(notification) => match &notification.method[..] {
                "mining.notify" => {
                    let method = notification.clone().try_into().map_err(|e: ParsingMethodError| e.as_method_error(msg))?;
                    Ok(Method::Server2Client(Server2Client::Notify(method)))
                }
                "mining.set_version_mask" => {
                    let method = notification.clone().try_into().map_err(|e: ParsingMethodError| e.as_method_error(msg))?;
                    Ok(Method::Server2Client(Server2Client::SetVersionMask(method)))
                }
                "mining.set_difficulty" => {
                    let method = notification.clone().try_into().map_err(|e: ParsingMethodError| e.as_method_error(msg))?;
                    Ok(Method::Server2Client(Server2Client::SetDifficulty(method)))
                }
                "mining.set_extranonce" => {
                    let method = notification.clone().try_into().map_err(|e: ParsingMethodError| e.as_method_error(msg))?;
                    Ok(Method::Server2Client(Server2Client::SetExtranonce(method)))
                }
                _ => Err(MethodError::MethodNotFound(notification.clone().method)),
            },
            Message::OkResponse(response) => {
                response.clone().try_into().map(|result| Method::Server2ClientResponse(result)).map_err(|e| e.as_method_error(msg))
            }
            Message::ErrorResponse(_) => Ok(Method::ErrorMessage(msg)),
        }
    }
}

impl TryFrom<crate::json_rpc::Response> for Server2ClientResponse {
    type Error = ParsingMethodError;

    fn try_from(msg: Response) -> Result<Self, Self::Error> {
       let subscribe: Result<server_to_client::Subscribe, ParsingMethodError> = (&msg).try_into();
       let configure: Result<server_to_client::Configure, ParsingMethodError> = (&msg).try_into();
       let general_response: Result<server_to_client::GeneralResponse, ParsingMethodError> = (&msg).try_into();
       match (subscribe, configure, general_response) {
           (Ok(a), Err(_), Err(_)) => Ok(
               Server2ClientResponse::Subscribe(a),
           ),
           (Err(_), Ok(a), Err(_)) => Ok(
               Server2ClientResponse::Configure(a),
           ),
           (Err(_), Err(_), Ok(a)) => Ok(
               Server2ClientResponse::GeneralResponse(a),
           ),
           (Err(e), Err(ee), Err(eee)) => {
               Err(ParsingMethodError::MultipleError(vec![e,ee,eee]))
           }
           // Impossible state a message can not be more than one method
           _ => panic!()
       }
    }
}
