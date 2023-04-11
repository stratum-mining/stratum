use crate::{
    codec::{GetSize, SizeHint},
    Error,
};
mod non_copy_data_types;

mod copy_data_types;
use crate::codec::decodable::FieldMarker;
pub use copy_data_types::U24;
pub use non_copy_data_types::{
    Inner, PubKey, Seq0255, Seq064K, ShortTxId, Signature, Str0255, Sv2Option, U32AsRef, B016M,
    B0255, B032, B064K, U256,
};

#[cfg(not(feature = "no_std"))]
use std::io::{Error as E, Read, Write};

use std::convert::TryInto;

pub trait Sv2DataType<'a>: Sized + SizeHint + GetSize + TryInto<FieldMarker> {
    fn from_bytes_(data: &'a mut [u8]) -> Result<Self, Error> {
        Self::size_hint(data, 0)?;
        Ok(Self::from_bytes_unchecked(data))
    }

    fn from_bytes_unchecked(data: &'a mut [u8]) -> Self;

    fn from_vec_(data: Vec<u8>) -> Result<Self, Error>;

    fn from_vec_unchecked(data: Vec<u8>) -> Self;

    #[cfg(not(feature = "no_std"))]
    fn from_reader_(reader: &mut impl Read) -> Result<Self, Error>;

    fn to_slice(&'a self, dst: &mut [u8]) -> Result<usize, Error> {
        if dst.len() >= self.get_size() {
            self.to_slice_unchecked(dst);
            Ok(self.get_size())
        } else {
            Err(Error::WriteError(self.get_size(), dst.len()))
        }
    }

    fn to_slice_unchecked(&'a self, dst: &mut [u8]);

    #[cfg(not(feature = "no_std"))]
    fn to_writer_(&self, writer: &mut impl Write) -> Result<(), E>;
}
