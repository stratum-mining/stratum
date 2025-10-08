# Stratum Core

Central hub for the Stratum V2 ecosystem, providing a cohesive API for all low-level protocol functionality.

## Overview

`stratum-core` re-exports all the foundational Stratum protocol crates through a single entry point. This includes binary serialization, framing, message handling, cryptographic operations, and all Stratum V2 subprotocols.

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
stratum-core = "0.1.0"
```

Basic usage:

```rust
use stratum_core::{
    binary_sv2,
    codec_sv2,
    framing_sv2,
    noise_sv2,
    mining_sv2,
    // ... all protocol crates available
};
```

## Features

- `with_buffer_pool` - Enable buffer pooling for improved memory management and performance
- `sv1` - Include Stratum V1 protocol support
- `translation` - Enable translation utilities between SV1 and SV2 (includes `sv1`)

