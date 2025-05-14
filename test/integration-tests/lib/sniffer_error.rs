#[derive(Debug, PartialEq)]
pub enum SnifferError {
    DownstreamClosed,
    UpstreamClosed,
}
