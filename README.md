# STRATUM

The goal of the project is to provide:
* A robust set of Sv2 primitives as rust library crates that can be used by anyone that want expand
    the protocol or implement a role (EG a pool that want to support Sv2, or mining-device producer
    that want to integrate Sv2 into the firmware, a bitcoin node that want to implement Template Provide capabilities).
    Make this primitives available also to non rust user. 
* A set of helpers built on top of the above primitives and external bitcoin related rust crates,
    that can be used by anyone the want to implement an Sv2 role (EG a pool that want to support Sv2
    and that use rust)
* An open-source implementation of an Sv2 proxy, that can be used by miners that want to use Sv2.
* An open-source implementation of an Sv2 pool that can be used by anyone that want to operate an Sv2
    pool.
* A self hosted Job Negotiator + Template Provider that can be used by Sv2 or Sv1 pool that want to
    offer job negotiation capabilities to their costumer without messing with the pool codebase (a
    minimum amount of modification is still required)

[Project page](https://stratum-mining.github.io/)

..intro..
..links..

The stratum monorepo is divided in:

* protocols: implementation of Sv1 and all Sv2 subprotocol + utilities to work with them
* roles: implementation of the Sv2 roles
* utils: generic utilities (eg async adaptors)
* examples: just example of how to use the crates inside this repo, each example is a binary repo
    and can be tested with `cargo run`
* test: general tests (eg if crate x can be built on guix, if crate x can be called by C)

## Roles

## Protocols

### Sv2

Stratum v2 is encrypted and it define his custom binary data format. Sv2 is composed by several
crates:

* [**serde_sv2**](/protocols/v2/serde-sv2/src/lib.rs): serde serializer and deserializer for the custom binary format, it also export the
    Sv2 primitive data types.
* [**noise_sv2**](/protocols/v2/noise-sv2/src/lib.rs): used to encrypt and decrypt Sv2 messages.
* [**codec_sv2**](/protocols/v2/codec-sv2/src/lib.rs]): encode and decode Sv2 frames.
* [**framing_sv2**](/protocols/v2/framing-sv2/src/lib.rs): Sv2 frame definition and helpers.
* [**common_messages**](/protocols/v2/subprotocols/common-messages/src/lib.rs): connection messages used by every (sub)protocol.
* **mining-protocol**: the miner protocol as defined by the Sv2 specs.
* **job-negotiation-protocol**: the job negotiation protocol as defined by the Sv2 specs.
* [**template-distribution-protocol**](/protocols/v2/subprotocols/template-distribution/src/lib.rs): the template distribution protocol as defined by the Sv2 specs.

### Sv1

Exports `IsServer` and `IsClient` traits. A client that implement `IsClient` will be a correct Sv1
client and a server implementing `IsServer` will be a correct Sv1 server. This library do not
assume async framework and do not IO, the implementor must decide how to do IO and how to manage
multiple connection. An example of implementation is in `protocols/v1/examples/client-and-server`,
to run the example: `cargo run v1`. To run the test: `cargo test v1`

Stratum v1 do not use any kind of encryption. Stratum v1 data format is json so `serde-json` is
imported. Stratum v1 is defined over json-rpc protocol so inside the v1 crate there is a very simple
json-rpc module.

## Examples

### Ping pong with noise
To run [ping pong with noise](/examples/ping-pong-with-noise/README.md)

1. clone this repo: `git clone git@github.com:stratum-mining/stratum.git`
2. go in the right directory: `cd ./stratum/examples/ping-pong-with-noise`
3. run with cargo: `cargo run`

### Ping pong without noise
To run [ping pong without noise](/examples/ping-pong-without-noise/README.md)

1. clone this repo: `git clone git@github.com:stratum-mining/stratum.git`
2. go in the right directory: `cd ./stratum/examples/ping-pong-without-noise`
3. run with cargo: `cargo run`

### Sv1 client and server
To run [Sv1 client and server](/examples/sv1-client-and-server/src/bin.rs)

1. clone this repo: `git clone git@github.com:stratum-mining/stratum.git`
2. go in the right directory: `cd ./stratum/examples/sv1-client-and-server`
3. run with cargo: `cargo run`

## Contrib

### Merging policy

Usually an optimistic merging policy is adopted but in particular cases the contribution must be
reviewed:
* Code is not easy to understand, in this case the reviewer should indicate a simpler implementation.
* It add a dependency, in this case a discussion about the new dependency is needed.
* It modify the build or deploy process.

For everything else including performance an safety issues just accept the PR then amend the
problematic code and do another PR tagging the author of the amended PR.

#### Formatting
Before merging, run `cargo +nightly fmt` to properly apply the settings in `rustfmt.toml`.
