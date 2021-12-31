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
use std::collections::HashMap;
use std::sync::{Arc, Mutex as Mutex_, MutexGuard, PoisonError};
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

    fn new() -> Self;

    fn new_as_mutex() -> Arc<Mutex<Self>>
    where
        Self: Sized,
    {
        Arc::new(Mutex::new(Self::new()))
    }
}

/// Proxyies likely need to change the request ids of downsteam's messages. They also need to
/// remeber original id to patch the upstream's response with it
#[derive(Debug)]
pub struct RequestIdMapper {
    // upstream id -> downstream id
    request_ids_map: HashMap<u32, u32>,
    next_id: u32,
}

impl RequestIdMapper {
    pub fn new_as_mutex() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            request_ids_map: HashMap::new(),
            next_id: 0,
        }))
    }

    pub fn on_open_channel(&mut self, id: u32) -> u32 {
        let new_id = self.next_id;
        self.next_id += 1;

        //let mut inner = self.request_ids_map.lock().unwrap();
        self.request_ids_map.insert(new_id, id);
        new_id
    }

    pub fn remove(&mut self, upstream_id: u32) -> u32 {
        //let mut inner = self.request_ids_map.lock().unwrap();
        self.request_ids_map.remove(&upstream_id).unwrap()
    }
}

#[derive(Debug)]
pub struct Mutex<T>(Mutex_<T>);

impl<T> Mutex<T> {
    pub fn safe_lock<F, Ret>(&self, thunk: F) -> Result<Ret, PoisonError<MutexGuard<'_, T>>>
    where
        F: FnOnce(&mut T) -> Ret,
    {
        let mut lock = self.0.lock()?;
        let return_value = thunk(&mut *lock);
        drop(lock);
        Ok(return_value)
    }

    pub fn new(v: T) -> Self {
        Mutex(Mutex_::new(v))
    }
}
//pub trait RemoteSelector<Downstream, Up: IsUpstream> {
//
//    /// This get the connection-wide updated request_id
//    fn on_open_standard_channel_request(
//        &mut self,
//        request_id: u32,
//        downstream: Downstream,
//        //upstream: Upstream,
//    );
//
//    fn on_open_standard_channel_success(&mut self, request_id: u32, channel_id: u32) -> Downstream;
//
//    fn get_downstreams_in_channel(&self, channel_id: u32) -> Vec<Downstream>;
//
//    fn get_upstreams_in_channel(&self, channel_id: u32) -> Vec<Arc<Mutex<Up>>>;
//
//    fn remote_from_request_id(&mut self, request_id: u32) -> Downstream;
//
//    fn new(upstreams: Vec<Up>) -> Self;
//
//    fn new_as_mutex(upstreams: Vec<Up>) -> Arc<Mutex<Self>>
//    where
//        Self: Sized,
//    {
//        Arc::new(Mutex::new(Self::new(upstreams)))
//    }
//
//    fn on_setup_connection(&self, pair_settings: PairSettings) -> Arc<Mutex<Up>>;
//}
//#[derive(Debug, Copy, Clone)]
//pub struct PairSettings {
//    pub protocol: crate::handlers::common::Protocol,
//    pub min_v: u16,
//    pub max_v: u16,
//    pub flags: u32,
//}
//
//pub struct UpstreamData<T> {
//    upstream: T,
//    version: u16,
//    flags: u32,
//    supported_protocols: Vec<crate::handlers::common::Protocol>,
//}
//
//pub trait IsUpstream {
//    fn get_version(&self) -> u16;
//    fn get_flags(&self) -> u32;
//    fn get_supported_protocols(&self) -> Vec<crate::handlers::common::Protocol>;
//}
