#![allow(clippy::result_unit_err)]
//! Startum V1 application protocol:
//!
//! json-rpc has two types of messages: **request** and **response**.
//! A request message can be either a **notification** or a **standard message**.
//! Standard messages expect a response, notifications do not. A typical example of a notification
//! is the broadcasting of a new block.
//!
//! Every RPC request contains three parts:
//! * message ID: integer or string
//! * remote method: unicode string
//! * parameters: list of parameters
//!
//! ## Standard requests
//! Message ID must be an unique identifier of request during current transport session. It may be
//! integer or some unique string, like UUID. ID must be unique only from one side (it means, both
//! server and clients can initiate request with id “1”). Client or server can choose string/UUID
//! identifier for example in the case when standard “atomic” counter isn’t available.
//!
//! ## Notifications
//! Notifications are like Request, but it does not expect any response and message ID is always
//! null:
//! * message ID: null
//! * remote method: unicode string
//! * parameters: list of parameters
//!
//! ## Responses
//! Every response contains the following parts
//! * message ID: same ID as in request, for pairing request-response together
//! * result: any json-encoded result object (number, string, list, array, …)
//! * error: null or list (error code, error message)
//!
//! References:
//! [https://docs.google.com/document/d/17zHy1SUlhgtCMbypO8cHgpWH73V5iUQKk_0rWvMqSNs/edit?hl=en_US#]
//! [https://braiins.com/stratum-v1/docs]
//! [https://en.bitcoin.it/wiki/Stratum_mining_protocol]
//! [https://en.bitcoin.it/wiki/BIP_0310]
//! [https://docs.google.com/spreadsheets/d/1z8a3S9gFkS8NGhBCxOMUDqs7h9SQltz8-VX3KPHk7Jw/edit#gid=0]

pub mod client;
pub mod error;
pub mod json_rpc;
pub mod methods;
pub mod server;
pub mod utils;
