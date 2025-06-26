use crate::error::{self, Error};
use binary_sv2::{B032, U256};
use bitcoin_hashes::hex::{FromHex, ToHex};
use byteorder::{BigEndian, ByteOrder, LittleEndian, WriteBytesExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{convert::TryFrom, mem::size_of, ops::BitAnd};

/// Helper type that allows simple serialization and deserialization of byte vectors
/// that are represented as hex strings in JSON.
/// Extranonce must be less than or equal to 32 bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Extranonce<'a>(pub B032<'a>);

impl Extranonce<'_> {
    pub fn len(&self) -> usize {
        self.0.inner_as_ref().len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.inner_as_ref().is_empty()
    }
}

impl<'a> TryFrom<Vec<u8>> for Extranonce<'a> {
    type Error = Error<'a>;
    fn try_from(value: Vec<u8>) -> Result<Self, Error<'a>> {
        Ok(Extranonce(B032::try_from(value)?))
    }
}

impl<'a> From<Extranonce<'a>> for Vec<u8> {
    fn from(v: Extranonce<'a>) -> Self {
        v.0.to_vec()
    }
}

impl<'a> From<Extranonce<'a>> for Value {
    fn from(eb: Extranonce<'a>) -> Self {
        Into::<String>::into(eb).into()
    }
}

/// fix for error on odd-length hex sequences
/// FIXME: find a nicer solution
fn hex_decode(s: &str) -> Result<Vec<u8>, Error<'static>> {
    if s.len() % 2 != 0 {
        Ok(hex::decode(format!("0{s}"))?)
    } else {
        Ok(hex::decode(s)?)
    }
}

impl<'a> TryFrom<&str> for Extranonce<'a> {
    type Error = error::Error<'a>;

    fn try_from(value: &str) -> Result<Self, Error<'a>> {
        Ok(Extranonce(B032::try_from(hex_decode(value)?)?))
    }
}

impl<'a> From<Extranonce<'a>> for String {
    fn from(bytes: Extranonce<'a>) -> String {
        hex::encode(bytes.0)
    }
}

impl BitAnd<u32> for HexU32Be {
    type Output = u32;

    fn bitand(self, rhs: u32) -> Self::Output {
        self.0 & rhs
    }
}

impl<'a> From<B032<'a>> for Extranonce<'a> {
    fn from(b: B032<'a>) -> Self {
        Extranonce(b)
    }
}

/// Big-endian alternative of the HexU32
#[derive(Clone, Debug, PartialEq, Eq)]
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
    type Error = Error<'static>;

    fn try_from(value: &str) -> Result<Self, Error<'static>> {
        let expected_len = 8;
        let delta_len = expected_len - value.len();
        let mut prefix = "".to_string();
        for _ in 0..delta_len {
            prefix.push('0');
        }
        prefix.push_str(value);
        let parsed_bytes: [u8; 4] = FromHex::from_hex(&prefix)?;
        Ok(HexU32Be(u32::from_be_bytes(parsed_bytes)))
    }
}

/// Helper Serializer
impl From<HexU32Be> for String {
    fn from(v: HexU32Be) -> Self {
        v.0.to_be_bytes().to_hex()
    }
}

impl From<u32> for HexU32Be {
    fn from(a: u32) -> Self {
        HexU32Be(a)
    }
}

//Example of serialization for testing purpose
impl Serialize for HexU32Be {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let serialized_string = self.0.to_be_bytes().to_hex();
        serializer.serialize_str(&serialized_string)
    }
}

//Example of deserialization for testing purpose
impl<'de> Deserialize<'de> for HexU32Be {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let hex_string: String = Deserialize::deserialize(deserializer)?;

        match u32::from_str_radix(&hex_string, 16) {
            Ok(value) => Ok(HexU32Be(value)),
            Err(_) => Err(serde::de::Error::custom("Invalid hex value")),
        }
    }
}

/// PrevHash in Stratum V1 has brain-damaged serialization as it swaps bytes of every u32 word
/// into big endian. Therefore, we need a special type for it
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrevHash<'a>(pub U256<'a>);

impl<'a> From<PrevHash<'a>> for Vec<u8> {
    fn from(p_hash: PrevHash<'a>) -> Self {
        p_hash.0.to_vec()
    }
}

impl<'a> TryFrom<&str> for PrevHash<'a> {
    type Error = Error<'a>;

    fn try_from(value: &str) -> Result<Self, Error<'a>> {
        // Reorder PrevHash will be stored via this cursor
        // let mut prev_hash_cursor = std::io::Cursor::new([0_u8; 32]);
        let mut prev_hash_arr = [0_u8; 32];

        // Decode the plain byte array and sanity check
        let prev_hash_stratum_order = hex_decode(value)?;

        match prev_hash_stratum_order.len() {
            32 => {
                // Swap every u32 from big endian to little endian byte order
                for (chunk, mut arr_chunks) in prev_hash_stratum_order
                    .chunks(size_of::<u32>())
                    .zip(prev_hash_arr.chunks_mut(size_of::<u32>()))
                {
                    let prev_hash_word = BigEndian::read_u32(chunk);
                    arr_chunks
                        .write_u32::<LittleEndian>(prev_hash_word)
                        .expect("Internal error: Could not write buffer");
                }
                Ok(PrevHash(prev_hash_arr.into()))
            }
            _ => Err(error::Error::BadBytesConvert(
                binary_sv2::Error::InvalidU256(prev_hash_stratum_order.len()),
            )),
        }
    }
}

impl From<PrevHash<'_>> for Value {
    fn from(ph: PrevHash) -> Self {
        Into::<String>::into(ph).into()
    }
}

/// Helper Serializer that peforms the reverse process of converting the prev hash into stratum V1
/// ordering
impl From<PrevHash<'_>> for String {
    fn from(v: PrevHash) -> Self {
        let mut prev_hash_stratum_cursor = std::io::Cursor::new(Vec::new());
        // swap every u32 from little endian to big endian
        for chunk in v.0.inner_as_ref().chunks(size_of::<u32>()) {
            let prev_hash_word = LittleEndian::read_u32(chunk);
            prev_hash_stratum_cursor
                .write_u32::<BigEndian>(prev_hash_word)
                .expect("Internal error: Could not write buffer");
        }
        hex::encode(prev_hash_stratum_cursor.into_inner())
    }
}

// / Referencing the internal part of hex bytes
impl AsRef<[u8]> for PrevHash<'_> {
    fn as_ref(&self) -> &[u8] {
        self.0.inner_as_ref()
    }
}

/// Referencing the internal part of hex bytes
impl<'a> AsRef<U256<'a>> for PrevHash<'a> {
    fn as_ref(&self) -> &U256<'a> {
        &self.0
    }
}

/// Referencing the internal part of hex bytes
impl AsRef<[u8]> for Extranonce<'_> {
    fn as_ref(&self) -> &[u8] {
        self.0.inner_as_ref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MerkleNode<'a>(pub U256<'a>);

impl MerkleNode<'_> {
    pub fn is_empty(&self) -> bool {
        self.0.inner_as_ref().is_empty()
    }
}

impl<'a> TryFrom<Vec<u8>> for MerkleNode<'a> {
    type Error = Error<'a>;

    fn try_from(value: Vec<u8>) -> Result<Self, Error<'a>> {
        //TODO handle error
        Ok(MerkleNode(U256::try_from(value).unwrap()))
    }
}

impl<'a> From<MerkleNode<'a>> for Vec<u8> {
    fn from(v: MerkleNode<'a>) -> Self {
        v.0.to_vec()
    }
}

impl<'a> From<MerkleNode<'a>> for Value {
    fn from(eb: MerkleNode<'a>) -> Self {
        Into::<String>::into(eb).into()
    }
}

/// Referencing the internal part of hex bytes
impl AsRef<[u8]> for MerkleNode<'_> {
    fn as_ref(&self) -> &[u8] {
        self.0.inner_as_ref()
    }
}

impl<'a> TryFrom<&str> for MerkleNode<'a> {
    type Error = Error<'a>;

    fn try_from(value: &str) -> Result<Self, Error<'a>> {
        //TODO handle error
        Ok(MerkleNode(U256::try_from(hex_decode(value)?).unwrap()))
    }
}

impl<'a> From<MerkleNode<'a>> for String {
    fn from(bytes: MerkleNode<'a>) -> String {
        hex::encode(bytes.0)
    }
}

/// Helper type that allows simple serialization and deserialization of byte vectors
/// that are represented as hex strings in JSON.
/// HexBytes must be less than or equal to 32 bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
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

impl TryFrom<&str> for HexBytes {
    type Error = Error<'static>;

    fn try_from(value: &str) -> Result<Self, Error<'static>> {
        Ok(HexBytes(hex_decode(value)?))
    }
}

impl From<HexBytes> for String {
    fn from(bytes: HexBytes) -> String {
        hex::encode(bytes.0)
    }
}

//Example of serialization for testing purpose
impl Serialize for HexBytes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let bytes = self.0.as_ref();
        let serialized_string = String::from_utf8_lossy(bytes);
        serializer.serialize_str(&serialized_string)
    }
}

//Example of deserialization for testing purpose
impl<'de> Deserialize<'de> for HexBytes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let hex_string: String = Deserialize::deserialize(deserializer)?;
        let bytes = hex_string.as_bytes().to_vec();
        Ok(HexBytes(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[quickcheck_macros::quickcheck]
    fn test_prev_hash(mut bytes: Vec<u8>) -> bool {
        bytes.resize(32, 0);
        let be_hex = bytes.to_hex();
        let me = PrevHash::try_from(be_hex.clone().as_str()).unwrap();
        let back_to_hex = String::from(me.clone());
        let back_to_hex_value = Value::from(me.clone());
        let value_to_string = back_to_hex_value.as_str().unwrap();

        let chunk_size: usize = size_of::<u32>();
        let me_chunks = me.clone().0.to_vec();
        let me_chunks = me_chunks.chunks(chunk_size);
        for (be_chunk, le_chunk) in bytes.clone().chunks(chunk_size).zip(me_chunks) {
            let le_chunk = [le_chunk[0], le_chunk[1], le_chunk[2], le_chunk[3]];
            let be_chunk = [be_chunk[0], be_chunk[1], be_chunk[2], be_chunk[3]];
            let le_u32 = u32::from_le_bytes(le_chunk);
            let be_u32 = u32::from_be_bytes(be_chunk);

            if le_u32 != be_u32 {
                return false;
            }
        }

        be_hex == back_to_hex && be_hex == value_to_string
    }
}
