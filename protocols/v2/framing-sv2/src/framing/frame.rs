use crate::Error;

use binary_sv2::{GetSize, Serialize};

pub trait Frame<'a, T: Serialize + GetSize>: Sized {
    type Buffer: AsMut<[u8]>;
    type Deserialized;

    /// Serialize the frame into dst if the frame is already serialized it just swap dst with
    /// itself
    fn serialize(self, dst: &mut [u8]) -> Result<(), Error>;

    //fn deserialize(&'a mut self) -> Result<Self::Deserialized, serde_sv2::Error>;

    fn payload(&'a mut self) -> &'a mut [u8];

    /// If is an Sv2 frame return the Some(header) if it is a noise frame return None
    fn get_header(&self) -> Option<crate::header::Header>;

    /// Try to build an Frame frame from raw bytes.
    /// It return the frame or the number of the bytes needed to complete the frame
    /// The resulting frame is just a header plus a payload with the right number of bytes nothing
    /// is said about the correctness of the payload
    fn from_bytes(bytes: Self::Buffer) -> Result<Self, isize>;

    fn from_bytes_unchecked(bytes: Self::Buffer) -> Self;

    fn size_hint(bytes: &[u8]) -> isize;

    fn encoded_length(&self) -> usize;

    /// Try to build an Frame frame from a serializable payload.
    /// It return a Frame if the size of the payload fit in the frame, if not it return None
    fn from_message(
        message: T,
        message_type: u8,
        extension_type: u16,
        channel_msg: bool,
    ) -> Option<Self>;
}
