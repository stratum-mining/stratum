# SRI Pool
SRI Pool is designed to communicate with Downstream role (most typically a Translator Proxy or a Mining Proxy) running SV2 protocol to exploit features introduced by its sub-protocols.

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
The `pool-config-example.toml` is is a configuration example which can be modified by the party that is running the SV2 Pool (most
typically the pool service provider) to address the most preferred customization.

The configuration file contains the following information:

1. The SRI Pool information which includes the SRI Pool authority public key 
   (`authority_pubkey`), the SRI Pool authority secret key (`authority_secret_key`), along with its certificate validity (`cert_validity_sec`). In addiction to this, it contains also the address which it use to listen new connections from downstream roles (`listen_address`) and the list of uncompressed pubkeys for coinbase payout (`coinbase_outputs`).
2. The SRI Pool Job Negatiator information which includes the Template Provider address (`tp_address`) and the address it uses to listen new request from the downstream JNs (`jn_address`).

### Run
1. Copy the `pool-config-example.toml` into `conf/` directory.
2. Edit it with custom desired configuration and rename it `pool-config.toml`
3. Run the Pool:

```
cd roles/v2/pool
```
```
cargo run -p pool_sv2 -- -c conf/pool-config.toml
```