mod byte_arrays;
mod sequences;
mod signature;
mod u24;
mod u256;

pub use byte_arrays::b016m::B016M;
pub use byte_arrays::b0255::B0255;
pub use byte_arrays::b064k::B064K;
pub use sequences::seq0255::Seq0255;
pub use sequences::seq064k::Seq064K;

pub use signature::Signature;
pub use u24::U24;
pub use u256::U256;

pub type Bool = bool;
pub type U8 = u8;
pub type U16 = u16;
pub type U32 = u32;
pub type Bytes = [u8]; // TODO test if both serialize and deserialize workd for Bytes
pub type Pubkey<'u> = U256<'u>;
// TODO rust string are valid UTF-8 Sv2 string (STR0255) are raw bytes. So there are Sv2 string not
// representable as Str0255. I suggest to define Sv2 STR0255 as 1 byte len + a valid UTF-8 string.
pub type Str0255 = String;
