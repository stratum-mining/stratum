
# Job Declarator Client

The **Job Declarator Client (JDC)** is responsible for:

* Connecting to the **Pool** and **JD Server**.
* Connecting to the **Template Provider**.
* Receiving custom block templates from the Template Provider and declaring them to the pool via the **Job Declaration Protocol**.
* Sending jobs to downstream clients.
* Forwarding shares to the pool.

## Architecture Overview

The JDC sits between **SV2 downstream clients** (e.g., SV2 mining devices or Translator Proxies) and **SV2 upstream servers** (the Pool and JD Server).

* It obtains templates from the Bitcoin node.
* It creates and broadcasts jobs to downstream clients.
* It declares and sets custom jobs to the pool side.
* It also supports solo mining mode in case no upstream is available or the upstream is fraudulent

Note: while JDC can cater for multiple downstream clients, with either one or multiple channels per client, it only opens one single extended channel with the upstream Pool server.

```
<--- Most Downstream ------------------------------------------------------------------------------------------------ Most Upstream --->

+----------------------------------------------------------------------------------------------------+   +------------------------------+
|                     Mining Farm                                                                     |  |      Remote Pool             |
|                                                                                                     |  |                              |
|  +-------------------+     +------------------+                                                     |  |    +-----------------+       |
|  | SV1 Mining Device | <-> | Translator Proxy |-------|                  |------------------------------->  | SV2 Pool Server |       |
|  +-------------------+     +------------------+       |                  |                          |  |    +-----------------+       |
|                                                       |                  |                          |  |                              |
|                                                       |                  |                          |  |                              |
|                                                 +-----------------------+|                          |  |                              |
|                                                 | Job Declarator Client |                           |  |                              |
|                                                 +-----------------------+|                          |  |    +-----------------------+ |
|                                                     |                    |--------------------------------> | Job Declarator Server | |
|   +-------------------+                             |                                               |  |    +-----------------------+ |
|   | SV2 Mining Device |-----------------------------|                                               |  |                              |
|   +-------------------+                                                                             |  |                              |
|                                                                                                     |  |                              |
|                                                                                                     |  |                              |
|                                                                                                     |  |                              |
+----------------------------------------------------------------------------------------------------+   +------------------------------+


```
## Setup

### Configuration File

The configuration file contains the following information:

1. The downstream socket information, which includes the listening IP address (`downstream_address`) and port (`downstream_port`).
2. The maximum and minimum protocol versions (`max_supported_version` and `min_supported_version`) with size as (`min_extranonce2_size`)
3. The authentication keys used for the downstream connections (`authority_public_key`, `authority_secret_key`)
4. The Template Provider address (`tp_address`).

## Configuration

The JDC is configured via a `.toml` file.
See [`config-examples/jdc-config-local-example.toml`](./config-examples/jdc-config-local-example.toml) for a full example.

### Example Configuration

```toml
# Listening address for downstream clients
listening_address = "127.0.0.1:34265"

# Version support
max_supported_version = 2
min_supported_version = 2

# Authentication keys for encrypted downstream connections
authority_public_key  = "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72"
authority_secret_key  = "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n"
cert_validity_sec     = 3600

user_identity = "your_username_here"

# Target shares per minute & batching
shares_per_minute   = 1.0
share_batch_size    = 1
min_extranonce_size = 4

# Template Provider
tp_address = "127.0.0.1:8442"
jdc_signature = "Sv2MinerSignature"

# Coinbase output for solo mining fallback
coinbase_reward_script = "addr(tb1qa0sm0hxzj0x25rh8gw5xlzwlsfvvyz8u96w3p8)"

[[upstreams]]
authority_pubkey = "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72"
pool_address     = "127.0.0.1"
pool_port        = 34254
jd_address       = "127.0.0.1"
jd_port          = 34264
```

For a complete, annotated config, see the [full example](./config-examples/jdc-config-hosted-example.toml).


## Usage

### Installation & Build

```bash
# Clone the repository
git clone https://github.com/stratum-mining/stratum.git
cd stratum

# Build JDC
cargo build --release -p jd_client
```

### Running JDC

#### With Local Pool and Job Declarator Server

```bash
cd roles/jd_client
cargo run -- -c config-examples/jdc-config-local-example.toml
```

#### With Hosted Pool and Job Declarator Server

```bash
cd roles/jd_client
cargo run -- -c config-examples/jdc-config-hosted-example.toml
```

### Command Line Options

```bash
# Use specific config file
jd_client -c /path/to/config.toml
jd_client --config /path/to/config.toml

# Show help
jd_client -h
jd_client --help
```

## Architecture Details

### **Component Overview**

1. **Channel Manager**: Orchestrates message routing among sub-systems in JDC
2. **Task Manager**: Manages async task lifecycle and coordination
3. **Status System**: Provides real-time monitoring and health reporting

## Internal Architecture

JDC is built from several modules that divide responsibility for handling different roles and protocols:

### **Modules**

1. **Upstream**

   * Connects to the **pool**.
   * Handles messages coming from the Pool  (the ones defined in the Common Protocol are directly handled, others are forwarded to the Channel Manager).

2. **Downstream**

   * Accepts connections from Sv2 Mining Devices or Translator Proxies.
   * Includes a **ChannelState**, which provisions new channels when `OpenStandard/ExtendedChannel` messages arrive from the downstreams.

3. **Template Receiver**

   * Connects to the **Template Provider**.
   * Handles messages received by the TP (the ones defined in the Common Protocol are directly handled, while the others are forwarded to the Channel Manager).

4. **Job Declarator**

   * Connects to the **Job Declarator Server (JDS)**.
   * Handles messages received by the JDS (the ones defined in the Common Protocol are directly handled, while the others are forwarded to the Channel Manager).

5. **Channel Manager (Orchestrator)**

   * Central coordination point.
   * Responsibilities:

     * Handles **non-common messages** forwarded from all modules.
     * Maintains **upstream channel state**.
     * Maintains most of the **Job Declarator state**.
     * Orchestrates job lifecycle and state synchronization across upstream and downstream roles.

