#[cfg(feature = "prop_test")]
use quickcheck::{Arbitrary, Gen};

use alloc::string::String;
#[cfg(feature = "prop_test")]
use alloc::vec::Vec;

mod inner;
mod seq_inner;

trait IntoOwned {
    fn into_owned(self) -> Self;
}

pub use inner::Inner;
pub use seq_inner::{Seq0255, Seq064K, Sv2Option};

pub type U32AsRef<'a> = Inner<'a, true, 4, 0, 0>;
pub type U256<'a> = Inner<'a, true, 32, 0, 0>;
pub type ShortTxId<'a> = Inner<'a, true, 6, 0, 0>;
pub type PubKey<'a> = Inner<'a, true, 32, 0, 0>;
pub type Signature<'a> = Inner<'a, true, 64, 0, 0>;
pub type B032<'a> = Inner<'a, false, 1, 1, 32>;
pub type B0255<'a> = Inner<'a, false, 1, 1, 255>;
pub type Str0255<'a> = Inner<'a, false, 1, 1, 255>;
pub type B064K<'a> = Inner<'a, false, 1, 2, { u16::MAX as usize }>;
pub type B016M<'a> = Inner<'a, false, 1, 3, { 2_usize.pow(24) - 1 }>;

impl<'decoder> From<[u8; 32]> for U256<'decoder> {
    fn from(v: [u8; 32]) -> Self {
        Inner::Owned(v.into())
    }
}

#[cfg(feature = "prop_test")]
impl<'a> U256<'a> {
    pub fn from_gen(g: &mut Gen) -> Self {
        let mut inner = Vec::<u8>::arbitrary(g);
        inner.resize(32, 0);
        // 32 Bytes arrays are always converted into U256 unwrap never panic
        let inner: [u8; 32] = inner.try_into().unwrap();
        inner.into()
    }
}

#[cfg(feature = "prop_test")]
impl<'a> B016M<'a> {
    pub fn from_gen(g: &mut Gen) -> Self {
        // This can fail but is used only for tests purposes
        Vec::<u8>::arbitrary(g).try_into().unwrap()
    }
}

use core::convert::{TryFrom, TryInto};

impl<'a> TryFrom<String> for Str0255<'a> {
    type Error = crate::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.into_bytes().try_into()
    }
}

impl<'a> U32AsRef<'a> {
    pub fn as_u32(&self) -> u32 {
        let inner = self.inner_as_ref();
        u32::from_le_bytes([inner[0], inner[1], inner[2], inner[3]])
    }
}

impl<'a> From<u32> for U32AsRef<'a> {
    fn from(v: u32) -> Self {
        let bytes = v.to_le_bytes();
        let inner = vec![bytes[0], bytes[1], bytes[2], bytes[3]];
        U32AsRef::Owned(inner)
    }
}

impl<'a> From<&'a U32AsRef<'a>> for u32 {
    fn from(v: &'a U32AsRef<'a>) -> Self {
        let b = v.inner_as_ref();
        u32::from_le_bytes([b[0], b[1], b[2], b[3]])
    }
}
