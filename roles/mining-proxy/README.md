# mining-proxy

## Run

## proxy-config.toml file

When spawned the proxy will look in the current working directory (linux) for a
`proxy-config.config` if the file is not available the proxy will panic. We can specify a different
path for the config file with the `-c` option.

The config need to be a valid toml file with the below values:
1. upstreams: vector of upstreams (likely pools). An upstream is composed by:
  1. channel_kind: can be either `Group`, `Extended`, `ExtendedWithDeclarator`.
    * __Group__: Proxy do not open an extended channel with upstream but just relay request to
        open standard channel from downstream to upstream, being the proxy non HOM the channels are
        grouped.
    * __Extended__: Proxy open an extended channel with upstream. When downstream ask to open
        standard channels it just use the open extended channel with upstream to itself open
        standard channels downstream.
    * __ExtendedWithDeclarator__: Like `Extended` but do not relay on the pool to create new job. It
        just connect to a TP and communicate to the pool which is the job that it want to work with.
  2. adress: ip address of the upstream
  3. port: upstream's port
  4. pub_key: is the public key that upstream will use to sign the upstream cert needed for the
     noise handshake.
  5. jd_values: optional value only needed when `channel_kind` is `ExtendedWithDeclarator` is
     composed by:
       1. address: ip of the JD that we want to use with this upstream
       2. port: port of the JD that we want to use with this upstream
       3. pub_key: pub_key of the JD that we want to use with this upstream
2. tp_address: optional value only needed when at least one `upstream` in `upstreams` has the kind
   `ExtendedWithDeclarator`. Is the address in the form `[ip:port]` of the TP.
3. listen_address: the address at which the `mining-proxy` will accept downstream connection.
4. listen_mining_port: the port at which the `mining-proxy` will accept downstream connection.
5. max_supported_version: the `mining-proxy` will not connect to upstream the are using an Sv2
   version higher that the one specified here (default to 2)
6. min_supported_version: the `mining-proxy` will not connect to upstream the are using an Sv2
   version smaller that the one specified here (default to 2)
7. downstream_share_per_minute: how many share per minute downstream is supposed to produce. The
   `mining-proxy` will use this value and the expected downstream hash rate (communicate vie 
   `penStandardMiningChannel` to calculate the right downstream target.

### Test miner <-> proxy <-> pool stack

Terminal 1:
```
% cd examples/sv2-proxy
% cargo run --bin pool
```

Terminal 2:
Run mining proxy:

```
% # For help run `cargo run -- --help`
% cd roles/v2/mining-proxy
% cargo run
```

Terminal 3:
```
% cd examples/sv2-proxy
% cargo run --bin mining-device
```
