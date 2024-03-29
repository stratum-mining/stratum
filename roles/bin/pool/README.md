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
The `pool-config-example.toml` is a configuration example which can be copy/paste into `/conf` directory by the party that is running the SV2 Pool (most
typically the pool service provider) to address the most preferred customization.

The configuration file contains the following information:

1. The SRI Pool information which includes the SRI Pool authority public key 
   (`authority_pubkey`), the SRI Pool authority secret key (`authority_secret_key`), along with its certificate validity (`cert_validity_sec`). In addition to this, it contains the address which it will use to listen to new connection from downstream roles (`listen_address`) and the list of uncompressed pubkeys for coinbase payout (`coinbase_outputs`).
2. The SRI Pool Job Negatiator information which includes the Template Provider address (`tp_address`) and the address it uses to listen new request from the downstream JDs (`jd_address`).
3. Optionally, you may want to verify that your TP connection is authentic. You may get `tp_authority_public_key` from the logs of your TP, for example:
```
# 2024-02-13T14:59:24Z Template Provider authority key: EguTM8URcZDQVeEBsM4B5vg9weqEUnufA8pm85fG4bZd
```

### Run
1. Copy the `pool-config-example.toml` into `conf/` directory.
2. Edit it with custom desired configuration and rename it `pool-config.toml`
   > <ins>**Warning**</ins><br>
   > If you want to mine spendable bitcoin on regtest, you can do it with bitcoin-cli:
   > 1. Get a legacy Bitcoin address:
   >   ```
   >   bitcoin-cli -regtest -rpcwallet="<PUT YOUR WALLET NAME HERE>" getnewaddress "test" "legacy"
   >   ```
   > 2. Retrieve its corresponding public key:
   >   ```
   >   bitcoin-cli -regtest getaddressinfo <PUT THE ADDRESS GENERATED HERE>
   >   ```
   > 3. Copy the pubkey showed in the output
   > 4. Paste it in the `coinbase_outputs` of `pool-config.toml`, after deleting the one which is already present
   > 5. Mine a block
   > 6. Generate 100 blocks 
   >   ```
   >   bitcoin-cli -regtest generatetoaddress 100 bcrt1qc5xss0cma0zldxfzzdpjxsayut7yy86e2lr6km
   >   ```
   > Now the mined bitcoin are spendable!
   
3. Run the Pool:

   ```
   cd roles/v2/pool
   ```
   ```
   cargo run -p pool_sv2 -- -c conf/pool-config.toml
   ```