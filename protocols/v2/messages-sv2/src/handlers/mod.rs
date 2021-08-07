pub mod common;
pub mod mining;

pub enum SendTo_<T> {
    Upstream(T),
    Downstream(T),
    Relay,
    None,
}

impl<T> SendTo_<T> {
    pub fn into_inner(self) -> Option<T> {
        match self {
            Self::Upstream(t) => Some(t),
            Self::Downstream(t) => Some(t),
            Self::Relay => None,
            Self::None => None,
        }
    }
}
