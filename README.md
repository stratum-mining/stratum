# STRATUM

# Stratum

..intro..
..links..

The stratum monorepo contains...

## Protocols

### Sv1

Exports `IsServer` and `IsClient` traits. A client that implement `IsClient` will be a correct Sv1
client and a server implementing `IsServer` will be a correct Sv1 server. This library do not
assume async framework and do not IO, the implementor must decide how to do IO and how to manage
multiple connection. An example of implementation is in `protocols/v1/examples/client-and-server`,
to run the example: `cargo run v1`. To run the test: `cargo test v1`

Stratum v1 do not use any kind of encryption. Stratum v1 data format is json so `serde-json` is
imported. Stratum v1 is defined over json-rpc protocol so inside the v1 crate there is a very simple
json-rpc module.

### Sv2

Stratum v2 is encrypted and it define his custom binary data format. Sv2 is composed by several
crates:

* **serde-sv2**: serde serializer and deserializer for the custom binary format, it also export the
    Sv2 primitive data types.
* **noise**: used to encrypt and decrypt Sv2 messages
* **common**: connection messages used by every (sub)protocol.
* **mining-protocol**: the miner protocol as defined by the Sv2 specs.
* **job-negotiation-protocol**: the job negotiation protocol as defined by the Sv2 specs.
* **template-distribution-protocol**: the template distribution protocol as defined by the Sv2 specs.

## Translations

## Roles

## Contrib


### Merging policy

Usually an optimistic merging policy is adopted but in particular cases the contribution must be
reviewed:
* Code is not easy to understand, in this case the reviewer should indicate a simpler implementation.
* It add a dependency, in this case a discussion about the new dependency is needed.
* It modify the build or deploy process.

For everything else including performance an safety issues just accept the PR then amend the
problematic code and do another PR tagging the author of the amended PR.
