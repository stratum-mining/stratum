//! Job Dispatcher
//!
//! This module contains relevant logic to maintain group channels in proxy roles such as:
//! - converting extended jobs to standard jobs
//! - handling updates to jobs when new templates and prev hashes arrive, as well as cleaning up old
//!   jobs
//! - determining if submitted shares correlate to valid jobs

use crate::utils::merkle_root_from_path;
use mining_sv2::{
    NewExtendedMiningJob, NewMiningJob, SubmitSharesError, SubmitSharesStandard, Target,
};
use std::convert::TryInto;

use bitcoin::hashes::{sha256d, Hash, HashEngine};

/// Used to convert an extended mining job to a standard mining job. The `extranonce` field must
/// be exactly 32 bytes.
pub fn extended_to_standard_job_for_group_channel<'a>(
    extended: &NewExtendedMiningJob,
    extranonce: &[u8],
    channel_id: u32,
    job_id: u32,
) -> Option<NewMiningJob<'a>> {
    let merkle_root = merkle_root_from_path(
        extended.coinbase_tx_prefix.inner_as_ref(),
        extended.coinbase_tx_suffix.inner_as_ref(),
        extranonce,
        &extended.merkle_path.inner_as_ref(),
    );

    Some(NewMiningJob {
        channel_id,
        job_id,
        min_ntime: extended.min_ntime.clone().into_static(),
        version: extended.version,
        merkle_root: merkle_root?.try_into().ok()?,
    })
}

// helper struct to easily calculate block hashes from headers
#[allow(dead_code)]
struct Header<'a> {
    version: u32,
    prev_hash: &'a [u8],
    merkle_root: &'a [u8],
    timestamp: u32,
    nbits: u32,
    nonce: u32,
}

impl Header<'_> {
    // calculates the sha256 blockhash of the header
    #[allow(dead_code)]
    pub fn hash(&self) -> Target {
        let mut engine = sha256d::Hash::engine();
        engine.input(&self.version.to_le_bytes());
        engine.input(self.prev_hash);
        engine.input(self.merkle_root);
        engine.input(&self.timestamp.to_be_bytes());
        engine.input(&self.nbits.to_be_bytes());
        engine.input(&self.nonce.to_be_bytes());
        let hashed: [u8; 32] = *sha256d::Hash::from_engine(engine).as_ref();
        hashed.into()
    }
}

/// Used to signal if submitted shares correlate to valid jobs
pub enum SendSharesResponse {
    /// ValidAndMeetUpstreamTarget((SubmitSharesStandard,SubmitSharesSuccess)),
    Valid(SubmitSharesStandard),
    Invalid(SubmitSharesError<'static>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use codec_sv2::binary_sv2::U256;

    #[test]
    fn test_block_hash() {
        let le_version = "0x32950000".strip_prefix("0x").unwrap();
        let be_prev_hash = "0x00000000000000000004e962c1a0fc6a201d937bf08ffe4b1221e956615c7cd9";
        let be_merkle_root = "0x897dff6755a7c255455f1b2a2c8ad44ad1b6c23ef00fbf501d0dde7e42cd8c71";
        let le_timestamp = "0x637B9A4C".strip_prefix("0x").unwrap();
        let le_nbits = "0x17079e15".strip_prefix("0x").unwrap();
        let le_nonce = "0x102aa10".strip_prefix("0x").unwrap();

        let le_version = u32::from_str_radix(le_version, 16).expect("Failed converting hex to u32");
        let mut be_prev_hash =
            utils::decode_hex(be_prev_hash).expect("Failed converting hex to bytes");
        let mut be_merkle_root =
            utils::decode_hex(be_merkle_root).expect("Failed converting hex to bytes");
        let le_timestamp: u32 =
            u32::from_str_radix(le_timestamp, 16).expect("Failed converting hex to u32");
        let le_nbits = u32::from_str_radix(le_nbits, 16).expect("Failed converting hex to u32");
        let le_nonce = u32::from_str_radix(le_nonce, 16).expect("Failed converting hex to u32");
        be_prev_hash.reverse();
        be_merkle_root.reverse();
        let le_prev_hash = be_prev_hash.as_slice();
        let le_merkle_root = be_merkle_root.as_slice();

        let block_header: Header = Header {
            version: le_version,
            prev_hash: le_prev_hash,
            merkle_root: le_merkle_root,
            timestamp: le_timestamp.to_be(),
            nbits: le_nbits.to_be(),
            nonce: le_nonce.to_be(),
        };

        let target = U256::from(block_header.hash());
        let mut actual_block_hash =
            utils::decode_hex("00000000000000000000199349a95526c4f83959f0ef06697048a297f25e7fac")
                .expect("Failed converting hex to bytes");
        actual_block_hash.reverse();
        assert_eq!(
            target.to_vec(),
            actual_block_hash,
            "Computed block hash does not equal the actaul block hash"
        );
    }

    pub mod utils {
        use std::fmt::Write;

        pub fn decode_hex(s: &str) -> Result<Vec<u8>, core::num::ParseIntError> {
            let s = match s.strip_prefix("0x") {
                Some(s) => s,
                None => s,
            };
            (0..s.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
                .collect()
        }

        pub fn _encode_hex(bytes: &[u8]) -> String {
            let mut s = String::with_capacity(bytes.len() * 2);
            for &b in bytes {
                write!(&mut s, "{b:02x}").unwrap();
            }
            s
        }
    }
}
