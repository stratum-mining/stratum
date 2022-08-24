# Stratum

[![codecov](https://codecov.io/gh/stratum-mining/stratum/branch/main/graph/badge.svg)](https://codecov.io/gh/stratum-mining/stratum)

The Stratum protocol defines how miners, proxies, and pools communicate to contribute hashrate to
the Bitcoin network.

### Test Coverage
Command to generate test coverage percentage:

```
cargo install cargo-tarpaulin
cargo +nightly tarpaulin --verbose --features prop_test noise_sv2 fuzz with_buffer_pool async_std debug tokio with_tokio derive_codec_sv2 binary_codec_sv2 default core --lib --exclude-files examples/* --timeout 120 --fail-under 20 --out Xml
```

Must have [`cargo-tarpaulin`](https://github.com/xd009642/tarpaulin) installed globally:
```
cargo install cargo-tarpaulin
```

## Table of Contents

- [Stratum](#stratum)
  - [1. Goals](#1-goals)
  - [2. Common Use Cases](#2-common-use-cases)
    - [2.1 Miners](#21-miners)
    - [2.2 Pools](#22-pools)
  - [3. Structure](#3-structure)
    - [3.01 protocols](#301-protocols)
    - [3.02 protocols/v1](#302-protocolsv1)
    - [3.03 protocols/v2](#303-protocolsv2)
    - [3.04 protocols/v2/const-sv2](#304-protocolsv2const-sv2)
    - [3.05 protocols/v2/binary-sv2](#305-protocolsv2binary-sv2)
    - [3.06 protocols/v2/framing-sv2](#306-protocolsv2framing-sv2)
    - [3.07 protocols/v2/codec-sv2](#307-protocolsv2codec-sv2)
    - [3.08 protocols/v2/subprotocols](#308-protocolsv2subprotocols)
    - [3.09 protocols/v2/sv2-ffi](#309-protocolsv2sv2-ffi)
    - [3.10 protocols/v2/noise-sv2](#310-protocolsv2noise-sv2)
    - [3.11 protocols/v2/roles-logic-sv2](#311-protocolsv2roles-logic-sv2)
    - [3.12 utils/buffer](#312-utilsbuffer)
    - [3.13 utils/network-helpers](#313-utilsnetwork-helpers)
    - [3.14 roles/mining-proxy](#314-rolesmining-proxy)
    - [3.15 roles/pool](#315-rolespool)
    - [3.16 roles/test-utils/pool](#316-rolestest-utilspool)
    - [3.17 roles/test-utils/mining-device](#317-rolestest-utilsmining-device)
    - [3.18 examples](#318-examples)
    - [3.19 examples/interop-cpp](#319-examplesinterop-cpp)
    - [3.20 examples/interop-cpp-no-cargo](#320-examplesinterop-cpp-no-cargo)
    - [3.21 examples/ping-pong-with-noise](#321-examplesping-pong-with-noise)
    - [3.22 examples/ping-pong-without-noise](#322-examplesping-pong-without-noise)
    - [3.23 examples/sv1-client-and-server](#323-examplessv1-client-and-server)
    - [3.24 examples/sv2-proxy](#324-examplessv2-proxy)
    - [3.25 examples/template-provider-test](#325-examplestemplate-provider-test)
    - [3.26 test](#326-test)
    - [3.27 experimental](#327-experimental)
  - [4. Build/Run/Test](#4-build)
  - [5. Branches](#5-branches)


## 1. Goals

The goals of this project are to provide:

1. A robust set of Stratum V2 (Sv2) primitives as Rust library crates which anyone can use
   to expand the protocol or implement a role. For example:

   - Pools supporting Sv2
   - Mining-device/hashrate producers integrating Sv2 into their firmware
   - Bitcoin nodes implementing Template Provider to build the `blocktemplate`

2. The above Rust primitives as a C library available for use in other languages via FFI.

3. A set of helpers built on top of the above primitives and the external Bitcoin-related Rust
   crates for anyone to implement the Sv2 roles.

4. An open-source implementation of a Sv2 proxy for miners.

5. An open-source implementation of a Sv2 pool for mining pool operators.

## 2. Common Use Cases

The library is modular to address different use-cases and desired functionality. Examples include:

## 2.1 Miners

1. Sv1 Miners can use the proxy (`roles/sv2/mining-proxy`) to connect with a Sv2-compatible pool.

2. Sv1 mining farms mining to a Sv2-compatible pool gain some of the security and efficiency
   improvements Sv2 offers over Stratum V1 (Sv1). The Sv1<->Sv2 miner proxy does not support
   _all_ the features of Sv2, but works as a temporary measure before upgrading completely
   to Sv2-compatible firmware. (The Sv1<->Sv2 translation proxy implementation is a work in progress.)

## 2.2 Pools

1. Pools supporting Sv2 can deploy the open source binary crate (`roles/pool`) to offer
   their clients (miners participating in said pool) an Sv2-compatible pool.

1. The Rust helper library provides a suite of tools for mining pools to build custom Sv2 compatible
   pool implementations.

1. The C library provides a set of FFI bindings to the Rust helper library for miners to integrate
   Sv2 into their existing firmware stack.

## 3. Structure

```
stratum
  └─ examples
  │   └─ interop-cpp
  │   └─ interop-cpp-no-cargo
  │   └─ ping-pong-with-noise
  │   └─ ping-pong-without-noise
  │   └─ sv1-client-and-server
  │   └─ sv2-proxy
  │   └─ template-provider-test
  └─ experimental
  │   └─ coinbase-negotiator
  └─ protocols
  │   └─ v1
  │   └─ v2
  │       └─ binary-sv2
  │       │   └─ binary-sv2
  │       │   └─ no-serde-sv2
  │       │   │   └─ codec
  │       │   │   └─ derive_codec
  │       │   └─ serde-sv2
  │       └─ codec-sv2
  │       └─ const-sv2
  │       └─ framing-sv2
  │       └─ noise-sv2
  │       └─ roles-logic-sv2
  │       └─ subprotocols
  │       │   └─ common-messages
  │       │   └─ job-negotiation
  │       │   └─ mining
  │       │   └─ template-distribution
  │       └─ sv2-ffi
  └─ roles/v2
  │   └─ mining-proxy
  │   └─ pool
  │   └─ test-utils
  │   │   └─ mining-device
  │   │   └─ pool
  └─ test
  └─ utils
      └─ buffer
      └─ network-helpers
```

This workspace's 6 macro-areas:

1. `protocols`: Stratum V2 and V1 libraries.
2. `roles`: The Sv2 roles logic.
3. `utils`: Offers a more efficient alternative buffer. Less safe than the default buffers used.
   Includes network helpers.
4. `examples`: Several example implementations of various use cases.
5. `test`: Integration tests.
6. `experimental`: Experimental logic not yet specified as part of the protocol or
   extensions.

All dependencies related to the testing and benchmarking of these workspace crates are optional
and are included in the binary ONLY during testing and benchmarking. For that reason,
these dependencies are NOT included under **External dependencies** in the lists below.

### 3.01 protocols

Core Stratum V2 and V1 libraries.

### 3.02 protocols/v1

Contains a Sv1 library.
TODO: more information

### 3.03 protocols/v2

Contains several Sv2 libraries serving various purposes depending on the desired application.
TODO: more info

### 3.04 protocols/v2/const-sv2

Contains the Sv2 related constants.

### 3.05 protocols/v2/binary-sv2

`binary-sv2` contains five crates (`binary-sv2`, `serde-sv2`, `no-serde-sv2`,
`no-serde-sv2/codec`, `no-serde-sv2/derive_codec`) of Sv2 data types & binary mappings.
The user only uses `procotols/v2/binary-sv2/binary-sv2`. Other crates are for
internal use only.

Exports an API for serialization and deserialization of Sv2 primitive data types.
Also exports two procedural macros, `Encodable` and `Decodable`, that, when applied to a
struct, make it serializable/deserializable to/from the Sv2 binary format.

This crate can be compiled with the `with_serde` feature, which will use the external `serde` crate
to serialize and deserialize. Uses an internal serialization engine if `with_serde` feature flag NOT set.
The exported API is the same.

**External dependencies**:

- [`serde`](https://crates.io/crates/serde) (only when compiled with the `with_serde` flag)

**Internal dependencies**:

- `buffer-sv2` in `protocols/v2/serde-sv2` (only when compiled with the `with_serde` flag)
- `no-serde-sv2` (only when compiled WITHOUT the `with_serde` flag)

### 3.06 protocols/v2/framing-sv2

Exports the `Frame` trait. A `Frame` can:

- be serialized (`serialize`) and deserialized (`from_bytes`, `from_bytes_unchecked`)
- return the payload (`payload`)
- return the header (`get_header`)
- be constructed from a serializable payload (when the payload does not exceed the maximum frame
  size (`from_message`))

Exports two implementations of the `Frame` trait: `Sv2Frame` and `NoiseFrame`, and the
`EitherFrame` enum for matching the appropriate trait implementation.

**External dependencies**:

- [`serde`](https://crates.io/crates/serde) (only when compiled with the `with_serde` flag)

**Internal dependencies**:

- `const_sv2`
- `binary_sv2`

### 3.07 protocols/v2/codec-sv2

Exports `StandardNoiseDecoder` and `StandardSv2Decoder`, both initialized with a buffer
containing a "stream" of bytes. On calling `next_frame`, they return either a `Sv2Frame` or an
error containing the number of missing bytes to allow next step[^1]. The caller can fill the
buffer with the missing bytes before calling again.

Also exports `NoiseEncoder` and `Encoder`.

[^1] For a non-noise decoder, this refers to the missing bytes needed to

1. complete the frame header, or
2. have a complete frame.

For a noise decoder, it returns a `Sv2Frame`, which can be composed of several `NoiseFrames`.

**External dependencies**:

- [`serde`](https://crates.io/crates/serde) (only when compiled with the `with_serde` flag)

**Internal dependencies**:

- `const_sv2`
- `binary_sv2`
- `framing_sv2`
- `noise_sv2` (only when compiled with the `with_noise` flag)

### 3.08 protocols/v2/subprotocols

`subprotocols` has four crates (`common-messages`, `job-negotiation`, `mining`, and
`template-distribution`) which are the Rust translation of the messages defined by each Sv2
(sub)protocol. They all have the same internal/external dependencies.

**External dependencies**:

- [`serde`](https://crates.io/crates/serde) (only when compiled with the `with_serde` flag)

**Internal dependencies**:

- `const_sv2`
- `binary_sv2`

### 3.09 protocols/v2/sv2-ffi

Exports a C static library with the minimum subset of the `protocols/v2` libraries required to
build a Template Provider. Every dependency is compiled WITHOUT the `with_noise` and `with_serde`
flags.

**Internal dependencies**:

- `codec_sv2`
- `const_sv2`
- `binary_sv2` (maybe in the future will use the one already imported by `codec_sv2`)
- `subprotocols/common_messages_sv2`
- `subprotocols/template_distribution_sv2`

### 3.10 protocols/v2/noise-sv2

Contains the Noise Protocol logic.
TODO: More information.

**External dependencies**: TODO
**Internal dependencies**: TODO

### 3.11 protocols/v2/roles-logic-sv2

**Previously called messages-sv2**.
A "catch-all" workspace of miscellaneous logic, required to build a Sv2 role, but which don't "fit"
into the other workspaces. Includes:

- roles properties
- message handlers
- channel "routing" logic (Group and Extended channels)
- `Sv2Frame` <-> specific (sub)protocol message mapping
- (sub)protocol <-> (sub)protocol mapping
- job logic (this overlaps with the channel logic)
- Bitcoin data structures <-> Sv2 data structures mapping
- utils

A Rust implementation of a Sv2 role should import this crate to have all the required Sv2- or
Bitcoin-related logic. In the future, every library under `protocols/v2` will be reexported by this
crate, so if a Rust implementation of a role needs access to a lower level library, there is no
need to reimport it.

This crate does not assume any async runtime.
The user is required to use a safe `Mutex` defined in `messages_sv2::utils::Mutex`.

**External dependencies**:

- [`serde`](https://crates.io/crates/serde) (only when compiled with the `with_serde` flag)
- [Rust Bitcoin Library](https://docs.rs/bitcoin/latest/bitcoin)

**Internal dependencies**:

- `const_sv2`
- `binary_sv2`
- `framing_sv2`
- `codec_sv2`
- `subprotocols/common_messages_sv2`
- `subprotocols/mining_sv2`
- `subprotocols/template_distribution_sv2`
- `subprotocols/job_negotiation_sv2`

### 3.12 utils/buffer

Unsafe (but fast) buffer pool with fuzz testing and benches. Can be used with `codec_sv2`.

### 3.13 utils/network-helpers

Async runtime specific helpers.
Exports `Connection` used to open an Sv2 connection with or without noise.

TODO: More information

**External dependencies**:

- [`serde`](https://crates.io/crates/serde) (only when compiled with the `with_serde` flag)
- [`async-std`](https://crates.io/crates/async-std)
- [`async-channel`](https://crates.io/crates/async-channel)

**Internal dependencies**:

- `binary_sv2`
- `const_sv2`
- `messages_sv2` (it will be removed very soon already committed in a working branch)

### 3.14 roles/mining-proxy

A Sv2 proxy role logic.

### 3.15 roles/pool

A Sv2 pool role logic.

### 3.16 roles/test-utils/pool

A Sv2 pools useful to do integration tests.

### 3.17 roles/test-utils/mining-device

A Sv2 CPU miner useful to do integration tests.

### 3.18 examples

Contains several example implementations of various use cases.

### 3.19 examples/interop-cpp

TODO

### 3.20 examples/interop-cpp-no-cargo

TODO

### 3.21 examples/ping-pong-with-noise

TODO

### 3.22 examples/ping-pong-without-noise

TODO

### 3.23 examples/sv1-client-and-server

TODO

### 3.24 examples/sv2-proxy

TODO

### 3.25 examples/template-provider-test

TODO

### 3.26 test

Contains integration tests.

### 3.27 experimental

Contains experimental logic that is not yet specified as part of the protocol or extensions.

## 4. Build/Run/Test

### 4.1 Build

### 4.2 Run

### 4.3 Test

Performs test coverage of project's libraries using cargo-tarpaulin and generates results using codecov.io.
The following flags are used when executing cargo-tarpaulin:
`--features`
Includes the code with the listed features.
The following features result in a tarpaulin error and are NOT included:
derive, alloc, arbitrary-derive, attributes, with_serde
`--lib`
Only tests the package's library unit tests. Includes protocols, and utils (without the exclude-files flag, it includes this example because it contains a lib.rs file)
`--exclude-files examples/*`: Excludes all projects in examples directory (specifically added to ignore examples that that contain a lib.rs file like interop-cpp)
`--timeout 120`: If unresponsive for 120 seconds, action will fail
`--fail-under 40`: If code coverage is less than 40%, action will fail
`--out Xml`: Required for codecov.io to generate coverage result

## 5. Branches

TODO

- main
- POCRegtest-1-0-0
- sv2-tp-crates
