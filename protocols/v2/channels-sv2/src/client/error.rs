use crate::bip141::StripBip141Error;

#[derive(Debug)]
pub enum ExtendedChannelError {
    NewExtranoncePrefixTooLarge,
    JobIdNotFound,
    FailedToTryToStripBip141(StripBip141Error),
    FailedToSerializeToB064K,
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
