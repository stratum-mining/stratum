# How to run the Demo

### Clone and build core

1. clone https://github.com/ccdle12/bitcoin/ and checkout `2022.09.08-add-sv2-template-provider`
2. do `./autogen.sh && ./configure --enable-template-provider`
3. do `make`

### Start and initialize bictoind
4. start bitcoind with `./src/bitcoind -regtest`
5. create **at least 16 blocks** with `./src/bitcoin-cli -regtest generatetoaddress 16 bcrt1qttuwhmpa7a0ls5kr3ye6pjc24ng685jvdrksxx`

### Start pool proxy and mining-device
6. go on the root of this repo `cd stratum`
7. start the pool with `cargo run -p pool`
8. start the proxy with `cd ./roles/v2/mining-proxy` then `cargo run`
9. start the mining-device with `cargo run -p mining-device`

