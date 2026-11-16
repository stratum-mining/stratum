use crate::error::{self, Error, ParamError};
use binary_sv2::{B032Owned, U256Owned};
use bitcoin::hashes::hex::{DisplayHex, FromHex};
use serde_json::{Number, Value};
use std::fmt;
use std::{convert::TryFrom, ops::BitAnd};

/// Version rolling mask as defined by
/// [BIP323](https://github.com/bitcoin/bips/blob/master/bip-0323.mediawiki).
///
/// Bits 5-28 of the block header version field are general-purpose bits that can be freely
/// manipulated by the miner. Bits 0-4 and 29-31 are reserved and must not be rolled.
pub const VERSION_ROLLING_MASK: u32 = 0x1FFFFFE0;

/// Helper type that allows simple serialization and deserialization of byte vectors
/// that are represented as hex strings in JSON.
/// Extranonce must be less than or equal to 32 bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Extranonce(pub B032Owned);

impl fmt::Display for Extranonce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.as_bytes().to_lower_hex_string())
    }
}

impl Extranonce {
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl TryFrom<Vec<u8>> for Extranonce {
    type Error = Error;
    fn try_from(value: Vec<u8>) -> Result<Self, Error> {
        Ok(Extranonce(B032Owned::try_from(value)?))
    }
}

impl From<Extranonce> for Vec<u8> {
    fn from(v: Extranonce) -> Self {
        v.0.into_bytes()
    }
}

impl From<Extranonce> for Value {
    fn from(eb: Extranonce) -> Self {
        Into::<String>::into(eb).into()
    }
}

/// Decodes a hexadecimal string into bytes.
///
/// Returns an error if the input has an odd length or contains
/// non-hexadecimal characters.
pub(crate) fn hex_decode(s: &str) -> Result<Vec<u8>, Error> {
    Ok(Vec::<u8>::from_hex(s)?)
}

/// Odd-length input is left-padded with `0`.
impl TryFrom<&str> for Extranonce {
    type Error = error::Error;

    fn try_from(value: &str) -> Result<Self, Error> {
        let bytes = if value.len() % 2 == 0 {
            hex_decode(value)?
        } else {
            hex_decode(&format!("0{value}"))?
        };
        Extranonce::try_from(bytes)
    }
}

impl From<Extranonce> for String {
    fn from(bytes: Extranonce) -> String {
        bytes.0.as_bytes().to_lower_hex_string()
    }
}

impl BitAnd<u32> for HexU32Be {
    type Output = HexU32Be;

    fn bitand(self, rhs: u32) -> Self::Output {
        HexU32Be(self.0 & rhs)
    }
}

/// Big-endian alternative of the HexU32
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct HexU32Be(pub u32);

impl fmt::Display for HexU32Be {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:08x}", self.0)
    }
}

impl fmt::Debug for HexU32Be {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HexU32Be({self})")
    }
}

impl HexU32Be {
    /// Whether this mask allows every bit set in `bits`.
    pub fn check_mask(&self, bits: &HexU32Be) -> bool {
        ((!self.0) & bits.0) == 0
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
        let expected_len = 8;
        if value.len() > expected_len {
            return Err(Error::InvalidHexLen(value.len()));
        }

        let delta_len = expected_len - value.len();
        let mut prefix = "".to_string();
        for _ in 0..delta_len {
            prefix.push('0');
        }
        prefix.push_str(value);
        let parsed_bytes: [u8; 4] = hex_decode(&prefix)?
            .try_into()
            .map_err(|_| Error::InvalidHexLen(prefix.len()))?;
        Ok(HexU32Be(u32::from_be_bytes(parsed_bytes)))
    }
}

/// Helper Serializer
impl From<HexU32Be> for String {
    fn from(v: HexU32Be) -> Self {
        v.0.to_be_bytes().to_lower_hex_string()
    }
}

impl From<u32> for HexU32Be {
    fn from(a: u32) -> Self {
        HexU32Be(a)
    }
}

/// PrevHash in Stratum V1 has brain-damaged serialization as it swaps bytes of every u32 word
/// into big endian. Therefore, we need a special type for it
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrevHash(pub U256Owned);

impl fmt::Display for PrevHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Reuse the Stratum V1 serialization logic
        let s = String::from(self.clone());
        f.write_str(&s)
    }
}

impl From<PrevHash> for Vec<u8> {
    fn from(p_hash: PrevHash) -> Self {
        p_hash.0.into_bytes()
    }
}

/// The bytes are taken in header order, without the word swap of the hex form.
impl TryFrom<Vec<u8>> for PrevHash {
    type Error = Error;

    fn try_from(value: Vec<u8>) -> Result<Self, Error> {
        Ok(PrevHash(U256Owned::try_from(value)?))
    }
}

impl TryFrom<&str> for PrevHash {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self, Error> {
        let mut bytes = hex_decode(value)?;
        reverse_words(&mut bytes);
        Ok(PrevHash(U256Owned::try_from(bytes)?))
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
        let mut bytes = v.0.into_bytes();
        reverse_words(&mut bytes);
        bytes.to_lower_hex_string()
    }
}

/// Reverses the byte order of each 4-byte word, converting between Stratum V1's previous-hash
/// encoding and the header's byte order.
fn reverse_words(bytes: &mut [u8]) {
    for word in bytes.chunks_exact_mut(4) {
        word.reverse();
    }
}

// / Referencing the internal part of hex bytes
impl AsRef<[u8]> for PrevHash {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

/// Referencing the internal part of hex bytes
impl AsRef<[u8]> for Extranonce {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MerkleNode(pub U256Owned);

impl fmt::Display for MerkleNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.as_bytes().to_lower_hex_string())
    }
}

impl TryFrom<Vec<u8>> for MerkleNode {
    type Error = Error;

    fn try_from(value: Vec<u8>) -> Result<Self, Error> {
        let merkle = U256Owned::try_from(value).map_err(error::Error::from)?;
        Ok(MerkleNode(merkle))
    }
}

impl From<MerkleNode> for Vec<u8> {
    fn from(v: MerkleNode) -> Self {
        v.0.into_bytes()
    }
}

impl From<MerkleNode> for Value {
    fn from(eb: MerkleNode) -> Self {
        Into::<String>::into(eb).into()
    }
}

/// Referencing the internal part of hex bytes
impl AsRef<[u8]> for MerkleNode {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl TryFrom<&str> for MerkleNode {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self, Error> {
        let merkle = U256Owned::try_from(hex_decode(value)?).map_err(error::Error::from)?;
        Ok(MerkleNode(merkle))
    }
}

impl From<MerkleNode> for String {
    fn from(bytes: MerkleNode) -> String {
        bytes.0.as_bytes().to_lower_hex_string()
    }
}

/// Bytes of any length, represented as a hex string in JSON.
#[derive(Clone, PartialEq, Eq)]
pub struct HexBytes(pub Vec<u8>);

impl fmt::Display for HexBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.to_lower_hex_string())
    }
}

impl fmt::Debug for HexBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HexBytes({self})")
    }
}

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
impl AsRef<[u8]> for HexBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
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
        bytes.0.to_lower_hex_string()
    }
}

// Readers for positional parameters and results, shared by the message modules.

pub(crate) fn params_array(params: &Value) -> Result<&[Value], ParamError> {
    params
        .as_array()
        .map(Vec::as_slice)
        .ok_or(ParamError::new("params", "not an array"))
}

/// The positional parameters of a message its decoder owns, so strings can be moved out.
pub(crate) fn params_array_mut(params: &mut Value) -> Result<&mut [Value], ParamError> {
    params
        .as_array_mut()
        .map(Vec::as_mut_slice)
        .ok_or(ParamError::new("params", "not an array"))
}

/// Short hex is left-padded.
pub(crate) fn hex_u32(value: &str, field: &'static str) -> Result<HexU32Be, ParamError> {
    HexU32Be::try_from(value).map_err(|_| ParamError::new(field, "not a 32-bit hex value"))
}

pub(crate) fn integer_u32(value: &Number, field: &'static str) -> Result<HexU32Be, ParamError> {
    value
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .map(HexU32Be)
        .ok_or(ParamError::new(field, "not a 32-bit unsigned integer"))
}

/// Reads a `version-rolling.min-bit-count`: an integer, or a decimal string.
///
/// The count only informs, so a value that is neither, or that is above 32, reads as absent
/// instead of failing the request.
pub(crate) fn min_bit_count(value: &Value) -> Option<u32> {
    let count = match value {
        Value::Number(count) => count.as_u64(),
        Value::String(count) => count.parse().ok(),
        _ => None,
    };
    count
        .and_then(|count| u32::try_from(count).ok())
        .filter(|count| *count <= u32::BITS)
}

pub(crate) fn extranonce2_size_param(size: &Number) -> Result<usize, ParamError> {
    size.as_u64()
        .and_then(|size| usize::try_from(size).ok())
        .ok_or(ParamError::new(
            "extranonce2_size",
            "not an unsigned integer",
        ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[quickcheck_macros::quickcheck]
    fn test_prev_hash(mut bytes: Vec<u8>) -> bool {
        bytes.resize(32, 0);
        let be_hex = bytes.to_lower_hex_string();
        let me = PrevHash::try_from(be_hex.clone().as_str()).unwrap();
        let back_to_hex = String::from(me.clone());
        let back_to_hex_value = Value::from(me.clone());
        let value_to_string = back_to_hex_value.as_str().unwrap();

        let chunk_size: usize = std::mem::size_of::<u32>();
        let me_chunks = me.0.as_bytes().chunks(chunk_size);
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

    #[test]
    fn prev_hash_try_from_vec_keeps_header_order() {
        let bytes: Vec<u8> = (0..32).collect();
        assert_eq!(PrevHash::try_from(bytes.clone()).unwrap().as_ref(), bytes);
        assert!(PrevHash::try_from(vec![0_u8; 31]).is_err());
    }

    #[test]
    fn merkle_node_try_from_vec_invalid_len_returns_err() {
        assert!(MerkleNode::try_from(vec![0_u8; 31]).is_err());
    }

    #[test]
    fn merkle_node_try_from_str_invalid_len_returns_err() {
        let invalid_hex = "00".repeat(31);
        assert!(MerkleNode::try_from(invalid_hex.as_str()).is_err());
    }
}
