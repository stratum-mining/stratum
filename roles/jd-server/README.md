# JD Server

The JD Server is a Sv2 proxy, It is also the entity responsible for Pool-side block propagation in case of valid blocks found by miners connected to the pool (who are using the Job Declaration Protocol).

## Setup

### Configuration File

The configuration file contains the following information:

1. The SRI Pool information which includes the SRI Pool authority public key (`authority_public_key`), the SRI Pool authority secret key (`authority_secret_key`). 
2. The list of uncompressed pubkeys for coinbase payout (`coinbase_outputs`)
3. The address which it will use to listen to new connection from downstream roles (`listen_jd_address`)
2. The RPC config for mempool config where are (`core_rpc_url`, `core_rpc_port`, `core_rpc_user`, `core_rpc_pass`), and the time interval for the JDS mempool update in form of (`unit`,`value`)

### Run

There are two files when you cd into roles/jd-server/config-examples/

1. jds-config-hosted-example.toml is community hosted server
2. jds-config-local-example.toml serves as Self-hosted

Run the Job Declarator Server (JDS):

```bash
   cd roles/jd-server/config-examples
  cargo run -- -c jds-config-hosted-example.toml
```

