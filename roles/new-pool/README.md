
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

`pool-config-hosted-tp-example.toml` and `pool-config-local-tp-example.toml` are examples of configuration files.

The configuration file contains the following information:

1. The SRI Pool information which includes the SRI Pool authority public key
   (`authority_public_key`), the SRI Pool authority secret key (`authority_secret_key`).
2. The address which it will use to listen to new connection from downstream roles (`listen_address`)
3. The list of uncompressed pubkeys for coinbase payout (`coinbase_outputs`)
4. A string that serves as signature on the coinbase tx (`pool_signature`).
5. The Template Provider address (`tp_address`).
6. Optionally, you may want to verify that your TP connection is authentic. You may get `tp_authority_public_key` from the logs of your TP, for example:

```
# 2024-02-13T14:59:24Z Template Provider authority key: EguTM8URcZDQVeEBsM4B5vg9weqEUnufA8pm85fG4bZd
```

### Run

There are two files found in `roles/pool/config-examples`

1. `pool-config-hosted-tp-example.toml` runs on our community hosted server.
2. `pool-config-example-tp-example.toml` runs with your local config.

Run the Pool:

```bash
cd roles/pool/config-examples
cargo run -- -c pool-config-hosted-tp-example.toml
``` 
