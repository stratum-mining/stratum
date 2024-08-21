
# SV1 to SV2 Translator Proxy

This proxy is designed to sit in between a SV1 Downstream role (most typically Mining Device(s) 
running SV1 firmware) and a SV2 Upstream role (most typically a SV2 Pool Server with Extended
Channel support).

The most typical high level configuration is:

```
<--- Most Downstream ----------------------------------------- Most Upstream --->

+---------------------------------------------------+  +------------------------+
|                     Mining Farm                   |  |      Remote Pool       |
|                                                   |  |                        |
|  +-------------------+     +------------------+   |  |   +-----------------+  |
|  | SV1 Mining Device | <-> | Translator Proxy | <------> | SV2 Pool Server |  |
|  +-------------------+     +------------------+   |  |   +-----------------+  |
|                                                   |  |                        |
+---------------------------------------------------+  +------------------------+

```

## Setup

### Configuration File

`tproxy-config-local-jdc-example.toml` and `tproxy-config-local-pool-example.toml` are examples of configuration files for the Translator Proxy.

The configuration file contains the following information:

1. The SV2 Upstream connection information which includes the SV2 Pool authority public key 
   (`upstream_authority_pubkey`) and the SV2 Pool connection address (`upstream_address`) and port
   (`upstream_port`).
2. The SV1 Downstream socket information which includes the listening IP address
   (`downstream_address`) and port (`downstream_port`).
3. The maximum and minimum SRI versions (`max_supported_version` and `min_supported_version`) that
   the Translator Proxy implementer wants to support. Currently the only available version is `2`.
4. The desired minimum `extranonce2` size that the Translator Proxy implementer wants to use
   (`min_extranonce2_size`). The `extranonce2` size is ultimately decided by the SV2 Upstream role,
   but if the specified size meets the SV2 Upstream role's requirements, the size specified in this
   configuration file should be favored.
5. The downstream difficulty params such as:
- the hashrate (hashes/s) of the weakest Mining Device that will be connecting to the Translator Proxy (`min_individual_miner_hashrate`)
- the number of shares per minute that Mining Devices should be sending to the Translator Proxy (`shares_per_minute`). 
6. The upstream difficulty params such as:
- the interval in seconds to elapse before updating channel hashrate with the pool (`channel_diff_update_interval`)
- the estimated aggregate hashrate of all SV1 Downstream roles (`channel_nominal_hashrate`)

### Run

There are two files in `roles/translator/config-examples`:
- `tproxy-config-local-jdc-example.toml` which assumes the Job Declaration protocol is used and a JD Client is deployed locally
- `tproxy-config-local-pool-example.toml` which assumes Job Declaration protocol is NOT used, and a Pool is deployed locally

```bash
cd roles/translator/config-examples/
cargo run -- -c tproxy-config-local-jdc-example.toml