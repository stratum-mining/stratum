# CPU Sv2 mining device

Header only sv2 cpu miner.

```
Usage: mining-device [OPTIONS] --address-pool <ADDRESS_POOL>

Options:
  -p, --pubkey-pool <PUBKEY_POOL>    Pool pub key, when left empty the pool certificate is not checked
  -i, --id-device <ID_DEVICE>        Sometimes used by the pool to identify the device
  -a, --address-pool <ADDRESS_POOL>  Address of the pool in this format ip:port or domain:port
      --handicap <HANDICAP>          This value is used to slow down the cpu miner, it represents the number of micro-seconds that are awaited between hashes [default: 0]
      --id-user <ID_USER>            User id, used when a new channel is opened, it can be used by the pool to identify the miner
  -h, --help                         Print help
  -V, --version                      Print version
```

## Example
1. Start a pool. The hosted example in the [`pool`](https://github.com/stratum-mining/stratum/tree/main/roles/pool)
   crate can be started with:

```sh
cd roles/pool
cargo run -- -c config-examples/pool-config-hosted-tp-example.toml
```

2. Start the mining-device, making sure the port is the same as the `pool`'s `listen_address` port.

```sh
cd roles/test-util/mining-device
cargo run -- --address-pool 127.0.0.1:34254 --id-device device_id::SOLO::bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh
```
