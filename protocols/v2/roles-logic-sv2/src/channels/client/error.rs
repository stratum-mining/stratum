#[derive(Debug)]
pub enum ExtendedChannelError {
    NewExtranoncePrefixTooLarge,
    JobIdNotFound,
}

#[derive(Debug)]
pub enum StandardChannelError {
    JobIdNotFound,
    NewExtranoncePrefixTooLarge,
}

#[derive(Debug)]
pub enum GroupChannelError {
    JobIdNotFound,
}
