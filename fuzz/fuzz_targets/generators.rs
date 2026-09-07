//! These byte-sequence generators are based on the Stratum V2 specification.
//!
//! # Wire-valid only
//!
//! These generators produce **wire-format-valid** byte sequences, not
//! protocol-semantically valid messages. The output MUST always be parseable
//! by the corresponding `from_bytes` implementation for each type. Field
//! values may be intentionally non-spec-compliant, since the reference
//! implementation does not enforce protocol-level semantic constraints during
//! serialization or deserialization.
//!
//! If parsing fails, it may indicate a bug in the SRI implementation or an
//! issue in the generators themselves.
//!
//! Field layouts follow the current main branch of the Stratum V2
//! specification at revision: 025e2f76802d5942a10f6f3fea07941522b108ca.
//! Field comments use the specification's field names.
// Every fuzz target compiles this whole module, so generators used by only some
// targets show up as dead code in the others.
#![allow(dead_code)]

use arbitrary::Unstructured;

/// Generates a random-length byte payload with a maximum size of `max`.
///
/// The actual payload length is chosen by the fuzzer between 0 and `max`.
/// Choosing the length consumes input bytes, so the chosen length is clamped
/// to what remains afterward. This prevents requesting more bytes than are
/// available and avoids unnecessary parsing failures.
fn gen_var_payload(u: &mut Unstructured, max: usize) -> arbitrary::Result<Vec<u8>> {
    let cap = u.len().min(max);
    let len = u.int_in_range(0..=cap)?.min(u.len());
    Ok(u.bytes(len)?.to_vec())
}

// ---------------------------------------------------------------------------
// Fixed-size primitive (LE encoding)
// ---------------------------------------------------------------------------
/// Generates a single arbitrary byte.
pub fn gen_u8(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    Ok(u.arbitrary::<u8>()?.to_le_bytes().to_vec())
}

/// Generates a `u16` in little-endian wire format.
pub fn gen_u16(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    Ok(u.arbitrary::<u16>()?.to_le_bytes().to_vec())
}

/// Generates a `u32` in little-endian wire format.
pub fn gen_u32(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    Ok(u.arbitrary::<u32>()?.to_le_bytes().to_vec())
}

/// Generates a `u64` in little-endian wire format.
pub fn gen_u64(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    Ok(u.arbitrary::<u64>()?.to_le_bytes().to_vec())
}

/// Generates an `f32` in little-endian wire format.
pub fn gen_f32(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    Ok(u.arbitrary::<f32>()?.to_le_bytes().to_vec())
}

// the bool type has a weird behavior where we should only look at the
// LEAST SIGNIFICANT BIT; but this behavior will be fuzzed in the macros.
// Here I just generate canonical u8;
/// Generates a canonical `bool` as a single `0` or `1` byte.
pub fn gen_bool(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    Ok(vec![u.arbitrary::<bool>()? as u8])
}

/// Generates a `U24` in three-byte little-endian wire format.
pub fn gen_u24(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let raw = u.int_in_range(0u32..=0x00FF_FFFF)?;
    let b = raw.to_le_bytes();
    Ok(vec![b[0], b[1], b[2]])
}

/// Generates a `U256` as 32 arbitrary bytes.
pub fn gen_u256(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    Ok(u.arbitrary::<[u8; 32]>()?.to_vec())
}

/// Generates a `PubKey` as 32 arbitrary bytes.
pub fn gen_pubkey(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    Ok(u.arbitrary::<[u8; 32]>()?.to_vec())
}

/// Generates a `Signature` as 64 arbitrary bytes.
pub fn gen_signature(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    Ok(u.arbitrary::<[u8; 64]>()?.to_vec())
}

// ---------------------------------------------------------------------------
// Variable-length byte arrays (length-prefixed)
// ---------------------------------------------------------------------------

/// Generates a `B032` with a one-byte length prefix.
pub fn gen_b032(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let payload = gen_var_payload(u, 32)?;
    let mut buf = Vec::with_capacity(1 + payload.len());
    buf.push(payload.len() as u8);
    buf.extend_from_slice(&payload);
    Ok(buf)
}

/// Generates a `B0255` with a one-byte length prefix.
pub fn gen_b0255(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let payload = gen_var_payload(u, 255)?;
    let mut buf = Vec::with_capacity(1 + payload.len());
    buf.push(payload.len() as u8);
    buf.extend_from_slice(&payload);
    Ok(buf)
}

/// Generates a `Str0255` with a one-byte length prefix.
pub fn gen_str0255(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let payload = gen_var_payload(u, 255)?;
    let mut buf = Vec::with_capacity(1 + payload.len());
    buf.push(payload.len() as u8);
    buf.extend_from_slice(&payload);
    Ok(buf)
}

/// Capped at 512 bytes of payload to keep fuzzer corpus sizes manageable.
pub fn gen_b064k(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let payload = gen_var_payload(u, 512)?;
    let len = payload.len() as u16;
    let mut buf = Vec::with_capacity(2 + payload.len());
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(&payload);
    Ok(buf)
}

/// Capped at 100 bytes of payload to keep fuzzer corpus sizes manageable.
pub fn gen_b016m(u: &mut Unstructured) -> arbitrary::Result<Vec<u8>> {
    let payload = gen_var_payload(u, 100)?;
    let len = payload.len() as u32;
    let b = len.to_le_bytes();
    let mut buf = Vec::with_capacity(3 + payload.len());
    buf.extend_from_slice(&b[..3]); // 3 bytes LE
    buf.extend_from_slice(&payload);
    Ok(buf)
}

// ---------------------------------------------------------------------------
// Nested Types
// ---------------------------------------------------------------------------

/// Generates a `Seq0255` of up to `max_count` generated elements.
pub fn gen_seq0255(
    u: &mut Unstructured,
    gen_elem: impl FnMut(&mut Unstructured) -> arbitrary::Result<Vec<u8>>,
    max_count: usize,
) -> arbitrary::Result<Vec<u8>> {
    let mut gen_elem = gen_elem;
    let count = u.int_in_range(0..=max_count.min(255))?;
    let mut buf = Vec::with_capacity(1 + count);
    buf.push(count as u8);
    for _ in 0..count {
        buf.extend_from_slice(&gen_elem(u)?);
    }
    Ok(buf)
}

/// Generates a `Seq064K` of up to `max_count` generated elements.
pub fn gen_seq064k(
    u: &mut Unstructured,
    gen_elem: impl FnMut(&mut Unstructured) -> arbitrary::Result<Vec<u8>>,
    max_count: usize,
) -> arbitrary::Result<Vec<u8>> {
    let mut gen_elem = gen_elem;
    let count = u.int_in_range(0..=max_count.min(65535))?;
    let mut buf = Vec::with_capacity(2 + count);
    buf.extend_from_slice(&(count as u16).to_le_bytes());
    for _ in 0..count {
        buf.extend_from_slice(&gen_elem(u)?);
    }
    Ok(buf)
}

/// Generates an `Sv2Option`, optionally followed by the generated value.
pub fn gen_sv2_option(
    u: &mut Unstructured,
    mut gen_elem: impl FnMut(&mut Unstructured) -> arbitrary::Result<Vec<u8>>,
) -> arbitrary::Result<Vec<u8>> {
    if u.arbitrary::<bool>()? {
        let mut buf = vec![1u8];
        buf.extend_from_slice(&gen_elem(u)?);
        Ok(buf)
    } else {
        Ok(vec![0u8])
    }
}
