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
      --nonces-per-call <NONCES_PER_CALL>
          Number of nonces to try per mining loop iteration when fast hashing is available (micro-batching). [default: 32]
      --cores <CORES>
          Number of worker threads to use for mining. Defaults to logical CPUs minus one (leaves one core free).
  -h, --help
          Print help
  -V, --version
          Print version
```

Usage example:
```
cargo run --release -- --address-pool 127.0.0.1:20000 --id-device device_id::SOLO::bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh
```

To adjust micro-batching (see below), you can pass for example `--nonces-per-call 64`:

```
cargo run --release -- --address-pool 127.0.0.1:20000 \
        --id-device device_id::SOLO::bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh \
        --nonces-per-call 64
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

## Micro-batching (nonces per call)

The miner supports hashing multiple consecutive nonces per loop iteration when the fast hashing path is available. This reduces outer-loop overhead and can slightly increase throughput on some CPUs.

- Flag: `--nonces-per-call <N>`
- Default: `32`
- Trade-off: larger batches can increase latency to detecting a found share because the loop advances in steps of `N`. Choose smaller values (e.g., `4`–`16`) if you care more about latency; larger values (e.g., `32`–`128`) may squeeze a bit more throughput.

This setting only affects the CPU loop structure; it does not change the hash function or correctness.

## Worker threads

By default, the miner uses one worker thread per logical CPU minus one (N-1). This leaves a core available for the operating system and scheduling overhead.

You can override this with `--cores <N>`, clamped between `1` and the number of logical CPUs.

Examples:

```zsh
# Pin to a small fixed number of workers
cargo run --release -- --address-pool 127.0.0.1:20000 --cores 2
```

If `--cores` is omitted, auto mode (N-1) is used.

## Benchmarks

You can measure performance with Criterion. From this directory:

```zsh
cargo bench --bench hasher_bench -- --quiet
```

- `hasher_bench` compares baseline `block_hash()` against the optimized midstate+compress256 path.

To analyze the effect of micro-batching on an end-of-loop iteration, run:

```zsh
cargo bench --bench microbatch_bench -- --quiet
```

- `microbatch_bench` sweeps several batch sizes and sets Criterion throughput to `Elements = N` where each element is one nonce. This means:
        - The reported time per iteration divides roughly by `N` to get per-nonce time.
        - Criterion also prints throughput in elements/s (hashes/s). For convenience, the bench additionally prints a concise `MH/s` per configuration.

By default the bench runs a concise subset of batch sizes: `1,8,32,128`. You can override the list via an environment variable:

```zsh
MINING_DEVICE_BATCH_SIZES=1,4,8,16,32,64,128 cargo bench --bench microbatch_bench -- --quiet
```

Tip: pick the smallest `N` that gives you near-peak throughput to keep share-finding latency low.

### Total scaling (multi-core)

Total throughput doesn’t always scale linearly with more workers (due to CPU topology, turbo, thermal limits, etc.). Use the scaling bench to measure aggregate MH/s while ramping worker counts from 1 up to your number of logical CPUs:

```zsh
cargo bench --bench scaling_bench -- --quiet
```

- The bench automatically detects the number of logical CPUs and iterates workers from `1..=N` (no environment variable needed).

The bench prints one concise summary line per configuration and shows incremental improvements versus the previous worker count, including the approximate MH/s gained per additional worker. It also sets Criterion throughput to Elements equal to total nonces hashed, so Elements/s equals total hashes/s.
