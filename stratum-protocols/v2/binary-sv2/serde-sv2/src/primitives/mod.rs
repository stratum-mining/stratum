use alloc::{string::String, vec::Vec};
mod byte_arrays;
pub mod sequences;
mod short_tx_id;
mod signature;
mod u24;
mod u256;

pub use byte_arrays::{b016m::B016M, b0255::B0255, b032::B032, b064k::B064K, bytes::Bytes};
pub use sequences::{option::Sv2Option, seq0255::Seq0255, seq064k::Seq064K};

pub use short_tx_id::ShortTxId;
pub use signature::Signature;
pub use u24::U24;
pub use u256::U256;

pub type Bool = bool;
pub type U8 = u8;
pub type U16 = u16;
pub type U32 = u32;
pub type U64 = u64;
pub type Pubkey<'u> = U256<'u>;
// rust string are valid UTF-8 Sv2 string (STR0255) are raw bytes. So there are Sv2 string not
// representable as Str0255.
pub type Str0255<'a> = B0255<'a>;

pub trait GetSize {
    fn get_size(&self) -> usize;
}

pub trait FixedSize {
    const FIXED_SIZE: usize;
}

impl<T: FixedSize> GetSize for T {
    fn get_size(&self) -> usize {
        T::FIXED_SIZE
    }
}

impl FixedSize for bool {
    const FIXED_SIZE: usize = 1;
}

impl FixedSize for u8 {
    const FIXED_SIZE: usize = 1;
}

impl FixedSize for u16 {
    const FIXED_SIZE: usize = 2;
}

impl FixedSize for u32 {
    const FIXED_SIZE: usize = 4;
}

impl FixedSize for u64 {
    const FIXED_SIZE: usize = 8;
}

impl GetSize for [u8] {
    fn get_size(&self) -> usize {
        self.len()
    }
}

impl GetSize for String {
    fn get_size(&self) -> usize {
        // String is Str0255 1 byte len + x bytes
        self.len() + 1
    }
}

impl GetSize for Vec<u8> {
    fn get_size(&self) -> usize {
        self.len()
    }
}
