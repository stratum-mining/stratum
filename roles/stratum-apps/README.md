# Stratum Apps

Complete Stratum V2 application development kit - all utilities in one crate.

## Overview

`stratum-apps` is a unified crate that provides all the utilities needed for building Stratum V2 applications.

## Architecture

This crate is organized into three main modules:

- **`network_helpers`** - High-level networking utilities (from `network_helpers_sv2`)
- **`config_helpers`** - Configuration management helpers (from `config_helpers_sv2`)  
- **`rpc`** - RPC utilities with custom serializable types (from `rpc_sv2`) - *feature-gated*

The crate also re-exports `stratum-core`, the central hub for the Stratum V2 ecosystem that provides a cohesive API for all low-level protocol functionality.

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
stratum-apps = { version = "0.1.0", features = ["pool"] }
```

Basic usage:

```rust
use stratum_apps::{network_helpers, config_helpers};

// For RPC functionality (when rpc feature is enabled)
#[cfg(feature = "rpc")]
use stratum_apps::rpc::{BlockHash, MiniRpcClient};
```

## Features

### Core Features
- `network` - Networking utilities (enabled by default)
- `config` - Configuration helpers (enabled by default)
- `rpc` - RPC utilities with custom serializable types (optional)
  - Provides `Hash`, `BlockHash`, `Amount` types with proper JSON serialization
  - `MiniRpcClient` for Bitcoin RPC communication

### Protocol Features
- `sv1` - Enable SV1 protocol support (includes translation utilities)
- `with_buffer_pool` - Enable buffer pooling for better performance

### Role-Specific Bundles
- `pool` - Everything needed for pool applications
- `jd_client` - Everything needed for JD client applications
- `jd_server` - Everything needed for JD server applications (includes RPC)
- `translator` - Everything needed for translator applications (includes SV1 + translation)
- `mining_device` - Everything needed for mining device applications

## Usage Examples

### Pool Application

```toml
[dependencies]
stratum-apps = { version = "1.0", features = ["pool"] }
```

```rust
use stratum_apps::{network_helpers, config_helpers};

// Use networking
let connection = network_helpers::Connection::new(stream, HandshakeRole::Responder).await?;

// Use configuration
let config: PoolConfig = config_helpers::parse_config("pool.toml")?;
```

### JD Server Application

```toml
[dependencies]
stratum-apps = { version = "1.0", features = ["jd_server"] }
```

```rust
use stratum_apps::{network_helpers, config_helpers, rpc};

// RPC functionality with custom types
use stratum_apps::rpc::{BlockHash, MiniRpcClient};

// All networking and configuration utilities available
// Plus RPC server utilities with proper serialization
```