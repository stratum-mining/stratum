//! RequestIdMapper when present map the downstream's request_id with a newly and connection-wide
//! unique upstream's request_id
//!
//! RemoteSelector associate a channel_id and/or a request_id to a remote a remote is whatever
//! type the implementation is using to rapresents remote nodes.
//!
//! RequestIdMapper is used by proxies to change the request_id field in the message in order to:
//! 1. have connection-wide unique ids with upstream
//! 2. map the connection-wide unique id from upstream to the originale request id.
//!
//! RemoteSelector is used by proxies and TODO in order to know where messages should be realyed.
//!
//! Both RemotoSelector and RequestIdMapper in proxies are created for every upstream connection.
//! There is an 1 to 1 relation upstream connection <-> (RemotoSelector, RequestIdMapper)
//!
//! TODO
//! right now, following the above convection and using RequestIdMapper and RemotoSelector, the
//! scenario where a proxy split a downstream connection in two upstream connection is not
//! supported
pub mod common;
pub mod mining;

/// SubProtocol is the Sv2 (sub)protocol that the implementor is implementing (eg: mining, common,
/// ...)
/// Remote is wathever type the implementor use to represent remote connection
pub enum SendTo_<SubProtocol, Remote> {
    Upstream(SubProtocol),
    Downstream(SubProtocol),
    Relay(Vec<Remote>),
    None,
}

impl<SubProtocol, Remote> SendTo_<SubProtocol, Remote> {
    pub fn into_message(self) -> Option<SubProtocol> {
        match self {
            Self::Upstream(t) => Some(t),
            Self::Downstream(t) => Some(t),
            Self::Relay(_) => None,
            Self::None => None,
        }
    }
    pub fn into_remote(self) -> Option<Vec<Remote>> {
        match self {
            Self::Upstream(_) => None,
            Self::Downstream(_) => None,
            Self::Relay(t) => Some(t),
            Self::None => None,
        }
    }
}

/// Associate a channle id with a remote where remote is whathever type the implementation use
/// to represent Downstreams or/and Upstreams
pub trait RemoteSelector<T> {
    /// This get the connection-wide updated request_id
    fn on_open_standard_channel_request(&mut self, request_id: u32, remote: T);

    fn on_open_standard_channel_success(&mut self, request_id: u32, channel_id: u32) -> T;

    fn get_remotes_in_channel(&self, channel_id: u32) -> Vec<T>;

    fn remote_from_request_id(&mut self, request_id: u32) -> T;
}
