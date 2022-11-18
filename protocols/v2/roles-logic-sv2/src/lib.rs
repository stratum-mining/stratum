//! It exports traits that when implemented make the implementor a valid Sv2 role:
//!
//!```txt
//! MiningDevice:
//!     common_properties::IsUpstream +
//!     common_properties::IsMiningUpstream +
//!     handlers::common::ParseUpstreamCommonMessages +
//!     handlers::mining::ParseUpstreamMiningMessages +
//!
//! Pool:
//!     common_properties::IsDownstream +
//!     common_properties::IsMiningDownstream +
//!     handlers::common::ParseDownstreamCommonMessages +
//!     handlers::mining::ParseDownstreamMiningMessages +
//!
//! ProxyDownstreamConnetion:
//!     common_properties::IsDownstream +
//!     common_properties::IsMiningDownstream +
//!     handlers::common::ParseDownstreamCommonMessages +
//!     handlers::mining::ParseDownstreamMiningMessages +
//!
//! ProxyUpstreamConnetion:
//!     common_properties::IsUpstream +
//!     common_properties::IsMiningUpstream +
//!     handlers::common::ParseUpstreamCommonMessages +
//!     handlers::mining::ParseUpstreamMiningMessages +
//! ```
//!
//! In parser there is anything needed for serialize and deserialize messages.
//! Handlers export the main traits needed in order to implement a valid Sv2 role.
//! Routers in routing_logic are used by the traits in handlers for decide to which
//! downstream/upstrem realy/send they use selectors in order to do that.
pub mod common_properties;
pub mod errors;
pub mod group_channel_logic;
pub mod handlers;
pub mod job_creator;
pub mod job_dispatcher;
pub mod parsers;
pub mod routing_logic;
pub mod selectors;
pub mod utils;
pub use bitcoin;
pub use common_messages_sv2;
pub use errors::Error;
pub use job_negotiation_sv2;
pub use mining_sv2;
pub use template_distribution_sv2;



#[cfg(test)]
pub mod test_utils {
    use super::template_distribution_sv2::NewTemplate;
    use std::{convert::TryInto};
    use quickcheck::{Arbitrary, Gen};
    pub use bitcoin::{
        Network,
        secp256k1::SecretKey,
        secp256k1::Secp256k1,
        util::ecdsa::{PrivateKey, PublicKey},
    };
    use std::{cmp, vec};

    const PRIVATE_KEY_BTC: [u8; 32] = [34; 32];
    const NETWORK: Network = Network::Testnet;

    const BLOCK_REWARD: u64 = 625_000_000_000;

    pub fn new_pub_key() -> PublicKey {
        let priv_k = PrivateKey::from_slice(&PRIVATE_KEY_BTC, NETWORK).unwrap();
        let secp = Secp256k1::default();
        PublicKey::from_private_key(&secp, &priv_k)
    }

    pub fn template_from_gen(g: &mut Gen, id: u64) -> NewTemplate<'static> {
        let mut coinbase_prefix_gen = Gen::new(255);
        let mut coinbase_prefix: vec::Vec<u8> = vec::Vec::new();

        let max_num_for_script_prefix = 253;
        coinbase_prefix.resize_with(255, || {
            cmp::min(
                u8::arbitrary(&mut coinbase_prefix_gen),
                max_num_for_script_prefix,
            )
        });
        let coinbase_prefix: binary_sv2::B0255 = coinbase_prefix.try_into().unwrap();

        let mut coinbase_tx_outputs_gen = Gen::new(32);
        let mut coinbase_tx_outputs_inner: vec::Vec<u8> = vec::Vec::new();
        coinbase_tx_outputs_inner.resize_with(32, || u8::arbitrary(&mut coinbase_tx_outputs_gen));
        let coinbase_tx_outputs_inner: binary_sv2::B064K =
            coinbase_tx_outputs_inner.try_into().unwrap();
        let coinbase_tx_outputs: binary_sv2::Seq064K<binary_sv2::B064K> =
            vec![coinbase_tx_outputs_inner].into();

        let mut merkle_path_inner_gen = Gen::new(32);
        let mut merkle_path_inner: vec::Vec<u8> = vec::Vec::new();
        merkle_path_inner.resize_with(32, || u8::arbitrary(&mut merkle_path_inner_gen));
        let merkle_path_inner: binary_sv2::U256 = merkle_path_inner.try_into().unwrap();
        let merkle_path: binary_sv2::Seq0255<binary_sv2::U256> = vec![merkle_path_inner].into();

        NewTemplate {
            template_id: id,
            future_template: bool::arbitrary(g),
            version: u32::arbitrary(g),
            coinbase_tx_version: 2,
            coinbase_prefix,
            coinbase_tx_input_sequence: u32::arbitrary(g),
            coinbase_tx_value_remaining: u64::arbitrary(g),
            coinbase_tx_outputs_count: 0,
            coinbase_tx_outputs,
            coinbase_tx_locktime: u32::arbitrary(g),
            merkle_path,
        }
    }
}
