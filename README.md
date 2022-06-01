# Stratum
The Stratum protocol defines how miners, proxies, and pool communicate to contribute hashrate to
the Bitcoin network.

## Table of Contents
- [Stratum](#stratum)
  * [1. Goals](#1-goals)
  * [2. Common Use Cases](#2-common-use-cases)
    * [2.1 Miners](#21-miners)
    * [2.2 Pools](#22-pools)
  * [3. Structure](#3-structure)
    + [3.01 `protocols`](https://github.com/stratum-mining/stratum#301-protocols)
    + [3.02 `protocols/v1`](https://github.com/stratum-mining/stratum#302--protocols-v1)
    + [3.03 `protocols/v2`](https://github.com/stratum-mining/stratum#303--protocols-v2)
    + [3.04 `protocols/v2/const-sv2`](https://github.com/stratum-mining/stratum#304--protocols-v2-const-sv2)
    + [3.05 `protocols/v2/binary-sv2`](https://github.com/stratum-mining/stratum#305--protocols-v2-binary-sv2)
    + [3.06 `protocols/v2/framing-sv2`](https://github.com/stratum-mining/stratum#306--protocols-v2-framing-sv2)
    + [3.07 `protocols/v2/codec-sv2`](https://github.com/stratum-mining/stratum#307--protocols-v2-codec-sv2)
    + [3.08 `protocols/v2/subprotocols`](https://github.com/stratum-mining/stratum#308--protocols-v2-subprotocols)
    + [3.09 `protocols/v2/sv2-ffi`](https://github.com/stratum-mining/stratum#309--protocols-v2-sv2-ffi)
    + [3.10 `protocols/v2/noise-sv2`](https://github.com/stratum-mining/stratum#310--protocols-v2-noise-sv2)
    + [3.11 `protocols/v2/roles-logic-sv2`](https://github.com/stratum-mining/stratum#311--protocols-v2-roles-logic-sv2)
    + [3.12 `utils/buffer`](https://github.com/stratum-mining/stratum#312--utils-buffer)
    + [3.13 `utils/network-helpers`](https://github.com/stratum-mining/stratum#313--utils-network-helpers)
    + [3.14 `roles/mining-proxy`](https://github.com/stratum-mining/stratum#314--roles-mining-proxy)
    + [3.15 `roles/pool`](https://github.com/stratum-mining/stratum#315--roles-pool)
    + [3.16 `roles/test-utils/pool`](https://github.com/stratum-mining/stratum#316--roles-test-utils-pool)
    + [3.17 `roles/test-utils/mining-device`](https://github.com/stratum-mining/stratum#317--roles-test-utils-mining-device)
    + [3.18 `examples`](https://github.com/stratum-mining/stratum#318--examples)
    + [3.19 `examples/interop-cpp`](https://github.com/stratum-mining/stratum#319--examples-interop-cpp)
    + [3.20 `examples/interop-cpp-no-cargo`](https://github.com/stratum-mining/stratum#320--examples-interop-cpp-no-cargo)
    + [3.21 `examples/ping-pong-with-noise`](https://github.com/stratum-mining/stratum#321--examples-ping-pong-with-noise)
    + [3.22 `examples/ping-pong-without-noise`](https://github.com/stratum-mining/stratum#322--examples-ping-pong-without-noise)
    + [3.23 `examples/sv1-client-and-server`](https://github.com/stratum-mining/stratum#323--examples-sv1-client-and-server)
    + [3.24 `examples/sv2-proxy`](https://github.com/stratum-mining/stratum#324--examples-sv2-proxy)
    + [3.25 `examples/template-provider-test`](https://github.com/stratum-mining/stratum#325--examples-template-provider-test)
    + [3.26 `test`](https://github.com/stratum-mining/stratum#326--test)
    + [3.27 `experimental`](https://github.com/stratum-mining/stratum#327--experimental)
  * [4. Branches](#4-branches)

## 1. Goals
The goals of this project is to provide:

1. A robust set of Stratum V2 (Sv2) primitives as Rust library crates that can be used by anyone
   that wants to expand the protocol or implement a role (e.g. a pool wanting to support Sv2, a
   mining-device/hashrate producer that wants to integrate Sv2 into their firmware, a Bitcoin node
   that wants to implement Template Provider capabilities to build the `blocktemplate`).

1. The above Rust primitives as a C library, making it available to non-Rust users via a C
   bindings.

1. A set of helpers built on top of the above primitives and the external Bitcoin-related Rust
   crates that can be used by anyone who wants to implement the Sv2 roles (e.g. a pool that uses
   Rust and wants to support Sv2).

1. An open-source implementation of a Sv2 proxy that can be used by miners that want to use Sv2.

1. An open-source implementation of a Sv2 pool that can be used by anyone that wants to operate a
   Sv2 pool.


## 2. Common Use Cases
Different portions of the library will be used depending on the use case and/or desired
functionality. Some examples of different use-cases are:

## 2.1 Miners

1. A miner can use the proxy (`roles/sv2/mining-proxy`) to connect with a Sv2-compatible pool.

1. A miner who runs a mining farm with SV1-compatible mining firmware mining to a Sv2-compatible
   pool, who wants to gain some of the security and efficiency improvements that Sv2 offers over
   Stratum V1 (Sv1). This Sv1<->Sv2 miner proxy does not support all the features of Sv2, therefore
   it should be used as a temporary measure before completely upgrading to Sv2-compatible firmware.
   This Sv1<->Sv2 translation has not yet been implemented, but is currently being worked on.

## 2.2 Pools

1. A pool that wants to support Sv2 can deploy the open source binary crate (`roles/pool`) to offer
   an Sv2-compatible pool to their clients (miners participating in said pool).

1. A pool that wants to build an Sv2 compatible pool in Rust will likely use the Rust helper
   library.

1. A pool that wants to build an Sv2 compatible pool not in Rust will likely use the Rust
   primitives as C library.

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

This workspace is divided in 6 macro-areas:
1. `protocols`: Stratum V2 and V1 libraries.
1. `roles`: The Sv2 roles logic.
1. `utils`: Offers an alternative buffer to use that is more efficient, but less safe than the
            default buffers used. Also has network helpers.
1. `examples`: Several example implementations of various use cases.
1. `test`: Integration tests.
1. `experimental`: Experimental logic that is not yet specified as part of the protocol or
                   extensions.

All dependencies related to the testing and benchmarking of these workspace crates are optional
and are included in the binary ONLY during testing or benchmarking is executed. For that reason
these dependencies are NOT included under **External dependencies** in the lists below.

### 3.01 `protocols`
Core Stratum V2 and V1 libraries.

### 3.02 `protocols/v1`
Contains a Sv1 library.
TODO: more information

### 3.03 `protocols/v2`
Contains several Sv2 libraries serving various purposes depending on the desired application.
TODO: more info

### 3.04 `protocols/v2/const-sv2`
Contains the Sv2 related constants.

### 3.05 `protocols/v2/binary-sv2`
Under `binary-sv2` are five crates (`binary-sv2`, `serde-sv2`, `no-serde-sv2`,
`no-serde-sv2/codec`, `no-serde-sv2/derive_codec`) that are contain Sv2 data types binary mappings.
However, only `procotols/v2/binary-sv2/binary-sv2` is used by the user, the remaining are for
internal use only, therefore only `procotols/v2/binary-sv2/binary-sv2` is discussed here.

Exports an API that allows the serialization and deserialization of the Sv2 primitive data
types. It also exports two procedural macros, `Encodable` and `Decodable`, that, when applied to a
struct, make it serializable/deserializable to/from the Sv2 binary format.

This crate can be compiled with the `with_serde` feature, which will use the external `serde` crate
to serialize and deserialize. If this feature flag is NOT set, an internal serialization engine is
used. The exported API is the same when compiled `with_serde` and not.

**External dependencies**:
* [`serde`](https://crates.io/crates/serde) (only when compiled with the `with_serde` flag)

**Internal dependencies**:
* `buffer-sv2` in `protocols/v2/serde-sv2` (only when compiled with the `with_serde` flag)
* `no-serde-sv2` (only when compiled WITHOUT the `with_serde` flag)

### 3.06 `protocols/v2/framing-sv2`
Exports the `Frame` trait. A `Frame` can:
* be serialized (`serialize`) and deserialized (`from_bytes`, `from_bytes_unchecked`)
* return the payload (`payload`)
* return the header (`get_header`)
* be constructed from a serializable payload when the payload does not exceed the maximum frame
  size (`from_message`)

Two implementations of the `Frame` trait are exports: `Sv2Frame` and `NoiseFrame`. An enum named
`EitherFrame` representing one of these frames is also exported.

**External dependencies**:
* [`serde`](https://crates.io/crates/serde) (only when compiled with the `with_serde` flag)

**Internal dependencies**:
* `const_sv2`
* `binary_sv2`

### 3.07 `protocols/v2/codec-sv2`
Exports `StandardNoiseDecoder` and `StandardSv2Decoder` that are initialized with a buffer
containing a "stream" of bytes. When `next_frame` is called, they return either a `Sv2Frame` or an
error containing the number of missing bytes to allow next step[^1], so that the caller can fill the
buffer with the missing bytes and then call it again.

It also export `NoiseEncoder` and `Encoder`.

[^1] For the case of a non-noise decoder, this refers to the missing bytes that are either needed
to 1) complete the frame header, or 2) to have a complete frame. For the case of a noise decoder,
this is more complex as it returns a `Sv2Frame`, which can be composed of several `NoiseFrames`.

**External dependencies**:
* [`serde`](https://crates.io/crates/serde) (only when compiled with the `with_serde` flag)

**Internal dependencies**:
* `const_sv2`
* `binary_sv2`
* `framing_sv2`
* `noise_sv2` (only when compiled with the `with_noise` flag)

### 3.08 `protocols/v2/subprotocols`
Under `subprotocols` are four crates (`common-messages`, `job-negotiation`, `mining`, and
`template-distribution`) that are the Rust translation of the messages defined by each Sv2
(sub)protocol. They all have the same internal external dependencies.

**External dependencies**:
* [`serde`](https://crates.io/crates/serde) (only when compiled with the `with_serde` flag)

**Internal dependencies**:
* `const_sv2`
* `binary_sv2`

### 3.09 `protocols/v2/sv2-ffi`
Exports a C static library with the minimum subset of the `protocols/v2` libraries required to
build a Template Provider. Every dependency is compiled WITHOUT the `with_noise` and  `with_serde`
flags.

**Internal dependencies**:
* `codec_sv2`
* `const_sv2`
* `binary_sv2` (maybe in the future will be used the one already imported by `codec_sv2`)
* `subprotocols/common_messages_sv2`
* `subprotocols/template_distribution_sv2`

### 3.10 `protocols/v2/noise-sv2`
Contains the Noise Protocol logic.
TODO: More information.

**External dependencies**: TODO
**Internal dependencies**: TODO

### 3.11 `protocols/v2/roles-logic-sv2`
**Previously called messages-sv2**.
A sort of "catch-all" workspace that contains miscellaneous logic required to build a Sv2 role, but
that does not "fit" into the other workspaces. This includes:
* roles properties
* message handlers
* channel "routing" logic (Group and Extended channels)
* `Sv2Frame` <-> specific (sub)protocol message mapping
* (sub)protocol <-> (sub)protocol mapping
* job logic (this overlaps with the channel logic)
* Bitcoin data structures <-> Sv2 data structures mapping
* utils

A Rust implementation of a Sv2 role should import this crate to have all the required Sv2- or
Bitcoin-related logic. In the future, every library under `protocols/v2` will be reexported by this
crate, so if a Rust implementation of a role needs access to a lower level library, there is no
need to reimport it.

This crate does not assume any async runtime. The only thing that the user is required to use is a
safe `Mutex` defined in `messages_sv2::utils::Mutex`.

**External dependencies**:
* [`serde`](https://crates.io/crates/serde) (only when compiled with the `with_serde` flag)
* [Rust Bitcoin Library](https://docs.rs/bitcoin/latest/bitcoin)

**Internal dependencies**:
* `const_sv2`
* `binary_sv2`
* `framing_sv2`
* `codec_sv2`
* `subprotocols/common_messages_sv2`
* `subprotocols/mining_sv2`
* `subprotocols/template_distribution_sv2`
* `subprotocols/job_negotiation_sv2`

### 3.12 `utils/buffer`
Unsafe but fast buffer pool with fuzz testing and benches. Can be used with `codec_sv2`.

### 3.13 `utils/network-helpers`
Async runtime specific helpers. 
Exports `Connection` which is used to open an Sv2 connection either with or without noise.

TODO: More information

**External dependencies**:
* [`serde`](https://crates.io/crates/serde) (only when compiled with the `with_serde` flag)
* [`async-std`](https://crates.io/crates/async-std)
* [`async-channel`](https://crates.io/crates/async-channel)

**Internal dependencies**:
* `binary_sv2`
* `const_sv2`
* `messages_sv2` (it will be removed very soon already commited in a wroking branch)

### 3.14 `roles/mining-proxy`
A Sv2 proxy role logic.

### 3.15 `roles/pool`
A Sv2 pool role logic.

### 3.16 `roles/test-utils/pool`
A Sv2 pools useful to do integration tests.

### 3.17 `roles/test-utils/mining-device`
A Sv2 CPU miner useful to do integration tests.

### 3.18 `examples`
Contains several example implementations of various use cases.

### 3.19 `examples/interop-cpp`
TODO

### 3.20 `examples/interop-cpp-no-cargo`
TODO

### 3.21 `examples/ping-pong-with-noise`
TODO

### 3.22 `examples/ping-pong-without-noise`
TODO

### 3.23 `examples/sv1-client-and-server`
TODO

### 3.24 `examples/sv2-proxy`
TODO

### 3.25 `examples/template-provider-test`
TODO

### 3.26 `test`
Contains integration tests.

### 3.27 `experimental`
Contains experimental logic that is not yet specified as part of the protocol or extensions.

## 4. Branches
TODO

* main
* POCRegtest-1-0-0
* sv2-tp-crates
