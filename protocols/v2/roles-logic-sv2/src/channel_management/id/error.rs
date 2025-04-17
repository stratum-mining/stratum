#[derive(Debug)]
pub enum ChannelIdFactoryError {
    MessageSenderError,
    ResponseReceiverError,
    UnexpectedResponse,
}
