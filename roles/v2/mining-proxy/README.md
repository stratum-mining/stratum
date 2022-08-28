# mining-proxy

## Run

### Test miner <-> proxy <-> pool stack

Terminal 1:
```
% cd examples/sv2-proxy
% cargo run --bin pool
```

Terminal 2:
Run mining proxy:

```bash
# For help run `cargo run -- --help`
cargo run
```

Terminal 3:
```
% cd examples/sv2-proxy
% cargo run --bin mining-device
```
