# `channels_sv2`

[![crates.io](https://img.shields.io/crates/v/channels_sv2.svg)](https://crates.io/crates/channels_sv2)
[![docs.rs](https://docs.rs/channels_sv2/badge.svg)](https://docs.rs/channels_sv2)
[![rustc+](https://img.shields.io/badge/rustc-1.75.0%2B-lightgrey.svg)](https://blog.rust-lang.org/2023/12/28/Rust-1.75.0.html)
[![license](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/stratum-mining/stratum/blob/main/LICENSE.md)
[![codecov](https://codecov.io/gh/stratum-mining/stratum/branch/main/graph/badge.svg?flag=channels_sv2-coverage)](https://codecov.io/gh/stratum-mining/stratum)

`channels_sv2` provides primitives and abstractions for Stratum V2 (Sv2) Channels.

This crate implements the core channel management functionality for both mining clients and servers, including standard, extended and group channels, and share accounting mechanisms.

The `client` module is compatible with `no_std` environments. To enable this mode, build the crate with the `no_std` feature. In this configuration, standard library collections are replaced with the `hashbrown` crate, together with `core` and `alloc`, allowing the module to be used in embedded or constrained contexts.

```bash
cargo build --features no_std
```

## Prefix updates and allocation lifetime

The allocator divides the extranonce into
`[upstream_prefix | local_prefix | local_index | rollable]`.
An extended channel's prefix contains the first three regions; a standard channel's
prefix also includes the fixed padding for the rollable region.

An upstream-only update replaces `upstream_prefix` and preserves `local_prefix`,
`local_index`, standard-channel padding, and the existing allocation bitmap slot.
Update the allocator with `ExtranonceAllocator::set_upstream_prefix` and each live
channel with `set_upstream_extranonce_prefix`. These are separate operations:
the application must validate the complete transition before applying it and
coordinate downstream notifications. Each setter leaves its own state unchanged
if validation fails; it does not make the multi-channel transition atomic.
Wire-sourced prefixes have no allocation slot and are entirely upstream-owned,
so their complete value is replaced.

Jobs keep the exact prefix bytes captured at creation. For example, with
one-byte regions, a job created under `[AA | BB | 00]` continues using those
bytes after an upstream-only update changes the channel to `[CC | BB | 00]`.
Both prefix versions share the reservation for `local_index = 00`; this does
not allocate a second slot. If a later whole-prefix rotation changes the
channel to `[CC | BB | 01]`, slot `00` must remain reserved while the old job
is live. Otherwise, changing the allocator's upstream prefix back to `AA`
could reissue `[AA | BB | 00]` while that job still accepts shares.

Channels internally retain old prefix bytes with shared allocation ownership.
Future, active and past jobs keep matching reservations alive; stale or evicted
jobs do not. A slot is returned only when neither the current channel prefix
nor any retained snapshot owns it. Dropping the channel releases its ownership
of all these reservations. Snapshots do not keep a dropped allocator's bitmap
alive.

Repeated snapshots of the same allocation and bytes are deduplicated. Distinct
allocations with identical bytes remain independently reserved. The existing job
history limits and retirement rules are unchanged: upstream updates neither
extend job validity nor mutate a job's captured prefix or validation context.

## Weak-block propagation

`channels_sv2` currently does not support weak-block propagation. The Template Distribution `SetNewPrevHash.target`, which a Template Provider may set below the target `nBits` encodes, is not carried into the channels' chain tip: server channels classify a share as a found block against the `nBits` target alone.
