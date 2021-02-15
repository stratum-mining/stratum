use bitcoin_hashes::hex::{FromHex, ToHex};
use byteorder::{BigEndian, ByteOrder, LittleEndian, WriteBytesExt};
use hex::FromHexError;
use serde_json::Value;
use std::convert::TryFrom;
use std::mem::size_of;

/// Helper type that allows simple serialization and deserialization of byte vectors
/// that are represented as hex strings in JSON
#[derive(Clone, Debug, PartialEq)]
pub struct HexBytes(pub(crate) Vec<u8>);

impl HexBytes {
    pub fn len(&self) -> usize {
        self.0.len()
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
fn hex_decode(s: &str) -> std::result::Result<Vec<u8>, FromHexError> {
    if s.len() % 2 != 0 {
        hex::decode(&format!("0{}", s))
    } else {
        hex::decode(s)
    }
}

impl TryFrom<&str> for HexBytes {
    type Error = FromHexError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Ok(HexBytes(hex_decode(value)?))
    }
}

impl From<HexBytes> for String {
    fn from(bytes: HexBytes) -> String {
        hex::encode(bytes.0)
    }
}


/// Big-endian alternative of the HexU32
/// TODO: find out how to consolidate/parametrize it with generic parameters
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
    type Error = bitcoin_hashes::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let parsed_bytes: [u8; 4] = FromHex::from_hex(value)?;
        Ok(HexU32Be(u32::from_be_bytes(parsed_bytes)))
    }
}

/// Helper Serializer
impl Into<String> for HexU32Be {
    fn into(self) -> String {
        self.0.to_be_bytes().to_hex()
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

/// TODO: implement unit test
impl TryFrom<&str> for PrevHash {
    type Error = FromHexError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        // Reorder prevhash will be stored via this cursor
        let mut prev_hash_cursor = std::io::Cursor::new(Vec::new());

        // Decode the plain byte array and sanity check
        // let prev_hash_stratum_order = hex_decode(value).context("Parsing hex bytes failed")?;
        let prev_hash_stratum_order = hex_decode(value)?;
        // TODO
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
/// TODO: implement unit test
impl Into<String> for PrevHash {
    fn into(self) -> String {
        let mut prev_hash_stratum_cursor = std::io::Cursor::new(Vec::new());
        // swap every u32 from little endian to big endian
        for chunk in self.0.chunks(size_of::<u32>()) {
            let prev_hash_word = LittleEndian::read_u32(chunk);
            prev_hash_stratum_cursor
                .write_u32::<BigEndian>(prev_hash_word)
                .expect("Internal error: Could not write buffer");
        }
        hex::encode(prev_hash_stratum_cursor.into_inner())
    }
}
