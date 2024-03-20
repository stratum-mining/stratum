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
The `proxy-config-example.toml` is a configuration example which can be copy/paste into `/conf` directory by the party that is running the Translator Proxy (most
typically the mining farm/miner hobbyist) to address the most preferred customization.

The configuration file contains the following information:

1. The SV2 Upstream connection information which includes the SV2 Pool authority public key 
   (`upstream_authority_pubkey`) and the SV2 Pool connection address (`upstream_address`) and port
   (`upstream_port`).
1. The SV1 Downstream connection information which includes the SV1 Mining Device address
   (`downstream_address`) and port (`downstream_port`).
1. The maximum and minimum SV2 versions (`max_supported_version` and `min_supported_version`) that
   the Translator Proxy implementer wants to support. Currently the only available version is `2`.
1. The desired minimum `extranonce2` size that the Translator Proxy implementer wants to use
   (`min_extranonce2_size`). The `extranonce2` size is ultimately decided by the SV2 Upstream role,
   but if the specified size meets the SV2 Upstream role's requirements, the size specified in this
   configuration file should be favored.
1. The Job Declarator information which includes the Pool JD connection address (`jd_address`) and the Template Provider connection address to which to connect (`tp_address`).
1. The difficulty params such as the hashrate (hashes/s) of the weakest Mining Device that will be connecting to the Translator Proxy (`min_individual_miner_hashrate`), the number of shares needed before a mining.set_difficulty update (`miner_num_submits_before_update`) and the number of shares per minute that Mining Devices should be sending to the Translator Proxy (`shares_per_minute`). Ultimately, the estimated aggregate hashrate of all SV1 Downstream roles (Mining
   Devices) (`channel_nominal_hashrate`), which is communicated to the SV2 Upstream to help it decide a proper difficulty target.

### Run
1. Copy the `proxy-config-example.toml` into `conf/` directory.
2. Edit it with custom desired configuration and rename it `proxy-config.toml`
3. Point the SV1 Downstream Mining Device(s) to the Translator Proxy IP address and port.
4. Run the Translator Proxy:

   ```
   cd roles/translator
   ```
   ```
   cargo run -p translator_sv2 -- -c conf/proxy-config.toml
   ```
