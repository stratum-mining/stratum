//! Configurable Sv2 it support extended and group channel
//! Upstream means another proxy or a pool
//! Downstream means another proxy or a mining device
//!
//! UpstreamMining is the trait that a proxy must implement in order to
//! understand Downstream mining messages.
//!
//! DownstreamMining is the trait that a proxy must implement in order to
//! understand Upstream mining messages
//!
//! Same thing for DownstreamCommon and UpstreamCommon but for common messages
//!
//! DownstreamMiningNode implement both UpstreamMining and UpstreamCommon
//!
//! UpstreamMiningNode implement both DownstreamMining and DownstreamCommon
//!
//! A Downstream that signal the capacity to handle group channels can open more than one channel.
//! A Downstream that signal the incapacity to handle group channels can open only one channel.
use tracing::error;

use mining_proxy_sv2::start_mining_proxy;

mod args;
use args::process_cli_args;

/// 1. the proxy scan all the upstreams and map them
/// 2. downstream open a connection with proxy
/// 3. downstream send SetupConnection
/// 4. a mining_channels::Upstream is created
/// 5. upstream_mining::UpstreamMiningNodes is used to pair this downstream with the most suitable
///    upstream
/// 6. mining_channels::Upstream create a new downstream_mining::DownstreamMiningNode embedding
///    itself in it
/// 7. normal operation between the paired downstream_mining::DownstreamMiningNode and
///    upstream_mining::UpstreamMiningNode begin
#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let config = match process_cli_args() {
        Ok(c) => c,
        Err(e) => {
            error!("Mining Proxy Config error: {}", e);
            return;
        }
    };
    start_mining_proxy(config).await;
}
