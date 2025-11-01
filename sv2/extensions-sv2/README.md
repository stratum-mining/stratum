# `extensions_sv2`

This crate provides message types and utilities for Stratum V2 protocol extensions.

## Supported Extensions

### Extensions Negotiation (0x0001)

The Extensions Negotiation extension allows endpoints to negotiate which optional extensions are supported during connection setup. This enables backward compatibility and graceful feature detection.

**Messages:**
- `RequestExtensions` - Client requests support for specific extensions
- `RequestExtensionsSuccess` - Server accepts and confirms supported extensions
- `RequestExtensionsError` - Server rejects request or indicates missing required extensions

**Specification:** [extensions-negotiation.md](https://github.com/stratum-mining/sv2-spec/blob/main/extensions/extensions-negotiation.md)

### Worker-Specific Hashrate Tracking (0x0002)

This extension enables tracking of individual worker hashrates within extended channels by adding TLV (Type-Length-Value) fields to `SubmitSharesExtended` messages. It allows pools to monitor per-worker performance while maintaining a single extended channel for multiple workers.

**Specification:** [worker-specific-hashrate-tracking.md](https://github.com/stratum-mining/sv2-spec/blob/main/extensions/worker-specific-hashrate-tracking.md)

## Module Structure

The crate is organized into three main modules:

### `tlv` - Generic TLV Utilities
Generic encoding/decoding for Type-Length-Value fields, reusable by any TLV-based extension:
- `Tlv` - Generic TLV structure with encode/decode methods
- `has_tlv_for_extension()` - Check for TLV presence
- `has_valid_tlv_data()` - Validate TLV data against negotiated extensions
- `TlvError` - Error types for TLV operations

The `Tlv` struct provides type-safe access to TLV fields:
- `extension_type: u16` - Extension identifier (e.g., 0x0002)
- `field_type: u8` - Field identifier within extension
- `length: u16` - Value length in bytes
- `value: Vec<u8>` - Raw value bytes

### `extensions_negotiation` - Extension Negotiation (0x0001)
Messages for negotiating extension support (does not use TLV fields):
- `RequestExtensions`
- `RequestExtensionsSuccess`
- `RequestExtensionsError`

### `worker_specific_hashrate_tracking` - Worker-Specific Hashrate Tracking (0x0002)
Worker identity tracking using TLV fields:
- `UserIdentity` - Worker identifier structure
- `build_submit_shares_extended_with_user_identity_frame()` - Create frames with TLV
- Extension-specific TLV encoding/decoding