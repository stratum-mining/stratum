//! Provides functionality to strip coinbase_tx_prefix and coinbase_tx_suffix from bip141 marker,
//! flag and witness data, and to tell which witness a coinbase must carry.
extern crate alloc;

use alloc::{vec, vec::Vec};
use bitcoin::{TxOut, Witness};

const MARKER_OFFSET: usize = 4;
const FLAG_OFFSET: usize = 5;
/// The only witness serialization flag BIP141 currently defines. Other nonzero values are
/// reserved for future layouts that may carry additional serialized sections.
const BIP141_FLAG: u8 = 0x01;
const MARKER_FLAG_LEN: usize = 2;
const INPUT_COUNT_OFFSET: usize = MARKER_OFFSET + MARKER_FLAG_LEN;
const WITNESS_COUNT_LEN: usize = 1;
const WITNESS_LEN_LEN: usize = 1;
const WITNESS_DATA_LEN: usize = 32;
const LOCKTIME_LEN: usize = 4;
const BIP141_WITNESS_LEN: usize = WITNESS_COUNT_LEN + WITNESS_LEN_LEN + WITNESS_DATA_LEN;
/// Start of a witness commitment output script: `OP_RETURN`, a 36-byte push and the `aa21a9ed`
/// commitment header.
const WITNESS_COMMITMENT_HEADER: [u8; 6] = [0x6a, 0x24, 0xaa, 0x21, 0xa9, 0xed];
const WITNESS_COMMITMENT_LEN: usize = 32;
#[cfg(test)]
const MIN_COINBASE_TX_SUFFIX_LEN: usize = BIP141_WITNESS_LEN + LOCKTIME_LEN;

#[derive(Debug)]
pub enum StripBip141Error {
    FailedToDeserializeCoinbaseVersion,
    FailedToDeserializeCoinbaseInputs,
    FailedToDeserializeCoinbaseOutputs,
    FailedToDeserializeCoinbaseLockTime,
    FailedToDeserializeCoinbaseWitness,
    /// The coinbase carries the bip141 marker with a flag other than the only one BIP141 defines
    /// (`0x01`); the witness layout under any other flag is unknown.
    UnsupportedBip141Flag(u8),
    /// The witness section does not frame exactly one 32-byte item, which a coinbase carrying the
    /// witness commitment must have.
    UnexpectedWitnessFraming,
    /// The coinbase does not declare exactly one input. Each input has its own witness stack, so
    /// with more than one the suffix would hold more witness data than the single stack stripped
    /// here.
    UnexpectedInputCount,
}

/// Returns the witness of the input of a coinbase paying to `outputs`.
///
/// BIP141 only lets a block leave out the witness commitment when none of its transactions
/// carries witness data, and a coinbase witness is witness data. So the coinbase carries the
/// 32-byte witness reserved value when `outputs` include a commitment, and no witness otherwise.
pub(crate) fn coinbase_witness(outputs: &[TxOut]) -> Witness {
    let has_witness_commitment = outputs.iter().any(|output| {
        let script = output.script_pubkey.as_bytes();
        script.len() >= WITNESS_COMMITMENT_HEADER.len() + WITNESS_COMMITMENT_LEN
            && script.starts_with(&WITNESS_COMMITMENT_HEADER)
    });

    if has_witness_commitment {
        // 32 bytes of zeros is only safe to assume now, this could change in future soft forks
        Witness::from(vec![vec![0; WITNESS_DATA_LEN]])
    } else {
        Witness::new()
    }
}

/// Returns the size of the bip141 marker and flag in the serialization of a transaction whose
/// only input carries `witness`: they are serialized only when there is witness data.
pub(crate) fn marker_flag_len(witness: &Witness) -> usize {
    if witness.is_empty() {
        0
    } else {
        MARKER_FLAG_LEN
    }
}

/// Tries to strip the bip141 marker, flag and witness data from `coinbase_tx_prefix` and
/// `coinbase_tx_suffix` of a `NewExtendedMiningJob`.
///
/// This helps calculate the coinbase `txid` (instead of `wtxid`) for merkle root calculation, and
/// it is particularly helpful for translation to Sv1.
///
/// Returns `Ok(None)` when the bytes carry no bip141 marker and flag, whether they were already
/// stripped or the coinbase never had a witness (no witness commitment output): they can be used
/// as they are. Otherwise returns `Ok(Some((Vec<u8>, Vec<u8>)))` with the stripped
/// `coinbase_tx_prefix` and `coinbase_tx_suffix`.
///
/// # Errors
///
/// Returns [`StripBip141Error::FailedToDeserializeCoinbaseInputs`] when
/// `coinbase_tx_prefix` is too short to contain the marker and flag, or, when it carries them,
/// the input count that follows.
///
/// Returns [`StripBip141Error::UnsupportedBip141Flag`] when `coinbase_tx_prefix` carries the
/// bip141 marker with a flag other than the only currently defined value (`0x01`): other nonzero
/// flags are reserved for future serialization layouts whose witness section cannot be assumed
/// to have the fixed size stripped here.
///
/// Returns [`StripBip141Error::UnexpectedInputCount`] when `coinbase_tx_prefix` carries the
/// bip141 marker and flag but does not declare exactly one input, as a coinbase does.
///
/// Returns [`StripBip141Error::FailedToDeserializeCoinbaseLockTime`] when a transaction with a
/// bip141 marker and flag has a suffix too short to contain the lock time (suffix &lt; 4 bytes), or
/// [`StripBip141Error::FailedToDeserializeCoinbaseWitness`] when the suffix is too short to
/// contain the witness data and lock time (4 ≤ suffix &lt; 38 bytes).
///
/// Returns [`StripBip141Error::UnexpectedWitnessFraming`] when the witness framing does not
/// declare exactly one 32-byte item (the reserved value a coinbase carrying the witness
/// commitment must have).
#[allow(clippy::type_complexity)]
pub fn try_strip_bip141(
    coinbase_tx_prefix: &[u8],
    coinbase_tx_suffix: &[u8],
) -> Result<Option<(Vec<u8>, Vec<u8>)>, StripBip141Error> {
    // https://github.com/bitcoinbook/bitcoinbook/blob/third_edition_github/ch06_transactions.adoc#extended-marker-and-flag
    let (Some(&marker), Some(&flag)) = (
        coinbase_tx_prefix.get(MARKER_OFFSET),
        coinbase_tx_prefix.get(FLAG_OFFSET),
    ) else {
        return Err(StripBip141Error::FailedToDeserializeCoinbaseInputs);
    };
    let has_bip141_marker_and_flag = marker == 0x00 && flag != 0x00;

    if !has_bip141_marker_and_flag {
        return Ok(None);
    }

    // only the flag BIP141 defines today is known to carry the witness layout stripped below
    if flag != BIP141_FLAG {
        return Err(StripBip141Error::UnsupportedBip141Flag(flag));
    }

    // only the last witness stack is stripped below, which is the whole witness section only
    // when the transaction has a single input
    match coinbase_tx_prefix.get(INPUT_COUNT_OFFSET) {
        None => return Err(StripBip141Error::FailedToDeserializeCoinbaseInputs),
        Some(&1) => {}
        Some(_) => return Err(StripBip141Error::UnexpectedInputCount),
    }

    // strip bip141 marker and flag bytes from coinbase_tx_prefix
    let mut coinbase_tx_prefix_stripped_bip141 = coinbase_tx_prefix[0..MARKER_OFFSET].to_vec();
    coinbase_tx_prefix_stripped_bip141
        .extend_from_slice(&coinbase_tx_prefix[MARKER_OFFSET + MARKER_FLAG_LEN..]);

    // strip bip141 witness bytes from coinbase_tx_suffix
    let locktime_position = coinbase_tx_suffix
        .len()
        .checked_sub(LOCKTIME_LEN)
        .ok_or(StripBip141Error::FailedToDeserializeCoinbaseLockTime)?;
    let witness_position = locktime_position
        .checked_sub(BIP141_WITNESS_LEN)
        .ok_or(StripBip141Error::FailedToDeserializeCoinbaseWitness)?;

    // a coinbase carrying the witness commitment has exactly one witness item, the 32-byte
    // reserved value; any other framing is not the witness this function knows how to strip
    if coinbase_tx_suffix[witness_position] != 1
        || coinbase_tx_suffix[witness_position + WITNESS_COUNT_LEN] != WITNESS_DATA_LEN as u8
    {
        return Err(StripBip141Error::UnexpectedWitnessFraming);
    }

    // strip witness count, witness length and witness data
    let mut coinbase_tx_suffix_stripped_bip141 = coinbase_tx_suffix[..witness_position].to_vec();
    coinbase_tx_suffix_stripped_bip141.extend_from_slice(&coinbase_tx_suffix[locktime_position..]);

    Ok(Some((
        coinbase_tx_prefix_stripped_bip141,
        coinbase_tx_suffix_stripped_bip141,
    )))
}

#[cfg(test)]
mod tests {

    use alloc::vec;

    use super::*;
    use bitcoin::{
        absolute::LockTime, consensus::serialize, hashes::Hash, transaction::Version, Amount,
        OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Txid, Witness,
    };

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

    #[test]
    fn test_try_strip_bip141_rejects_short_prefix() {
        let coinbase_tx_suffix = [0; MIN_COINBASE_TX_SUFFIX_LEN];

        for prefix_len in 0..=FLAG_OFFSET {
            let coinbase_tx_prefix = vec![0; prefix_len];

            assert!(matches!(
                try_strip_bip141(&coinbase_tx_prefix, &coinbase_tx_suffix),
                Err(StripBip141Error::FailedToDeserializeCoinbaseInputs)
            ));
        }

        // with the marker and flag, the input count must follow them
        assert!(matches!(
            try_strip_bip141(&[0, 0, 0, 0, 0, 1], &coinbase_tx_suffix),
            Err(StripBip141Error::FailedToDeserializeCoinbaseInputs)
        ));
    }

    #[test]
    fn test_try_strip_bip141_rejects_short_locktime_suffix() {
        let coinbase_tx_prefix = [0, 0, 0, 0, 0, 1, 1];

        for suffix_len in 0..LOCKTIME_LEN {
            let coinbase_tx_suffix = vec![0; suffix_len];

            assert!(matches!(
                try_strip_bip141(&coinbase_tx_prefix, &coinbase_tx_suffix),
                Err(StripBip141Error::FailedToDeserializeCoinbaseLockTime)
            ));
        }
    }

    #[test]
    fn test_try_strip_bip141_rejects_short_witness_suffix() {
        let coinbase_tx_prefix = [0, 0, 0, 0, 0, 1, 1];

        for suffix_len in LOCKTIME_LEN..MIN_COINBASE_TX_SUFFIX_LEN {
            let coinbase_tx_suffix = vec![0; suffix_len];

            assert!(matches!(
                try_strip_bip141(&coinbase_tx_prefix, &coinbase_tx_suffix),
                Err(StripBip141Error::FailedToDeserializeCoinbaseWitness)
            ));
        }
    }

    #[test]
    fn test_try_strip_bip141_accepts_minimum_lengths() {
        let coinbase_tx_prefix = [0, 0, 0, 0, 0, 1, 1];
        let mut coinbase_tx_suffix = [0; MIN_COINBASE_TX_SUFFIX_LEN];
        coinbase_tx_suffix[0] = 1;
        coinbase_tx_suffix[1] = WITNESS_DATA_LEN as u8;

        let result = try_strip_bip141(&coinbase_tx_prefix, &coinbase_tx_suffix)
            .expect("minimum valid bip141 parts should be accepted")
            .expect("bip141 marker and flag should be stripped");

        assert_eq!(result, (vec![0, 0, 0, 0, 1], vec![0; 4]));
    }

    #[test]
    fn test_try_strip_bip141_rejects_unsupported_flag() {
        // only flag 0x01 is defined; other nonzero flags may gate additional serialized
        // sections, so an otherwise well-formed suffix must not be stripped under them
        let mut coinbase_tx_suffix = [0; MIN_COINBASE_TX_SUFFIX_LEN];
        coinbase_tx_suffix[0] = 1;
        coinbase_tx_suffix[1] = WITNESS_DATA_LEN as u8;

        for flag in 0x02..=0xff {
            let coinbase_tx_prefix = [0, 0, 0, 0, 0, flag, 1];

            assert!(matches!(
                try_strip_bip141(&coinbase_tx_prefix, &coinbase_tx_suffix),
                Err(StripBip141Error::UnsupportedBip141Flag(rejected)) if rejected == flag
            ));
        }
    }

    #[test]
    fn test_try_strip_bip141_rejects_invalid_witness_framing() {
        let coinbase_tx_prefix = [0, 0, 0, 0, 0, 1, 1];
        let mut coinbase_tx_suffix = [0; MIN_COINBASE_TX_SUFFIX_LEN];

        // a committed coinbase witness holds exactly one stack item
        coinbase_tx_suffix[0] = 2;
        coinbase_tx_suffix[1] = WITNESS_DATA_LEN as u8;
        assert!(matches!(
            try_strip_bip141(&coinbase_tx_prefix, &coinbase_tx_suffix),
            Err(StripBip141Error::UnexpectedWitnessFraming)
        ));

        // and that item is exactly the 32-byte reserved value
        coinbase_tx_suffix[0] = 1;
        coinbase_tx_suffix[1] = (WITNESS_DATA_LEN - 1) as u8;
        assert!(matches!(
            try_strip_bip141(&coinbase_tx_prefix, &coinbase_tx_suffix),
            Err(StripBip141Error::UnexpectedWitnessFraming)
        ));
    }

    #[test]
    fn test_try_strip_bip141_rejects_input_count_other_than_one() {
        let mut coinbase_tx_suffix = [0; MIN_COINBASE_TX_SUFFIX_LEN];
        coinbase_tx_suffix[0] = 1;
        coinbase_tx_suffix[1] = WITNESS_DATA_LEN as u8;

        for input_count in (0..=u8::MAX).filter(|input_count| *input_count != 1) {
            let coinbase_tx_prefix = [0, 0, 0, 0, 0, 1, input_count];

            assert!(matches!(
                try_strip_bip141(&coinbase_tx_prefix, &coinbase_tx_suffix),
                Err(StripBip141Error::UnexpectedInputCount)
            ));
        }
    }

    #[test]
    fn test_try_strip_bip141_rejects_two_input_transaction() {
        // both inputs carry a single 32-byte witness item, so the last witness stack alone is
        // framed exactly like a coinbase's
        let transaction = Transaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: vec![
                TxIn {
                    previous_output: OutPoint::null(),
                    script_sig: ScriptBuf::from(vec![0; 8]),
                    sequence: Sequence::MAX,
                    witness: Witness::from(vec![vec![0; WITNESS_DATA_LEN]]),
                },
                TxIn {
                    previous_output: OutPoint::new(Txid::from_byte_array([1; 32]), 0),
                    script_sig: ScriptBuf::new(),
                    sequence: Sequence::MAX,
                    witness: Witness::from(vec![vec![1; WITNESS_DATA_LEN]]),
                },
            ],
            output: vec![TxOut {
                value: Amount::ZERO,
                script_pubkey: ScriptBuf::new(),
            }],
        };
        let serialized = serialize(&transaction);
        // split in the middle of the first input's scriptSig, where the extranonce would sit
        let split = INPUT_COUNT_OFFSET + 1 + 36 + 1 + 4;
        let (coinbase_tx_prefix, coinbase_tx_suffix) = serialized.split_at(split);

        assert!(matches!(
            try_strip_bip141(coinbase_tx_prefix, coinbase_tx_suffix),
            Err(StripBip141Error::UnexpectedInputCount)
        ));
    }
}
