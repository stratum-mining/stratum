# CPU Sv2 mining device

Header only sv2 cpu miner.

```
Usage: mining_device [OPTIONS] --address-pool <ADDRESS_POOL>

Options:
  -p, --pubkey-pool <PUBKEY_POOL>
          Pool pub key, when left empty the pool certificate is not checked
  -i, --id-device <ID_DEVICE>
          Sometimes used by the pool to identify the device
  -a, --address-pool <ADDRESS_POOL>
          Address of the pool in this format ip:port or domain:port
      --handicap <HANDICAP>
          This value is used to slow down the cpu miner, it represents the number of micro-seconds that are awaited between hashes [default: 0]
      --id-user <ID_USER>
          User id, used when a new channel is opened, it can be used by the pool to identify the miner
      --nominal-hashrate-multiplier <NOMINAL_HASHRATE_MULTIPLIER>
          This floating point number is used to modify the advertised nominal hashrate when opening a channel with the upstream.
          If 0.0 < nominal_hashrate_multiplier < 1.0, the CPU miner will advertise a nominal hashrate that is smaller than its real capacity.
          If nominal_hashrate_multiplier > 1.0, the CPU miner will advertise a nominal hashrate that is bigger than its real capacity.
          If empty, the CPU miner will simply advertise its real capacity.
  -h, --help
          Print help
  -V, --version
          Print version
```

Usage example:
```
cargo run --release -- --address-pool 127.0.0.1:20000 --id-device device_id::SOLO::bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh
```

## handicap

CPU mining could damage the system due to excessive heat.

The `--handicap` parameter should be used as a safety mechanism to slow down the hashrate in order to preserve hardware.

## nominal hashrate multiplier

Let's imagine that:
- the upstream wants to receive shares every ~100s (on average)
- the CPU miner nominal hashrate is 1k H/s

Maybe we want to do a test where we don't want to wait ~100s before a share is submitted by the CPU miner.

In that case, we need the CPU miner to advertise a smaller hashrate, which will force the upstream to set a lower
difficulty target.

The `--nominal-hashrate-multiplier` can be used to advertise a custom nominal hashrate.

In the scenario described above, we could launch the CPU miner with `--nominal-hashrate-multiplier 0.01`. 

The CPU miner would advertise 0.01k H/s, which would cause the upstream to set the difficulty target such that the CPU miner would find a share within ~1s.

This feature can also be used to advertise a bigger nominal hashrate by using values above `1.0`.

That can also be useful for testing difficulty adjustment algorithms on Sv2 upstreams.