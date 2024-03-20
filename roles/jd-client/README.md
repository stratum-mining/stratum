# JD Client
The JD Client is a Sv2 proxy that support one extended channel upstream and one extended channel
dowstream, and do job declaration. On start it will:
* connect to the jd-server
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
2. The maximum and minimum SV2 versions (`max_supported_version` and `min_supported_version`)
3. The Job Declarator information which includes the Pool JD connection address (`jd_address`) and the Template Provider connection address to which to connect (`tp_address`).
4. Optionally, you may want to verify that your TP connection is authentic. You may get `tp_authority_public_key` from the logs of your TP, for example:
```
# 2024-02-13T14:59:24Z Template Provider authority key: EguTM8URcZDQVeEBsM4B5vg9weqEUnufA8pm85fG4bZd
```

### Run
1. Copy the `jdc-config-example.toml` into `conf/` directory.
2. Edit it with custom desired configuration and rename it `jdc-config.toml`
3. Point the SV1 Downstream Mining Device(s) to the Translator Proxy IP address and port.
4. Run the Translator Proxy:

   ```
   cd roles/translator
   ```
   ```
   cargo run -p translator_sv2 -- -c conf/jdc-config.toml
   ```
