# JD Server
The JD Server is a Sv2 proxy, It is also the entity responsible for Pool-side block propagation in case of valid blocks found by miners connected to the pool (who are using the Job Declaration Protocol).

## Setup
### Configuration File
The `jds-config-local-example.toml` is a configuration example.

The configuration file contains the following information:

1. The SRI Pool information which includes the SRI Pool authority public key 
   (`authority_public_key`), the SRI Pool authority secret key (`authority_secret_key`), along with its certificate validity (`cert_validity_sec`). In addition to this, it contains the address which it will use to listen to new connection from downstream roles (`listen_address`) and the list of uncompressed pubkeys for coinbase payout (`coinbase_outputs`).
2. The SRI Pool Job Negatiator information which includes the Template Provider address (`tp_address`) and the address it uses to listen new request from the downstream JDs (`jd_address`).
3. Optionally, you may want to verify that your TP connection is authentic. You may get `tp_authority_public_key` from the logs of your TP, for example:
```
# 2024-02-13T14:59:24Z Template Provider authority key: 9azQdassggC7L3YMVcZyRJmK7qrFDj5MZNHb4LkaUrJRUhct92W
```

### Run

Run the Job Declarator Server (JDS):

   ```
   cd roles/jd-server/config-examples
   ```
   ```
  cargo run -- -c jds-config-local-example.toml
   ```
