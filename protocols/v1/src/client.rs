// use error::Result;
use crate::{
    error::Error,
    json_rpc::{self, Message},
    methods::{self, client_to_server, server_to_client, Method, MethodError},
    utils::{Extranonce, HexU32Be},
};
use std::convert::{TryFrom, TryInto};
use tracing::debug;

pub trait IsClient<'a> {
    /// Deserialize a [raw json_rpc message][a] into a [stratum v1 message][b] and handle the
    /// result.
    ///
    /// [a]: crate::...
    /// [b]:
    fn handle_message(
        &mut self,
        msg: json_rpc::Message,
    ) -> Result<Option<json_rpc::Message>, Error<'a>>
    where
        Self: std::marker::Sized,
    {
        let method: Result<Method<'a>, MethodError<'a>> = msg.try_into();

        match method {
            Ok(m) => match m {
                Method::Server2ClientResponse(response) => {
                    let response = self.update_response(response)?;
                    self.handle_response(response)
                }
                Method::Server2Client(request) => self.handle_request(request),
                Method::Client2Server(_) => Err(Error::InvalidReceiver(m)),
                Method::ErrorMessage(msg) => self.handle_error_message(msg),
            },
            Err(e) => Err(e.into()),
        }
    }

    fn update_response(
        &mut self,
        response: methods::Server2ClientResponse<'a>,
    ) -> Result<methods::Server2ClientResponse<'a>, Error<'a>> {
        match &response {
            methods::Server2ClientResponse::GeneralResponse(general) => {
                let is_authorize = self.id_is_authorize(&general.id);
                let is_submit = self.id_is_submit(&general.id);
                match (is_authorize, is_submit) {
                    (Some(prev_name), false) => {
                        let authorize = general.clone().into_authorize(prev_name);
                        Ok(methods::Server2ClientResponse::Authorize(authorize))
                    }
                    (None, false) => Ok(methods::Server2ClientResponse::Submit(
                        general.clone().into_submit(),
                    )),
                    _ => Err(Error::UnknownID(general.id)),
                }
            }
            _ => Ok(response),
        }
    }

    /// Call the right handler according with the called method
    fn handle_request(
        &mut self,
        request: methods::Server2Client<'a>,
    ) -> Result<Option<json_rpc::Message>, Error<'a>>
    where
        Self: std::marker::Sized,
    {
        match request {
            methods::Server2Client::Notify(notify) => {
                self.handle_notify(notify)?;
                Ok(None)
            }
            methods::Server2Client::SetDifficulty(mut set_diff) => {
                self.handle_set_difficulty(&mut set_diff)?;
                Ok(None)
            }
            methods::Server2Client::SetExtranonce(mut set_extra_nonce) => {
                self.handle_set_extranonce(&mut set_extra_nonce)?;
                Ok(None)
            }
            methods::Server2Client::SetVersionMask(mut set_version_mask) => {
                self.handle_set_version_mask(&mut set_version_mask)?;
                Ok(None)
            }
        }
    }

    fn handle_response(
        &mut self,
        response: methods::Server2ClientResponse<'a>,
    ) -> Result<Option<json_rpc::Message>, Error<'a>>
    where
        Self: std::marker::Sized,
    {
        match response {
            methods::Server2ClientResponse::Configure(mut configure) => {
                self.handle_configure(&mut configure)?;
                self.set_version_rolling_mask(configure.version_rolling_mask());
                self.set_version_rolling_min_bit(configure.version_rolling_min_bit());
                self.set_status(ClientStatus::Configured);

                //in sv1 the mining.configure message should be the first message to come in before
                // the subscribe - the subscribe response is where the server hands out the
                // extranonce so it doesnt really matter what the server sets the
                // extranonce to in the mining.configure handler
                debug!("NOTICE: Subscribe extranonce is hardcoded by server");
                let subscribe = self
                    .subscribe(
                        configure.id,
                        Some(Extranonce::try_from(hex::decode("08000002")?)?),
                    )
                    .ok();
                Ok(subscribe)
            }
            methods::Server2ClientResponse::Subscribe(subscribe) => {
                self.handle_subscribe(&subscribe)?;
                self.set_extranonce1(subscribe.extra_nonce1);
                self.set_extranonce2_size(subscribe.extra_nonce2_size);
                self.set_status(ClientStatus::Subscribed);
                Ok(None)
            }
            methods::Server2ClientResponse::Authorize(authorize) => {
                if authorize.is_ok() {
                    self.authorize_user_name(authorize.user_name());
                };
                Ok(None)
            }
            methods::Server2ClientResponse::Submit(_) => Ok(None),
            // impossible state
            methods::Server2ClientResponse::GeneralResponse(_) => panic!(),
            methods::Server2ClientResponse::SetDifficulty(_) => Ok(None),
        }
    }

    fn handle_error_message(
        &mut self,
        message: Message,
    ) -> Result<Option<json_rpc::Message>, Error<'a>>;

    /// Check if the client sent an Authorize request with the given id, if so it return the
    /// authorized name
    fn id_is_authorize(&mut self, id: &u64) -> Option<String>;

    /// Check if the client sent a Submit request with the given id
    fn id_is_submit(&mut self, id: &u64) -> bool;

    fn handle_notify(&mut self, notify: server_to_client::Notify<'a>) -> Result<(), Error<'a>>;

    fn handle_configure(&mut self, conf: &mut server_to_client::Configure)
        -> Result<(), Error<'a>>;

    fn handle_set_difficulty(
        &mut self,
        m: &mut server_to_client::SetDifficulty,
    ) -> Result<(), Error<'a>>;

    fn handle_set_extranonce(
        &mut self,
        m: &mut server_to_client::SetExtranonce,
    ) -> Result<(), Error<'a>>;

    fn handle_set_version_mask(
        &mut self,
        m: &mut server_to_client::SetVersionMask,
    ) -> Result<(), Error<'a>>;

    fn handle_subscribe(
        &mut self,
        subscribe: &server_to_client::Subscribe<'a>,
    ) -> Result<(), Error<'a>>;

    fn set_extranonce1(&mut self, extranonce1: Extranonce<'a>);

    fn extranonce1(&self) -> Extranonce<'a>;

    fn set_extranonce2_size(&mut self, extra_nonce2_size: usize);

    fn extranonce2_size(&self) -> usize;

    fn version_rolling_mask(&self) -> Option<HexU32Be>;

    fn set_version_rolling_mask(&mut self, mask: Option<HexU32Be>);

    fn set_version_rolling_min_bit(&mut self, min: Option<HexU32Be>);

    fn version_rolling_min_bit(&mut self) -> Option<HexU32Be>;

    fn set_status(&mut self, status: ClientStatus);

    fn signature(&self) -> String;

    fn status(&self) -> ClientStatus;

    fn last_notify(&self) -> Option<server_to_client::Notify>;

    /// Check if the given user_name has been authorized by the server
    #[allow(clippy::ptr_arg)]
    fn is_authorized(&self, name: &String) -> bool;

    /// Register the given user_name has authorized by the server
    fn authorize_user_name(&mut self, name: String);

    fn configure(&mut self, id: u64) -> json_rpc::Message {
        if self.version_rolling_min_bit().is_none() && self.version_rolling_mask().is_none() {
            client_to_server::Configure::void(id).into()
        } else {
            client_to_server::Configure::new(
                id,
                self.version_rolling_mask(),
                self.version_rolling_min_bit(),
            )
            .into()
        }
    }

    fn subscribe(
        &mut self,
        id: u64,
        extranonce1: Option<Extranonce<'a>>,
    ) -> Result<json_rpc::Message, Error<'a>> {
        match self.status() {
            ClientStatus::Init => Err(Error::IncorrectClientStatus("mining.subscribe".to_string())),
            _ => Ok(client_to_server::Subscribe {
                id,
                agent_signature: self.signature(),
                extranonce1,
            }
            .try_into()?),
        }
    }

    fn authorize(
        &mut self,
        id: u64,
        name: String,
        password: String,
    ) -> Result<json_rpc::Message, Error> {
        match self.status() {
            ClientStatus::Init => Err(Error::IncorrectClientStatus("mining.authorize".to_string())),
            _ => Ok(client_to_server::Authorize { id, name, password }.into()),
        }
    }

    fn submit(
        &mut self,
        id: u64,
        user_name: String,
        extra_nonce2: Extranonce<'a>,
        time: i64,
        nonce: i64,
        version_bits: Option<HexU32Be>,
    ) -> Result<json_rpc::Message, Error<'a>> {
        match self.status() {
            ClientStatus::Init => Err(Error::IncorrectClientStatus("mining.submit".to_string())),
            _ => {
                if let Some(notify) = self.last_notify() {
                    if !self.is_authorized(&user_name) {
                        return Err(Error::UnauthorizedClient(user_name));
                    }
                    Ok(client_to_server::Submit {
                        job_id: notify.job_id,
                        user_name,
                        extra_nonce2,
                        time: HexU32Be(time as u32),
                        nonce: HexU32Be(nonce as u32),
                        version_bits,
                        id,
                    }
                    .into())
                } else {
                    Err(Error::IncorrectClientStatus(
                        "No Notify instance found".to_string(),
                    ))
                }
            }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ClientStatus {
    Init,
    Configured,
    Subscribed,
}
