# SV2 Core Repository

This is a test repository for the SV2 core protocols architecture.

## Contents

- `sv1/` - Stratum V1 protocol implementation and utilities
- `sv2/` - Stratum V2 protocol implementations
  - `binary-sv2/` - Binary encoding/decoding for SV2 messages
  - `codec-sv2/` - SV2 message codec with encryption support
  - `framing-sv2/` - SV2 message framing utilities
  - `noise-sv2/` - Noise protocol implementation for SV2
  - `subprotocols/` - SV2 subprotocol implementations
  - `channels-sv2/` - Channel management for SV2
  - `roles-logic-sv2/` - Common logic for SV2 roles
  - `parsers-sv2/` - Message parsing utilities
  - `sv2-ffi/` - Foreign Function Interface for SV2
- `roles-utils/` - Utilities for SV2 role implementations
  - `rpc/` - RPC client/server utilities for job declaration
  - `config-helpers/` - Configuration file management helpers
  - `network-helpers/` - Networking utilities for SV2 roles
  - `stratum-translation` - Stratum V1 ↔ Stratum V2 translation utilities
- `common/` - Shared utilities and common code
- `utils/` - General utility crates
  - `buffer/` - Buffer management and pooling
  - `error-handling/` - Error handling utilities
  - `key-utils/` - Cryptographic key utilities
  - `bip32-key-derivation/` - BIP32 key derivation implementation

## Roles Utilities

The `roles-utils/` directory contains shared utilities that are used across multiple SV2 role implementations:

- **RPC (`rpc/`)**: Provides HTTP RPC client functionality for communicating with job declaration servers, including JSON-RPC support and transaction handling.

- **Config Helpers (`config-helpers/`)**: Utilities for parsing and managing Stratum V2 configuration files, including support for miniscript and various configuration formats.

- **Network Helpers (`network-helpers/`)**: Low-level networking utilities for SV2 roles, including connection management for both encrypted (Noise) and plain connections, as well as SV1 compatibility.

- **Stratum Translation (`stratum-translation/`)**: Stratum V1 ↔ Stratum V2 translation utilities for reuse across proxies, apps, and firmware.

These utilities are designed to be consumed by multiple repositories in the Stratum ecosystem, providing a centralized location for common role functionality.

## Local Integration Testing

To run integration tests locally:

```bash
./scripts/run-integration-tests.sh
```

This will:
1. Clone/update the integration test framework
2. Update dependencies to use your local changes
3. Run the full integration test suite
4. Restore the original configuration

## CI/CD

This repository automatically runs integration tests on every PR using the reusable workflow from the `sv2-integration-test-framework` repository.
