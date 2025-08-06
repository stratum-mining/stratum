use corepc_node::{types::GetBlockchainInfo, Conf, ConnectParams, Node};
use std::{env, fs::create_dir_all, path::PathBuf};
use stratum_common::roles_logic_sv2::bitcoin::{Address, Amount, Txid};
use tracing::warn;

use crate::utils::{fs_utils, http, tarball};

const VERSION_TP: &str = "0.1.15";

fn get_bitcoind_filename(os: &str, arch: &str) -> String {
    match (os, arch) {
        ("macos", "aarch64") => {
            format!("bitcoin-sv2-tp-{VERSION_TP}-arm64-apple-darwin-unsigned.tar.gz")
        }
        ("macos", "x86_64") => {
            format!("bitcoin-sv2-tp-{VERSION_TP}-x86_64-apple-darwin-unsigned.tar.gz")
        }
        ("linux", "x86_64") => format!("bitcoin-sv2-tp-{VERSION_TP}-x86_64-linux-gnu.tar.gz"),
        ("linux", "aarch64") => format!("bitcoin-sv2-tp-{VERSION_TP}-aarch64-linux-gnu.tar.gz"),
        _ => format!("bitcoin-sv2-tp-{VERSION_TP}-x86_64-apple-darwin-unsigned.zip"),
    }
}

/// Represents a template provider node.
///
/// The template provider is a bitcoin node that implements the Stratum V2 protocol.
#[derive(Debug)]
pub struct TemplateProvider {
    bitcoind: Node,
}

/// Represents the consensus difficulty level of the network.
///
/// Low: regtest mode (every share is a block)
///
/// Mid: signet mode with genesis difficulty
/// (most of the time, a CPU should find a block in a minute or less)
///
/// High: signet mode with premined blocks raising difficulty to 77761.11
/// (most of the time, a CPU should take a REALLY long time to find a block)
///
/// Note: signet mode has signetchallenge=51, which means no signature is needed on the coinbase.
pub enum DifficultyLevel {
    Low,
    Mid,
    High,
}

impl TemplateProvider {
    /// Start a new [`TemplateProvider`] instance.
    pub fn start(port: u16, sv2_interval: u32, difficulty_level: DifficultyLevel) -> Self {
        let current_dir: PathBuf = std::env::current_dir().expect("failed to read current dir");
        let tp_dir = current_dir.join("template-provider");
        let mut conf = Conf::default();
        conf.wallet = Some(port.to_string());

        let staticdir = format!(".bitcoin-{port}");
        conf.staticdir = Some(tp_dir.join(staticdir.clone()));
        let port_arg = format!("-sv2port={port}");
        let sv2_interval_arg = format!("-sv2interval={sv2_interval}");

        match difficulty_level {
            DifficultyLevel::Low => {
                // use default corepc-node settings, which means regtest mode
                // where every share is a block
            }
            DifficultyLevel::Mid => {
                // use signet mode with genesis difficulty
                // (signetchallenge=51, no signature needed on the coinbase)
                // most of the time, a CPU should find a block in a minute or less
                conf.args = vec!["-signet", "-fallbackfee=0.0001", "-signetchallenge=51"];
                conf.network = "signet";
            }
            DifficultyLevel::High => {
                // use signet mode with premined blocks raising difficulty to 77761.11
                // (signetchallenge=51, no signature needed on the coinbase)
                // most of the time, a CPU should take a REALLY long time to find a block
                conf.args = vec!["-signet", "-fallbackfee=0.0001", "-signetchallenge=51"];
                conf.network = "signet";

                // Create signet datadir
                let signet_datadir = tp_dir.join(staticdir.clone()).join("signet");
                create_dir_all(signet_datadir.clone()).expect("Failed to create signet directory");

                // Copy high difficulty signet data into signet datadir
                let high_diff_chain_dir = current_dir.join("high_diff_chain");
                fs_utils::copy_dir_contents(&high_diff_chain_dir, &signet_datadir)
                    .expect("Failed to copy high difficulty chain data");
            }
        }

        conf.args.extend(vec![
            "-txindex=1",
            "-sv2",
            &port_arg,
            "-debug=rpc",
            "-debug=sv2",
            &sv2_interval_arg,
            "-sv2feedelta=0",
            "-loglevel=sv2:trace",
            "-logtimemicros=1",
        ]);
        let os = env::consts::OS;
        let arch = env::consts::ARCH;
        let download_filename = get_bitcoind_filename(os, arch);
        let bitcoin_exe_home = tp_dir
            .join(format!("bitcoin-sv2-tp-{VERSION_TP}"))
            .join("bin");

        if !bitcoin_exe_home.exists() {
            let tarball_bytes = match env::var("BITCOIND_TARBALL_FILE") {
                Ok(path) => tarball::read_from_file(&path),
                Err(_) => {
                    warn!("Downloading template provider for the testing session. This could take a while...");
                    let download_endpoint =
                        env::var("BITCOIND_DOWNLOAD_ENDPOINT").unwrap_or_else(|_| {
                            "https://github.com/Sjors/bitcoin/releases/download".to_owned()
                        });
                    let url =
                        format!("{download_endpoint}/sv2-tp-{VERSION_TP}/{download_filename}");
                    http::make_get_request(&url, 5)
                }
            };

            if let Some(parent) = bitcoin_exe_home.parent() {
                create_dir_all(parent).unwrap();
            }

            tarball::unpack(&tarball_bytes, &tp_dir);

            if os == "macos" {
                let bitcoind_binary = bitcoin_exe_home.join("bitcoind");
                std::process::Command::new("codesign")
                    .arg("--sign")
                    .arg("-")
                    .arg(&bitcoind_binary)
                    .output()
                    .expect("Failed to sign bitcoind binary");
            }
        }

        env::set_var("BITCOIND_EXE", bitcoin_exe_home.join("bitcoind"));
        let exe_path = corepc_node::exe_path().expect("Failed to get bitcoind path");

        // this timeout is used to avoid potential racing conditions
        // on the bitcoind executable while executing Integration Tests in parallel
        // for more context, see https://github.com/stratum-mining/stratum/issues/1278#issuecomment-2692316174
        let timeout = std::time::Duration::from_secs(10);
        let current_time = std::time::Instant::now();
        loop {
            match Node::with_conf(&exe_path, &conf) {
                Ok(bitcoind) => {
                    break TemplateProvider { bitcoind };
                }
                Err(e) => {
                    if current_time.elapsed() > timeout {
                        panic!("Failed to start bitcoind: {e}");
                    }
                    println!("Failed to start bitcoind due to {e}");
                }
            }
        }
    }

    /// Mine `n` blocks.
    pub fn generate_blocks(&self, n: u64) {
        let mining_address = self
            .bitcoind
            .client
            .new_address()
            .expect("Failed to get mining address");
        self.bitcoind
            .client
            .generate_to_address(n as usize, &mining_address)
            .expect("Failed to generate blocks");
    }

    /// Retrun the node's RPC info.
    pub fn rpc_info(&self) -> &ConnectParams {
        &self.bitcoind.params
    }

    /// Return the result of `getblockchaininfo` RPC call.
    pub fn get_blockchain_info(&self) -> Result<GetBlockchainInfo, corepc_node::Error> {
        let client = &self.bitcoind.client;
        let blockchain_info = client.get_blockchain_info()?;
        Ok(blockchain_info)
    }

    /// Create and broadcast a transaction to the mempool.
    ///
    /// It is recommended to use [`TemplateProvider::fund_wallet`] before calling this method to
    /// ensure the wallet has enough funds.
    pub fn create_mempool_transaction(&self) -> Result<(Address, Txid), corepc_node::Error> {
        let client = &self.bitcoind.client;
        const MILLION_SATS: Amount = Amount::from_sat(1_000_000);
        let address = client.new_address()?;
        let txid = client
            .send_to_address(&address, MILLION_SATS)?
            .txid()
            .expect("Unexpected behavior: txid is None");
        Ok((address, txid))
    }

    /// Fund the node's wallet.
    ///
    /// This can be useful before using [`TemplateProvider::create_mempool_transaction`].
    pub fn fund_wallet(&self) -> Result<(), corepc_node::Error> {
        let client = &self.bitcoind.client;
        let address = client.new_address()?;
        client.generate_to_address(101, &address)?;
        Ok(())
    }

    /// Return the hash of the most recent block.
    pub fn get_best_block_hash(&self) -> Result<String, corepc_node::Error> {
        let client = &self.bitcoind.client;
        let block_hash = client.get_best_block_hash()?.0;
        Ok(block_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::{DifficultyLevel, TemplateProvider};
    use crate::utils::get_available_address;

    #[tokio::test]
    async fn test_create_mempool_transaction() {
        let address = get_available_address();
        let port = address.port();
        let tp = TemplateProvider::start(port, 1, DifficultyLevel::Low);
        assert!(tp.fund_wallet().is_ok());
        assert!(tp.create_mempool_transaction().is_ok());
    }
}
