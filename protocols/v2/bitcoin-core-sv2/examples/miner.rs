//! A simple example of how to use `BitcoinCoreSv2`.
//!
//! We connect to the Bitcoin Core UNIX socket, and try to find a solution for the first provided
//! template.
//!
//! Since we're essentially doing CPU mining (under a tokio runtime), the hashrate is very low.
//! So running this example is only practical on networks with a very low difficulty.
//! For example, on regtest, or on a custom signet (without signatures).
//!
//! Please also note that we're not doing the full e2e Sv2 mining stack here. So this is not fully
//! emulating a realistic mining setup. We just want to find a solution for a template in the
//! simplest way possible. Also the coinbase doesn't pay anyone, so it's essentially burning money.

use bitcoin_core_sv2::BitcoinCoreSv2;
use std::path::Path;

use async_channel::unbounded;
use binary_sv2::U256;
use parsers_sv2::TemplateDistribution;
use stratum_core::bitcoin::{
    CompactTarget, Sequence,
    absolute::LockTime,
    block::{Block, Header, Version as BlockVersion},
    blockdata::witness::Witness,
    consensus::{Decodable, serialize},
    hash_types::{BlockHash, TxMerkleNode},
    hashes::Hash,
    transaction::{OutPoint, Transaction, TxIn, TxOut, Version},
};
use std::io::Cursor;
use template_distribution_sv2::{
    CoinbaseOutputConstraints, NewTemplate, RequestTransactionData, RequestTransactionDataSuccess,
    SetNewPrevHash, SubmitSolution,
};
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // the user must provide the path to the Bitcoin Core UNIX socket
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <bitcoin_core_unix_socket_path>", args[0]);
        eprintln!("Example: {} /path/to/bitcoin/regtest/node.sock", args[0]);
        std::process::exit(1);
    }

    let bitcoin_core_unix_socket_path = args[1].clone();

    // `BitcoinCoreSv2` uses this to cancel internally spawned tasks
    let cancellation_token = CancellationToken::new();

    // get new templates whenever the mempool has changed by more than 100 sats
    let fee_threshold = 100;

    // these messages are sent into the `BitcoinCoreSv2` instance
    let (msg_sender_into_bitcoin_core_sv2, msg_receiver_into_bitcoin_core_sv2) = unbounded();
    // these messages are received from the `BitcoinCoreSv2` instance
    let (msg_sender_from_bitcoin_core_sv2, msg_receiver_from_bitcoin_core_sv2) = unbounded();

    // clone so we can move it
    let cancellation_token_clone = cancellation_token.clone();

    // clone so we can move it
    let msg_sender_into_bitcoin_core_sv2_clone = msg_sender_into_bitcoin_core_sv2.clone();

    // spawn a dedicated thread to run the `BitcoinCoreSv2` instance
    // since it can only run inside a `LocalSet`
    std::thread::spawn(move || {
        // Create a new tokio runtime for this thread
        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async move {
            // `capnp` clients are not `Send`, so we need to use a `LocalSet` to run them
            let tokio_local_set = tokio::task::LocalSet::new();

            // run the BitcoinCoreSv2 instance inside a LocalSet
            tokio_local_set
                .run_until(async move {
                    // create a new `BitcoinCoreSv2` instance
                    let mut sv2_bitcoin_core = match BitcoinCoreSv2::new(
                        Path::new(&bitcoin_core_unix_socket_path),
                        fee_threshold,
                        msg_receiver_into_bitcoin_core_sv2,
                        msg_sender_from_bitcoin_core_sv2,
                        cancellation_token_clone.clone(),
                    )
                    .await
                    {
                        Ok(sv2_bitcoin_core) => sv2_bitcoin_core,
                        Err(e) => {
                            tracing::error!("Failed to create BitcoinCoreToSv2: {:?}", e);
                            cancellation_token_clone.cancel();
                            return;
                        }
                    };

                    // run the `BitcoinCoreSv2` instance,
                    // which will block until the cancellation token is activated
                    sv2_bitcoin_core.run().await;
                })
                .await;
        });
    });

    // send the initial `CoinbaseOutputConstraints` message to the `BitcoinCoreSv2` instance
    msg_sender_into_bitcoin_core_sv2_clone
        .send(TemplateDistribution::CoinbaseOutputConstraints(
            CoinbaseOutputConstraints {
                coinbase_output_max_additional_size: 2,
                coinbase_output_max_additional_sigops: 2,
            },
        ))
        .await
        .unwrap();

    tracing::info!("Sent CoinbaseOutputConstraints");

    // receive the first `NewTemplate` message from the `BitcoinCoreSv2` instance
    let TemplateDistribution::NewTemplate(new_template) =
        msg_receiver_from_bitcoin_core_sv2.recv().await.unwrap()
    else {
        return;
    };
    tracing::info!("Received: {:?}", new_template);

    // receive the `SetNewPrevHash` message from the `BitcoinCoreSv2` instance
    let TemplateDistribution::SetNewPrevHash(set_new_prev_hash) =
        msg_receiver_from_bitcoin_core_sv2.recv().await.unwrap()
    else {
        return;
    };
    tracing::info!("Received: {:?}", set_new_prev_hash);

    // send the `RequestTransactionData` message to the `BitcoinCoreSv2` instance
    msg_sender_into_bitcoin_core_sv2_clone
        .send(TemplateDistribution::RequestTransactionData(
            RequestTransactionData {
                template_id: new_template.template_id,
            },
        ))
        .await
        .unwrap();

    // receive the `RequestTransactionDataSuccess` message from the `BitcoinCoreSv2` instance
    let TemplateDistribution::RequestTransactionDataSuccess(request_transaction_data_success) =
        msg_receiver_from_bitcoin_core_sv2.recv().await.unwrap()
    else {
        return;
    };
    tracing::info!("Received: {:?}", request_transaction_data_success);

    // find the solution for the first template
    let solution = find_solution(
        new_template,
        set_new_prev_hash,
        request_transaction_data_success,
    );

    // send the `SubmitSolution` message to the `BitcoinCoreSv2` instance
    msg_sender_into_bitcoin_core_sv2_clone
        .send(TemplateDistribution::SubmitSolution(solution))
        .await
        .unwrap();
    tracing::info!("Sent SubmitSolution");

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("Ctrl+C received");
            cancellation_token.cancel();
            return;
        }
        _ = cancellation_token.cancelled() => {
            println!("Cancellation token activated");
            return;
        }
    }
}

/// Finds a solution for the given template.
fn find_solution(
    new_template: NewTemplate<'_>,
    set_new_prev_hash: SetNewPrevHash<'_>,
    request_transaction_data_success: RequestTransactionDataSuccess<'_>,
) -> SubmitSolution<'static> {
    let mut txdata: Vec<Transaction> = request_transaction_data_success
        .transaction_list
        .to_vec()
        .iter()
        .map(|tx| Transaction::consensus_decode(&mut Cursor::new(tx.to_vec())).unwrap())
        .collect();

    let coinbase = {
        let mut cursor = Cursor::new(new_template.coinbase_tx_outputs.to_vec());

        let outputs = (0..new_template.coinbase_tx_outputs_count)
            .map(|_| TxOut::consensus_decode(&mut cursor).unwrap())
            .collect();

        let tx_in = TxIn {
            previous_output: OutPoint::null(),
            script_sig: new_template.coinbase_prefix.to_vec().into(),
            sequence: Sequence(new_template.coinbase_tx_input_sequence),
            witness: Witness::from(vec![vec![0; 32]]),
        };

        Transaction {
            version: Version::non_standard(new_template.coinbase_tx_version as i32),
            lock_time: LockTime::from_consensus(new_template.coinbase_tx_locktime),
            input: vec![tx_in],
            output: outputs,
        }
    };

    // replace dummy coinbase tx with the actual coinbase tx from the solution
    txdata[0] = coinbase.clone();

    let mut nonce = 0;

    let mut header = Header {
        version: BlockVersion::from_consensus(new_template.version as i32),
        prev_blockhash: u256_to_block_hash(set_new_prev_hash.prev_hash.into_static()),
        merkle_root: TxMerkleNode::all_zeros(), // Will be computed from txdata
        time: set_new_prev_hash.header_timestamp,
        bits: CompactTarget::from_consensus(set_new_prev_hash.n_bits),
        nonce,
    };

    let dummy_block = Block { header, txdata };

    // compute the merkle root of the block
    header.merkle_root = dummy_block.compute_merkle_root().unwrap();

    // start the timer for periodically logging the progress
    let mut start = std::time::SystemTime::now();

    // find the nonce that satisfies the PoW requirement of a valid block
    loop {
        nonce = nonce.wrapping_add(1);
        header.nonce = nonce;
        if header.validate_pow(header.target()).is_ok() {
            tracing::info!("Found solution with nonce: {}", nonce);
            break;
        }

        let now = std::time::SystemTime::now();
        if now.duration_since(start).unwrap().as_secs() > 1 {
            tracing::info!("trying to find solution...");
            start = now;
        }
    }

    let serialized_coinbase_tx = serialize(&coinbase);

    SubmitSolution {
        template_id: new_template.template_id,
        version: new_template.version,
        header_timestamp: set_new_prev_hash.header_timestamp,
        header_nonce: nonce,
        coinbase_tx: serialized_coinbase_tx.try_into().unwrap(),
    }
}

/// Converts a `U256` to a [`BlockHash`] type.
fn u256_to_block_hash(v: U256<'static>) -> BlockHash {
    let hash: [u8; 32] = v.to_vec().try_into().unwrap();
    let hash = Hash::from_slice(&hash).unwrap();
    BlockHash::from_raw_hash(hash)
}
