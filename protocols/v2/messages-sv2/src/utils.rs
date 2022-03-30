//! Useful struct used into this crate and by crates that want to interact with this one
use crate::errors::Error;
use binary_sv2::U256;
use bitcoin::hashes::sha256d::Hash as DHash;
use bitcoin::{
    blockdata::block::BlockHeader,
    hash_types::{BlockHash, TxMerkleNode},
    hashes::{sha256d, Hash, HashEngine},
};
use std::convert::TryInto;
use std::sync::{Mutex as Mutex_, MutexGuard, PoisonError}; //compact_target_from_u256

/// Generator of unique ids
#[derive(Debug, PartialEq)]
pub struct Id {
    state: u32,
}

impl Id {
    pub fn new() -> Self {
        Self { state: 0 }
    }
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u32 {
        self.state += 1;
        self.state
    }
}

impl Default for Id {
    fn default() -> Self {
        Self::new()
    }
}

/// Safer Mutex wrapper
#[derive(Debug)]
pub struct Mutex<T: ?Sized>(Mutex_<T>);

impl<T> Mutex<T> {
    pub fn safe_lock<F, Ret>(&self, thunk: F) -> Result<Ret, PoisonError<MutexGuard<'_, T>>>
    where
        F: FnOnce(&mut T) -> Ret,
    {
        let mut lock = self.0.lock()?;
        let return_value = thunk(&mut *lock);
        drop(lock);
        Ok(return_value)
    }

    pub fn new(v: T) -> Self {
        Mutex(Mutex_::new(v))
    }

    pub fn to_remove(&self) -> MutexGuard<'_, T> {
        self.0.lock().unwrap()
    }
}

pub(crate) fn merkle_root_from_path(
    coinbase_tx_prefix: &[u8],
    coinbase_script: &[u8],
    coinbase_tx_suffix: &[u8],
    path: &[&[u8]],
) -> Vec<u8> {
    // RR TODO: catch empty cb
    if !coinbase_tx_prefix.len() == 46 {
        panic!("TODO: add error that checks cb prefix is 46 bytes");
    }
    let mut coinbase = Vec::with_capacity(
        coinbase_tx_prefix.len() + coinbase_tx_suffix.len() + coinbase_script.len(),
    );
    coinbase.extend_from_slice(coinbase_tx_prefix);
    coinbase.extend_from_slice(coinbase_script);
    coinbase.extend_from_slice(coinbase_tx_suffix);

    let mut engine = sha256d::Hash::engine();
    engine.input(&coinbase);
    let coinbase = sha256d::Hash::from_engine(engine);

    let root = path.iter().fold(coinbase, |root, leaf| {
        let mut engine = sha256d::Hash::engine();
        engine.input(&root);
        engine.input(leaf);
        sha256d::Hash::from_engine(engine)
    });

    root.to_vec()
}

/// Returns a new `BlockHeader`.
/// Expected endianness inputs:
/// version     LE
/// prev_hash   BE
/// merkle_root BE
/// time        BE
/// bits        BE
/// nonce       BE
pub(crate) fn new_header(
    version: i32,
    prev_hash: &[u8],
    merkle_root: &[u8],
    time: u32,
    bits: u32,
    nonce: u32,
) -> Result<BlockHeader, Error> {
    if prev_hash.len() != 32 {
        return Err(Error::ExpectedLen32(prev_hash.len()));
    }
    if merkle_root.len() != 32 {
        return Err(Error::ExpectedLen32(merkle_root.len()));
    }
    let mut prev_hash_arr = [0u8; 32];
    prev_hash_arr.copy_from_slice(prev_hash);
    let prev_hash = DHash::from_inner(prev_hash_arr);

    let mut merkle_root_arr = [0u8; 32];
    merkle_root_arr.copy_from_slice(merkle_root);
    let merkle_root = DHash::from_inner(merkle_root_arr);

    Ok(BlockHeader {
        version,
        prev_blockhash: BlockHash::from_hash(prev_hash),
        merkle_root: TxMerkleNode::from_hash(merkle_root),
        time,
        bits,
        nonce,
    })
}

/// Returns hash of the `BlockHeader`.
/// Endianness reference for the correct hash:
/// version     LE
/// prev_hash   BE
/// merkle_root BE
/// time        BE
/// bits        BE
/// nonce       BE
pub(crate) fn new_header_hash<'decoder>(header: BlockHeader) -> U256<'decoder> {
    let hash = header.block_hash().to_vec();
    hash.try_into().unwrap()
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "serde")]
    use super::*;
    use binary_sv2::{Seq0255, B064K, U256};
    #[cfg(feature = "serde")]
    use serde::Deserialize;

    #[cfg(feature = "serde")]
    use std::convert::TryInto;
    #[cfg(feature = "serde")]
    use std::num::ParseIntError;

    #[cfg(feature = "serde")]
    fn decode_hex(s: &str) -> Result<Vec<u8>, ParseIntError> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
            .collect()
    }

    #[cfg(feature = "serde")]
    #[derive(Debug, Deserialize)]
    struct TestBlockToml {
        block_hash: String,
        version: u32,
        prev_hash: String,
        time: u32,
        merkle_root: String,
        nbits: u32,
        nonce: u32,
        coinbase_tx_prefix: String,
        coinbase_script: String,
        coinbase_tx_suffix: String,
        path: Vec<String>,
    }

    #[derive(Debug)]
    struct TestBlock<'decoder> {
        block_hash: U256<'decoder>,
        version: u32,
        prev_hash: Vec<u8>,
        time: u32,
        merkle_root: Vec<u8>,
        nbits: u32,
        nonce: u32,
        coinbase_tx_prefix: B064K<'decoder>,
        coinbase_script: Vec<u8>,
        coinbase_tx_suffix: B064K<'decoder>,
        path: Seq0255<'decoder, U256<'decoder>>,
    }
    #[cfg(feature = "serde")]
    fn get_test_block<'decoder>() -> TestBlock<'decoder> {
        let test_file = std::fs::read_to_string("../../../test_data/reg-test-block.toml")
            .expect("Could not read file from string");
        let block: TestBlockToml =
            toml::from_str(&test_file).expect("Could not parse toml file as `TestBlockToml`");

        // Get block hash
        let block_hash_vec =
            decode_hex(&block.block_hash).expect("Could not decode hex string to `Vec<u8>`");
        let mut block_hash_vec: [u8; 32] = block_hash_vec
            .try_into()
            .expect("Slice is incorrect length");
        block_hash_vec.reverse();
        let block_hash: U256 = block_hash_vec
            .try_into()
            .expect("Could not convert `[u8; 32]` to `U256`");

        // Get prev hash
        let mut prev_hash: Vec<u8> =
            decode_hex(&block.prev_hash).expect("Could not convert `String` to `&[u8]`");
        prev_hash.reverse();

        // Get Merkle root
        let mut merkle_root =
            decode_hex(&block.merkle_root).expect("Could not decode hex string to `Vec<u8>`");
        // Swap endianness to LE
        merkle_root.reverse();

        // Get Merkle path
        let mut path_vec = Vec::<U256>::new();
        for p in block.path {
            let p_vec = decode_hex(&p).expect("Could not decode hex string to `Vec<u8>`");
            let p_arr: [u8; 32] = p_vec.try_into().expect("Slice is incorrect length");
            let p_u256: U256 = (p_arr)
                .try_into()
                .expect("Could not convert to `U256` from `[u8; 32]`");
            path_vec.push(p_u256);
        }

        let path = Seq0255::new(path_vec).expect("Could not convert `Vec<U256>` to `Seq0255`");

        // Pass in coinbase as three pieces:
        //   coinbase_tx_prefix + coinbase script + coinbase_tx_suffix
        let coinbase_tx_prefix_vec = decode_hex(&block.coinbase_tx_prefix)
            .expect("Could not decode hex string to `Vec<u8>`");
        let coinbase_tx_prefix: B064K = coinbase_tx_prefix_vec
            .try_into()
            .expect("Could not convert `Vec<u8>` into `B064K`");

        let coinbase_script =
            decode_hex(&block.coinbase_script).expect("Could not decode hex `String` to `Vec<u8>`");

        let coinbase_tx_suffix_vec = decode_hex(&block.coinbase_tx_suffix)
            .expect("Could not decode hex `String` to `Vec<u8>`");
        let coinbase_tx_suffix: B064K = coinbase_tx_suffix_vec
            .try_into()
            .expect("Could not convert `Vec<u8>` to `B064K`");

        TestBlock {
            block_hash,
            version: block.version,
            prev_hash,
            time: block.time,
            merkle_root,
            nbits: block.nbits,
            nonce: block.nonce,
            coinbase_tx_prefix,
            coinbase_script,
            coinbase_tx_suffix,
            path,
        }
    }
    #[test]
    #[cfg(feature = "serde")]
    fn gets_merkle_root_from_path() {
        let block = get_test_block();
        let expect: Vec<u8> = block.merkle_root;

        let actual = merkle_root_from_path(
            block.coinbase_tx_prefix.inner_as_ref(),
            &block.coinbase_script,
            block.coinbase_tx_suffix.inner_as_ref(),
            &block.path.inner_as_ref(),
        );
        assert_eq!(expect, actual);
    }

    #[test]
    #[cfg(feature = "serde")]
    fn gets_new_header() -> Result<(), Error> {
        let block = get_test_block();

        if !block.prev_hash.len() == 32 {
            return Err(Error::ExpectedLen32(block.prev_hash.len()));
        }
        if !block.merkle_root.len() == 32 {
            return Err(Error::ExpectedLen32(block.merkle_root.len()));
        }
        let mut prev_hash_arr = [0u8; 32];
        prev_hash_arr.copy_from_slice(&block.prev_hash);
        let prev_hash = DHash::from_inner(prev_hash_arr);

        let mut merkle_root_arr = [0u8; 32];
        merkle_root_arr.copy_from_slice(&block.merkle_root);
        let merkle_root = DHash::from_inner(merkle_root_arr);

        let expect = BlockHeader {
            version: block.version as i32,
            prev_blockhash: BlockHash::from_hash(prev_hash),
            merkle_root: TxMerkleNode::from_hash(merkle_root),
            time: block.time,
            bits: block.nbits,
            nonce: block.nonce,
        };

        let actual_block = get_test_block();
        let actual = new_header(
            block.version as i32,
            &actual_block.prev_hash,
            &actual_block.merkle_root,
            block.time,
            block.nbits,
            block.nonce,
        )?;
        assert_eq!(actual, expect);
        Ok(())
    }

    #[test]
    #[cfg(feature = "serde")]
    fn gets_new_header_hash() {
        let block = get_test_block();
        let expect = block.block_hash;
        let block = get_test_block();
        let prev_hash: [u8; 32] = block.prev_hash.to_vec().try_into().unwrap();
        let prev_hash = DHash::from_inner(prev_hash);
        let merkle_root: [u8; 32] = block.merkle_root.to_vec().try_into().unwrap();
        let merkle_root = DHash::from_inner(merkle_root);
        let header = BlockHeader {
            version: block.version as i32,
            prev_blockhash: BlockHash::from_hash(prev_hash),
            merkle_root: TxMerkleNode::from_hash(merkle_root),
            time: block.time,
            bits: block.nbits,
            nonce: block.nonce,
        };

        let actual = new_header_hash(header);

        assert_eq!(actual, expect);
    }
}
