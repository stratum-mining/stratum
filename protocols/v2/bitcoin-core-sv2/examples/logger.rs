//! A simple example of how to use `BitcoinCoreSv2`.
//!
//! We connect to the Bitcoin Core UNIX socket, and log the received Sv2 Template Distribution
//! Protocol messages.

use bitcoin_core_sv2::BitcoinCoreSv2;
use std::path::Path;

use async_channel::unbounded;
use parsers_sv2::TemplateDistribution;
use template_distribution_sv2::{CoinbaseOutputConstraints, RequestTransactionData};
use tokio_util::sync::CancellationToken;
use tracing::info;

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

    let bitcoin_core_unix_socket_path = Path::new(&args[1]);

    // `BitcoinCoreSv2` uses this to cancel internally spawned tasks
    let cancellation_token = CancellationToken::new();

    // some dummy coinbase output constraints
    // in the form of a Sv2 CoinbaseOutputConstraints message
    let coinbase_output_constraints = CoinbaseOutputConstraints {
        coinbase_output_max_additional_size: 1,
        coinbase_output_max_additional_sigops: 1,
    };

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

    // a task to consume and log the received Sv2 Template Distribution Protocol messages
    tokio::spawn(async move {
        loop {
            tokio::select! {
                // monitor for Ctrl+C, activating the cancellation token and exiting the loop
                _ = tokio::signal::ctrl_c() => {
                    info!("Ctrl+C received");
                    cancellation_token_clone.cancel();
                    return;
                }
                // monitor potential internal activations of the cancellation token for exiting the loop
                _ = cancellation_token_clone.cancelled() => {
                    info!("Cancellation token activated");
                    return;
                }
                // monitor for Sv2 Template Distribution Protocol messages
                // coming from `BitcoinCoreSv2`
                Ok(template_distribution_message) = msg_receiver_from_bitcoin_core_sv2.recv() => {
                    // log the message
                    info!("Message received: {}", template_distribution_message);

                    // send a RequestTransactionData every time a NewTemplate message is received
                    if let TemplateDistribution::NewTemplate(new_template) = template_distribution_message {
                        let template_id = new_template.template_id;
                        let request_transaction_data = TemplateDistribution::RequestTransactionData(RequestTransactionData {
                            template_id,
                        });

                        match msg_sender_into_bitcoin_core_sv2_clone.send(request_transaction_data).await {
                            Ok(_) => (),
                            Err(e) => {
                                tracing::error!("Failed to send request transaction data: {}", e);
                                cancellation_token_clone.cancel();
                                return;
                            }
                        }
                    }
                }
            }
        }
    });

    let cancellation_token_clone = cancellation_token.clone();

    let new_coinbase_output_constraints =
        TemplateDistribution::CoinbaseOutputConstraints(CoinbaseOutputConstraints {
            coinbase_output_max_additional_size: 2,
            coinbase_output_max_additional_sigops: 2,
        });

    // clone so we can move it
    let msg_sender_into_bitcoin_core_sv2_clone = msg_sender_into_bitcoin_core_sv2.clone();

    // spawn a task to periodically send new coinbase output constraints
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    info!("Ctrl+C received");
                    cancellation_token_clone.cancel();
                    return;
                }
                _ = cancellation_token_clone.cancelled() => {
                    info!("Cancellation token activated");
                    return;
                }
                // refresh coinbase output constraints every 10 seconds
                _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => {
                    match msg_sender_into_bitcoin_core_sv2_clone.send(new_coinbase_output_constraints.clone()).await {
                        Ok(_) => (),
                        Err(e) => {
                            tracing::error!("Failed to send new coinbase output constraints: {}", e);
                            cancellation_token_clone.cancel();
                            return;
                        }
                    }
                    info!("Sent new CoinbaseOutputConstraints");
                }
            }
        }
    });

    // `capnp` clients are not `Send`, so we need to use a `LocalSet` to run them
    let tokio_local_set = tokio::task::LocalSet::new();

    // run the BitcoinCoreSv2 instance inside a LocalSet
    tokio_local_set
        .run_until(async move {
            // create a new `BitcoinCoreSv2` instance
            let sv2_bitcoin_core = match BitcoinCoreSv2::new(
                Path::new(&bitcoin_core_unix_socket_path),
                coinbase_output_constraints,
                fee_threshold,
                msg_receiver_into_bitcoin_core_sv2,
                msg_sender_from_bitcoin_core_sv2,
                cancellation_token.clone(),
            )
            .await
            {
                Ok(sv2_bitcoin_core) => sv2_bitcoin_core,
                Err(e) => {
                    tracing::error!("Failed to create BitcoinCoreToSv2: {:?}", e);
                    cancellation_token.cancel();
                    return;
                }
            };

            // run the `BitcoinCoreSv2` instance,
            // which will block until the cancellation token is activated
            sv2_bitcoin_core.run().await;
        })
        .await;
}
