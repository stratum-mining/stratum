# SV1 to SV2 Translator Proxy

A proxy that translates between Stratum V1 (SV1) and Stratum V2 (SV2) mining protocols. This translator enables SV1 mining devices to connect to SV2 pools and infrastructure, bridging the gap between legacy mining hardware and modern mining protocols.

## Architecture Overview

The translator sits between SV1 downstream roles (mining devices) and SV2 upstream roles (pool servers or proxies), providing seamless protocol translation and advanced features like channel aggregation and failover.

```
<--- Most Downstream ----------------------------------------- Most Upstream --->

+---------------------------------------------------+  +------------------------+
|                     Mining Farm                   |  |      Remote Pool       |
|                                                   |  |                        |
|  +-------------------+     +------------------+   |  |   +-----------------+  |
|  | SV1 Mining Device | <-> | Translator Proxy | <------> | SV2 Pool Server |  |
|  +-------------------+     +------------------+   |  |   +-----------------+  |
|                                                   |  |                        |
+---------------------------------------------------+  +------------------------+
```

## Configuration

### Configuration File Structure

The translator uses TOML configuration files with the following structure:

```toml
# Downstream SV1 Connection (where miners connect)
downstream_address = "0.0.0.0"
downstream_port = 34255

# Protocol Version Support
max_supported_version = 2
min_supported_version = 2

# Extranonce Configuration
downstream_extranonce2_size = 4  # Min: 2, Max: 16 (CGminer max: 8)

# User Identity (appended with counter for each miner)
user_identity = "your_username_here"

# Channel Configuration
aggregate_channels = true  # true: shared channel, false: individual channels

# Downstream Difficulty Configuration
[downstream_difficulty_config]
min_individual_miner_hashrate = 10_000_000_000_000.0  # 10 TH/s
shares_per_minute = 6.0
enable_vardiff = true  # Set to false when using with Job Declarator Client (JDC)

# Upstream SV2 Connections (supports multiple with failover)
[[upstreams]]
address = "127.0.0.1"
port = 34254
authority_pubkey = "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72"

[[upstreams]]
address = "backup.pool.com"
port = 34254
authority_pubkey = "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72"
```

### Configuration Parameters

#### **Downstream Configuration**
- `downstream_address`: IP address for SV1 miners to connect to
- `downstream_port`: Port for SV1 miners to connect to

#### **Protocol Configuration**
- `max_supported_version`/`min_supported_version`: SV2 protocol version support
- `min_extranonce2_size`: Minimum extranonce2 size (affects mining efficiency)

#### **Channel Configuration**
- `aggregate_channels`: 
  - `true`: All miners share one upstream extended channel (more efficient)
  - `false`: Each miner gets its own upstream extended channel (more isolated)
- `user_identity`: Username for pool authentication (auto-suffixed per miner)

#### **Difficulty Configuration**
- `min_individual_miner_hashrate`: Expected hashrate of weakest miner (in H/s)
- `shares_per_minute`: Target share submission rate
- `enable_vardiff`: Enable/disable variable difficulty adjustment (set to false when using with JDC)
  - When `true`: Translator manages difficulty adjustments based on share submission rates
  - When `false`: Upstream manages difficulty, translator forwards SetTarget messages to miners

#### **Upstream Configuration**
- `address`/`port`: SV2 upstream server connection details
- `authority_pubkey`: Public key for SV2 connection authentication

## Usage

### Installation & Build

```bash
# Clone the repository
git clone https://github.com/stratum-mining/stratum.git
cd stratum

# Build the translator
cargo build --release -p translator_sv2
```

### Running the Translator

#### **With Local Pool**
```bash
cd roles/translator
cargo run -- -c config-examples/tproxy-config-local-pool-example.toml
```

#### **With Job Declaration Client**
```bash
cd roles/translator
cargo run -- -c config-examples/tproxy-config-local-jdc-example.toml
```

#### **With Hosted Pool**
```bash
cd roles/translator
cargo run -- -c config-examples/tproxy-config-hosted-pool-example.toml
```

### Command Line Options

```bash
# Use specific config file
translator_sv2 -c /path/to/config.toml
translator_sv2 --config /path/to/config.toml

# Show help
translator_sv2 -h
translator_sv2 --help
```

## Configuration Examples

### Example 1: Local Pool Setup
For connecting to a local SV2 pool server:

```toml
downstream_address = "0.0.0.0"
downstream_port = 34255
user_identity = "miner_farm_1"
aggregate_channels = true

[downstream_difficulty_config]
min_individual_miner_hashrate = 10_000_000_000_000.0
shares_per_minute = 6.0
enable_vardiff = true

[[upstreams]]
address = "127.0.0.1"
port = 34254
authority_pubkey = "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72"
```

### Example 2: High-Availability Setup
For production environments with failover:

```toml
downstream_address = "0.0.0.0"
downstream_port = 34255
user_identity = "production_farm"
aggregate_channels = true

[downstream_difficulty_config]
min_individual_miner_hashrate = 50_000_000_000_000.0  # 50 TH/s
shares_per_minute = 10.0
enable_vardiff = true

# Primary upstream
[[upstreams]]
address = "primary.pool.com"
port = 34254
authority_pubkey = "primary_pool_pubkey"

# Backup upstream
[[upstreams]]
address = "backup.pool.com"
port = 34254
authority_pubkey = "backup_pool_pubkey"
```

## Architecture Details

### **Component Overview**

1. **SV1 Server**: Handles incoming SV1 connections from mining devices
2. **SV2 Upstream**: Manages connections to SV2 pool servers with failover
3. **Channel Manager**: Orchestrates message routing and protocol translation
4. **Task Manager**: Manages async task lifecycle and coordination
5. **Status System**: Provides real-time monitoring and health reporting

### **Channel Modes**

- **Aggregated Mode**: All miners share one  extended channel
  - More efficient for large farms
  - Reduced upstream connection overhead
  - Shared work distribution

- **Non-Aggregated Mode**: Each miner gets individual upstream channel
  - Better isolation between miners
  - Individual difficulty adjustment by the upstream Pool