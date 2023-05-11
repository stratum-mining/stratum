# JN Client
The JN Client is a Sv2 proxy that support one extended channel upstream and one extended channel
dowstream, and do job negotiation. On start it will:
* connect to the jn-server
* connect to the template-provider
* listen for and `OpenExtendedChannel` from downstream
* transparently relay the `OpenExtendedChannel` to upstream 
After the setup phase it will start to negotiate jobs with upstream and send the negotiated job
downstream, so that everything that is dowstream do not need to know that job is negotiated.

## Setup
### Configuration File
The `proxy-config-example.toml` is a configuration example.

The configuration file contains the following information:

1. The Upstream connection information which includes the SV2 Pool authority public key 
   (`upstream_authority_pubkey`) and the SV2 Pool connection address (`upstream_address`) and port
   (`upstream_port`).
1. The maximum and minimum SV2 versions (`max_supported_version` and `min_supported_version`)
1. The Job Negotiator information which includes the Pool JN connection address (`jn_address`) and the Template Provider connection address to which to connect (`tp_address`).

### Run
1. Copy the `proxy-config-example.toml` into `conf/` directory.
2. Edit it with custom desired configuration and rename it `proxy-config.toml`
3. Point the SV1 Downstream Mining Device(s) to the Translator Proxy IP address and port.
4. Run the Translator Proxy:

   ```
   cd roles/translator
   ```
   ```
   cargo run -p translator_sv2 -- -c conf/proxy-config.toml
   ```
