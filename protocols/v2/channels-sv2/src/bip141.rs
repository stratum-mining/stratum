//! Provides functionality to strip coinbase_tx_prefix and coinbase_tx_suffix from bip141 marker,
//! flag and witness data.
extern crate alloc;

use alloc::vec::Vec;
use bitcoin::{blockdata::transaction::Transaction, consensus::Decodable};
use mining_sv2::MAX_EXTRANONCE_LEN;

use bitcoin::io::Cursor;

const MARKER_FLAG_OFFSET: usize = 4;
const MARKER_FLAG_LEN: usize = 2;
const WITNESS_COUNT_LEN: usize = 1;
const WITNESS_LEN_LEN: usize = 1;
const WITNESS_DATA_LEN: usize = 32;
const LOCKTIME_LEN: usize = 4;

#[derive(Debug)]
pub enum StripBip141Error {
    FailedToDeserializeCoinbaseVersion,
    FailedToDeserializeCoinbaseInputs,
    FailedToDeserializeCoinbaseOutputs,
    FailedToDeserializeCoinbaseLockTime,
}

/// Tries to strip the bip141 marker, flag and witness data from `coinbase_tx_prefix` and
/// `coinbase_tx_suffix` of a `NewExtendedMiningJob`.
///
/// This helps calculate the coinbase `txid` (instead of `wtxid`) for merkle root calculation, and
/// it is particularly helpful for translation to Sv1.
///
/// If the coinbase transaction is already stripped of bip141, returns `Ok(None)`.
/// If the coinbase transaction is not stripped, returns `Ok(Some((Vec<u8>, Vec<u8>)))` with the
/// stripped `coinbase_tx_prefix` and `coinbase_tx_suffix`.
#[allow(clippy::type_complexity)]
pub fn try_strip_bip141(
    coinbase_tx_prefix: &[u8],
    coinbase_tx_suffix: &[u8],
) -> Result<Option<(Vec<u8>, Vec<u8>)>, StripBip141Error> {
    let mut encoded = Vec::with_capacity(
        coinbase_tx_prefix.len() + coinbase_tx_suffix.len() + MAX_EXTRANONCE_LEN,
    );
    encoded.extend_from_slice(coinbase_tx_prefix);
    encoded.extend_from_slice(&[0; MAX_EXTRANONCE_LEN]);
    encoded.extend_from_slice(coinbase_tx_suffix);

    let mut decoder = Cursor::new(encoded);
    let coinbase = Transaction {
        version: Decodable::consensus_decode(&mut decoder)
            .map_err(|_| StripBip141Error::FailedToDeserializeCoinbaseVersion)?,
        input: Decodable::consensus_decode(&mut decoder)
            .map_err(|_| StripBip141Error::FailedToDeserializeCoinbaseInputs)?,
        output: Decodable::consensus_decode(&mut decoder)
            .map_err(|_| StripBip141Error::FailedToDeserializeCoinbaseOutputs)?,
        lock_time: Decodable::consensus_decode(&mut decoder)
            .map_err(|_| StripBip141Error::FailedToDeserializeCoinbaseLockTime)?,
    };

    if coinbase.compute_txid().to_raw_hash() == coinbase.compute_wtxid().to_raw_hash() {
        return Ok(None);
    }

    // strip bip141 marker and flag bytes from coinbase_tx_prefix
    let mut coinbase_tx_prefix_stripped_bip141 = coinbase_tx_prefix[0..MARKER_FLAG_OFFSET].to_vec();
    coinbase_tx_prefix_stripped_bip141
        .extend_from_slice(&coinbase_tx_prefix[MARKER_FLAG_OFFSET + MARKER_FLAG_LEN..]);

    // strip bip141 witness bytes from coinbase_tx_suffix
    let locktime_position = coinbase_tx_suffix.len() - LOCKTIME_LEN;

    // strip witness count, witness length and witness data
    let mut coinbase_tx_suffix_stripped_bip141 = coinbase_tx_suffix
        [..locktime_position - WITNESS_COUNT_LEN - WITNESS_LEN_LEN - WITNESS_DATA_LEN]
        .to_vec();
    coinbase_tx_suffix_stripped_bip141.extend_from_slice(&coinbase_tx_suffix[locktime_position..]);

    Ok(Some((
        coinbase_tx_prefix_stripped_bip141,
        coinbase_tx_suffix_stripped_bip141,
    )))
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_try_strip_bip141_sri_stripped() {
        // taken from a job sent by SRI that already stripped bip141
        let coinbase_tx_prefix = [
            2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 60, 2, 69, 8, 0, 22, 47, 83, 116, 114, 97,
            116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 47, 47, 32,
        ]
        .to_vec();
        let coinbase_tx_suffix = [
            254, 255, 255, 255, 2, 0, 242, 5, 42, 1, 0, 0, 0, 22, 0, 20, 235, 225, 183, 220, 194,
            147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194, 8, 252, 0, 0, 0, 0, 0, 0,
            0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209, 222, 253, 63, 169,
            153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180, 139, 235, 216, 54,
            151, 78, 140, 249, 68, 8, 0, 0,
        ]
        .to_vec();

        let result = try_strip_bip141(&coinbase_tx_prefix, &coinbase_tx_suffix)
            .expect("failed trying to strip bip141");

        assert!(result.is_none());
    }

    #[test]
    fn test_try_strip_bip141_braiins() {
        // taken from a job sent by Braiins (which strips bip141)
        let coinbase_tx_prefix = [
            1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 76, 3, 206, 226, 13, 15, 47, 115, 108, 117,
            115, 104, 47, 183, 0, 23, 4, 15, 174, 96, 163, 250, 190, 109, 109, 207, 170, 255, 170,
            165, 167, 227, 112, 68, 27, 6, 55, 218, 219, 107, 176, 134, 175, 81, 46, 136, 141, 22,
            52, 201, 113, 254, 3, 148, 98, 86, 129, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 146, 162, 70, 0,
        ]
        .to_vec();
        let coinbase_tx_suffix = [
            255, 255, 255, 255, 3, 194, 35, 192, 18, 0, 0, 0, 0, 23, 169, 20, 31, 12, 187, 236,
            139, 196, 201, 69, 228, 225, 98, 73, 177, 30, 238, 145, 30, 222, 213, 95, 135, 0, 0, 0,
            0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 255, 27, 83, 239, 4, 153, 53, 158, 142,
            142, 137, 160, 34, 151, 40, 92, 185, 56, 40, 203, 144, 158, 95, 20, 155, 139, 53, 163,
            229, 145, 31, 81, 0, 0, 0, 0, 0, 0, 0, 0, 43, 106, 41, 82, 83, 75, 66, 76, 79, 67, 75,
            58, 188, 174, 124, 112, 138, 29, 26, 158, 194, 209, 140, 216, 148, 128, 27, 238, 106,
            189, 119, 214, 62, 100, 213, 246, 143, 66, 110, 12, 0, 120, 89, 116, 0, 0, 0, 0,
        ]
        .to_vec();

        let result = try_strip_bip141(&coinbase_tx_prefix, &coinbase_tx_suffix)
            .expect("failed trying to strip bip141");

        assert!(result.is_none());
    }

    #[test]
    fn test_try_strip_bip141_sri_not_yet_stripped() {
        // taken from SRI before bip141 was stripped
        let coinbase_tx_prefix = [
            2, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 60, 2, 69, 8, 0, 22, 47, 83, 116,
            114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 47, 47, 32,
        ]
        .to_vec();
        let coinbase_tx_suffix = [
            254, 255, 255, 255, 2, 0, 242, 5, 42, 1, 0, 0, 0, 22, 0, 20, 235, 225, 183, 220, 194,
            147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194, 8, 252, 0, 0, 0, 0, 0, 0,
            0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209, 222, 253, 63, 169,
            153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180, 139, 235, 216, 54,
            151, 78, 140, 249, 1, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 68, 8, 0, 0,
        ]
        .to_vec();

        let (coinbase_tx_prefix_stripped_bip141, coinbase_tx_suffix_stripped_bip141) =
            try_strip_bip141(&coinbase_tx_prefix, &coinbase_tx_suffix)
                .expect("failed trying to strip bip141")
                .unwrap();

        assert_eq!(
            coinbase_tx_prefix_stripped_bip141,
            [
                2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 60, 2, 69, 8, 0, 22, 47, 83, 116,
                114, 97, 116, 117, 109, 32, 86, 50, 32, 83, 82, 73, 32, 80, 111, 111, 108, 47, 47,
                32
            ]
            .to_vec()
        );
        assert_eq!(
            coinbase_tx_suffix_stripped_bip141,
            [
                254, 255, 255, 255, 2, 0, 242, 5, 42, 1, 0, 0, 0, 22, 0, 20, 235, 225, 183, 220,
                194, 147, 204, 170, 14, 231, 67, 168, 111, 137, 223, 130, 88, 194, 8, 252, 0, 0, 0,
                0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209, 222,
                253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180, 139,
                235, 216, 54, 151, 78, 140, 249, 68, 8, 0, 0
            ]
            .to_vec()
        );
    }
}
