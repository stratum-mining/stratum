use crate::error::Error;
use bitcoin_hashes::hex::{FromHex, ToHex};
use byteorder::{BigEndian, ByteOrder, LittleEndian, WriteBytesExt};
use serde_json::Value;
use std::{convert::TryFrom, mem::size_of};

/// Helper type that allows simple serialization and deserialization of byte vectors
/// that are represented as hex strings in JSON.
/// HexBytes must be less than or equal to 32 bytes.
#[derive(Clone, Debug, PartialEq)]
pub struct HexBytes(Vec<u8>);

impl HexBytes {
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<Vec<u8>> for HexBytes {
    fn from(value: Vec<u8>) -> Self {
        HexBytes(value)
    }
}

impl From<HexBytes> for Vec<u8> {
    fn from(v: HexBytes) -> Self {
        v.0
    }
}

impl From<HexBytes> for Value {
    fn from(eb: HexBytes) -> Self {
        Into::<String>::into(eb).into()
    }
}

/// Referencing the internal part of hex bytes
impl AsRef<Vec<u8>> for HexBytes {
    fn as_ref(&self) -> &Vec<u8> {
        &self.0
    }
}

/// fix for error on odd-length hex sequences
/// FIXME: find a nicer solution
fn hex_decode(s: &str) -> Result<Vec<u8>, Error> {
    if s.len() % 2 != 0 {
        Ok(hex::decode(&format!("0{}", s))?)
    } else {
        Ok(hex::decode(s)?)
    }
}

impl TryFrom<&str> for HexBytes {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self, Error> {
        Ok(HexBytes(hex_decode(value)?))
    }
}

impl From<HexBytes> for String {
    fn from(bytes: HexBytes) -> String {
        hex::encode(bytes.0)
    }
}

/// Big-endian alternative of the HexU32
#[derive(Clone, Debug, PartialEq)]
pub struct HexU32Be(pub u32);

impl HexU32Be {
    pub fn check_mask(&self, mask: &HexU32Be) -> bool {
        ((!self.0) & mask.0) == 0
    }
}

impl From<HexU32Be> for Value {
    fn from(eu: HexU32Be) -> Self {
        Into::<String>::into(eu).into()
    }
}

impl TryFrom<&str> for HexU32Be {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self, Error> {
        let parsed_bytes: [u8; 4] = FromHex::from_hex(value)?;
        Ok(HexU32Be(u32::from_be_bytes(parsed_bytes)))
    }
}

/// Helper Serializer
impl From<HexU32Be> for String {
    fn from(v: HexU32Be) -> Self {
        v.0.to_be_bytes().to_hex()
    }
}

/// PrevHash in Stratum V1 has brain-damaged serialization as it swaps bytes of every u32 word
/// into big endian. Therefore, we need a special type for it
#[derive(Clone, Debug, PartialEq)]
pub struct PrevHash(pub Vec<u8>);

impl From<PrevHash> for Vec<u8> {
    fn from(p_hash: PrevHash) -> Self {
        p_hash.0
    }
}

impl TryFrom<&str> for PrevHash {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self, Error> {
        // Reorder prevhash will be stored via this cursor
        let mut prev_hash_cursor = std::io::Cursor::new(Vec::new());

        // Decode the plain byte array and sanity check
        // let prev_hash_stratum_order = hex_decode(value).context("Parsing hex bytes failed")?;
        let prev_hash_stratum_order = hex_decode(value)?;
        // if prev_hash_stratum_order.len() != 32 {
        //     return Err(ErrorKind::Json(format!(
        //         "Incorrect prev hash length: {}",
        //         prev_hash_stratum_order.len()
        //     ));
        //     //.into());
        // }
        // Swap every u32 from big endian to little endian byte order
        for chunk in prev_hash_stratum_order.chunks(size_of::<u32>()) {
            let prev_hash_word = BigEndian::read_u32(chunk);
            prev_hash_cursor
                .write_u32::<LittleEndian>(prev_hash_word)
                .expect("Internal error: Could not write buffer");
        }

        Ok(PrevHash(prev_hash_cursor.into_inner()))
    }
}

impl From<PrevHash> for Value {
    fn from(ph: PrevHash) -> Self {
        Into::<String>::into(ph).into()
    }
}

/// Helper Serializer that peforms the reverse process of converting the prev hash into stratum V1
/// ordering
impl From<PrevHash> for String {
    fn from(v: PrevHash) -> Self {
        let mut prev_hash_stratum_cursor = std::io::Cursor::new(Vec::new());
        // swap every u32 from little endian to big endian
        for chunk in v.0.chunks(size_of::<u32>()) {
            let prev_hash_word = LittleEndian::read_u32(chunk);
            prev_hash_stratum_cursor
                .write_u32::<BigEndian>(prev_hash_word)
                .expect("Internal error: Could not write buffer");
        }
        hex::encode(prev_hash_stratum_cursor.into_inner())
    }
}
