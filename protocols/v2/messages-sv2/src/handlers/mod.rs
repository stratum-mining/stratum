pub mod common;
pub mod mining;

pub enum SendTo_<T> {
    Upstream(T),
    Downstream(T),
    /// ID of the node that must receive the relayed message
    /// that is not something defined in Sv2 protocol but is an helpful thing that can be used by
    /// the implementors of the Sv2 (sub)protocols
    /// TODO remove it is end up not beeing used
    Relay(Option<u32>),
    None,
}

impl<T> SendTo_<T> {
    pub fn into_inner(self) -> Option<T> {
        match self {
            Self::Upstream(t) => Some(t),
            Self::Downstream(t) => Some(t),
            Self::Relay(_) => None,
            Self::None => None,
        }
    }
}

/// Associate a channle id with a remote id where remote id is whathever the implementation use
/// to identify Downstreams or/and Upstreams
///
/// # Downstream
/// 1 -> opens channel req id (req id == upstream id) using on_open_standard_channel_request
/// 4 <- open channel success (channel id == upstream id) using on_open_standard_channel_success
///
/// # Upstream
/// 2 <- opens channel req id (req id == downstream id) using on_open_standard_channel_request
/// 3 -> open channel success (channel id == downstream id) using on_open_standard_channel_success
///
/// # Proxy
/// will use two RemoteSelectro in order to register both Downstram and Upstream remotes
pub trait RemoteSelector<T> {
    fn on_open_standard_channel_request(&mut self, request_id: u32, remote_id: T);

    fn on_open_standard_channel_success(&mut self, request_id: u32, channel_id: u32);

    fn get_remote_id(&self, channel_id: u32) -> Option<u32>;
}

pub trait DownstreamSelector<T> {
    fn on_request(&mut self, request_id: u32, downstream: T);

    fn get_dowstream(&mut self, dowstream_id: u32) -> T;
}
