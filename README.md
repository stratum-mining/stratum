# Stratum V2 (Sv2) Proxy

The goal of the project is to provide:
* A robust set of Sv2 primitives as rust library crates that can be used by anyone that want expand
    the protocol or implement a role (EG a pool that want to support Sv2, or mining-device producer
    that want to integrate Sv2 into the firmware, a bitcoin node that want to implement Template Provide capabilities).
    Make this primitives available also to non rust user. 
* A set of helpers built on top of the above primitives and external bitcoin related rust crates,
    that can be used by anyone the want to implement an Sv2 role (EG a pool that want to support Sv2
    and that use Rust)
* An open-source implementation of an Sv2 proxy, that can be used by miners that want to use Sv2.
* An open-source implementation of an Sv2 pool that can be used by anyone that want to operate an Sv2
    pool.
* A self hosted Job Negotiator + Template Provider that can be used by Sv2 or Sv1 pool that want to
    offer job negotiation capabilities to their costumer without messing with the pool codebase (a
    minimum amount of modification is still required)

## STRUCTURE

This workspace is divided in 6 macro-areas, (1) *protocols* several Stratum library, (2) *roles* the
actual Sv2 roles, (3) *utils*, (4) *examples*, (5) *test* integration tests, (6) *experimental* not yet
specified part of the protocol or extensions.

Every dependency related to benchmarking and testing can be always opted out so we never list
them under **External dependencies**.

### PROTOCOLS

Under *protocols* there is *v1* that contain an Sv1 library and *v2* that contain several Sv2
libraries.

### PROTOCOLS/V1
TODO

### PROTOCOLS/V2
TODO

### PROTOCOLS/V2/CONST-SV2

A bunch of Sv2 related constants.

**External dependencies**:
* no dependencies

**Internal dependencies**:
* no dependencies

### PROTOCOLS/V2/BINARY-SV2/BINARY-SV2

Sv2 data types binary mapping.

It export an API that allow to serialize and deserialize the Sv2 primitive data types, it also
export two procedural macro `Encodable` and `Decodable` that when applied to a struct make it
serializable/deserializable to/from the Sv2 binary format.

This crate can be compiled with the `with_serde` feature, when the feature is on the crate will use
serde in order to serialize and deserialize when it is off it will use an internal engine. The exported
API is the same when compiled `with_serde` and not.

**External dependencies**:
* serde (only when compiled `with_serde`)

**Internal dependencies**:
* buffer-sv2 (only when compiled `with_serde`)

### PROTOCOLS/V2/FRAMING-SV2

It export the `Frame` trait. A frame can:
* be serialized (`serialize`), deserialized (`from_bytes`, `from_bytes_unchecked`)
* return the payload (`payload`)
* return the header (`get_header`)
* be constructed from a serializable payload when the payload do not exceed the maximum frame size
    (`from_message`)

Along with `Frame` two implementation are exported `Sv2Frame`, `NoiseFrame` and an enum `EitherFrame`.

**External dependencies**:
* serde (only when compiled `with_serde`)

**Internal dependencies**:
* const_sv2
* binary_sv2

### PROTOCOLS/V2/CODEC-SV2

Exports `StandardNoiseDecoder` and `StandardSv2Decoder` they get initialized with a buffer that
contain a "stream" of bytes. When `next_frame` is called they return either an `Sv2Frame` or and
error containing the number of missing bytes to allow next step^1 so that the caller can fill the buffer
with the missing bytes and then call it again.

It also export `NoiseEncoder` and `Encoder`.

^1 for non noise decoder that refer to either missing bytes to complete the frame header or missing bytes to
have a complete frame. In noise case is more complex cause it return an `Sv2Frame` but an `Sv2Frame`
can be composed by several `NoiseFrame`.


**External dependencies**:
* serde (only when compiled `with_serde`)

**Internal dependencies**:
* const_sv2
* binary_sv2
* framing_sv2
* noise_sv2 (only when compiled `with_noise`)

### PROTOCOLS/V2/SUBPROTOCOLS

Under subprotocols there are 4 crates (common-messages, job-negotiation, mining,
template-distribution). They are just the Rust translation of the messages defined by each Sv2
(sub)protocol. They all have the same internal external dependencies.

**External dependencies**:
* serde (only when compiled `with_serde`)

**Internal dependencies**:
* const_sv2
* binary_sv2

### PROTOCOLS/V2/SV2-FFI

Export a C static library with the min subset of `protocols/v2` needed to build a Template Provider.
Every dependency is compiled without noise and without serde.

**External dependencies**:
* no dependencies

**Internal dependencies**:
* codec_sv2
* const_sv2
* binary_sv2 (maybe in the future will be used the one already imported by codec_sv2)
* subprotocols/common_messages_sv2
* subprotocols/template_distribution_sv2

### PROTOCOLS/V2/NOISE-SV2
TODO

### PROTOCOLS/V2/MESSAGES-SV2

**Very soon it will be renamed in roles-logic-sv2**. (already commited in a working branch)

It contain everything that is needed to build an Sv2 role that do not fit in the above crates. So we
have:
* roles properties
* message handlers
* channel "routing" logic (group extended)
* `Sv2Frame` <-> specific (sub)protocol message mapping
* (sub)protocol <-> (sub)protocol mapping
* job logic (it overlap with channel logic)
* bitcoin data structures <-> sv2 data structures mapping
* utils

A Rust implementation of an Sv2 role is supposed to import this crate in order to have anything it
need that is Sv2 or bitcoin related. In the future every library under `protocols/v2` will be
reexported by this crate, so if a Rust implementation of a role need access to a lower level library
there is no need to reimport it.

This crate do not assume any async runtime. The only thing that the user is required to use is a
safe `Mutex` defined in `messages_sv2::utils::Mutex`.

**External dependencies**:
* serde (only when compiled `with_serde`)
* bitcoin

**Internal dependencies**:
* const_sv2
* binary_sv2
* framing_sv2
* codec_sv2
* subprotocols/common_messages_sv2
* subprotocols/mining_sv2
* subprotocols/template_distribution_sv2
* subprotocols/job_negotiation_sv2

### UTILS/BUFFER

Unsafe fast buffer pool with fuzzy testing and benches. Can be used with codec_sv2.

**External dependencies**:
* no dependencies

**Internal dependencies**:
* no dependencies

### UTILS/NETWORK-HELPERS

Async runtime specific helpers.

It export `Connection` that is used to open an Sv2 connection either with or without noise.

**External dependencies**:
* serde (only when compiled `with_serde`)
* async-std
* async-channel

**Internal dependencies**:
* binary_sv2
* const_sv2
* messages_sv2 (it will be removed very soon already commited in a wroking branch)

### ROLES/MINING-PROXY
An Sv2 proxy

### ROLES/POOL
An Sv2 pool

### ROLES/TEST-UTILS/POOL
An Sv2 pools useful to do integration tests

### ROLES/TEST-UTILS/MINING-DEVICE
An Sv2 CPU miner useful to do integration tests

### EXAMPLES
TODO

### EXPERIMENTAL
TODO


## BRANCHES
TODO

* main
* POCRegtest-1-0-0
* sv2-tp-crates
