use crate::{
    framing::{frame::Frame, noise_frame::NoiseFrame, sv2_frame::Sv2Frame},
    Error,
};
use binary_sv2::{GetSize, Serialize};
use core::convert::TryFrom;

pub type HandShakeFrame = NoiseFrame;

#[derive(Debug)]
pub enum EitherFrame<T, B> {
    HandShake(HandShakeFrame),
    Sv2(Sv2Frame<T, B>),
}

impl<T: Serialize + GetSize, B: AsMut<[u8]> + AsRef<[u8]>> EitherFrame<T, B> {
    pub fn encoded_length(&self) -> usize {
        match &self {
            Self::HandShake(frame) => frame.encoded_length(),
            Self::Sv2(frame) => frame.encoded_length(),
        }
    }
}

impl<T, B> TryFrom<EitherFrame<T, B>> for HandShakeFrame {
    type Error = Error;

    fn try_from(v: EitherFrame<T, B>) -> Result<Self, Error> {
        match v {
            EitherFrame::HandShake(frame) => Ok(frame),
            EitherFrame::Sv2(_) => Err(Error::ExpectedHandshakeFrame),
        }
    }
}

impl<T, B> From<HandShakeFrame> for EitherFrame<T, B> {
    fn from(v: HandShakeFrame) -> Self {
        Self::HandShake(v)
    }
}

impl<T, B> From<Sv2Frame<T, B>> for EitherFrame<T, B> {
    fn from(v: Sv2Frame<T, B>) -> Self {
        Self::Sv2(v)
    }
}
