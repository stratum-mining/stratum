# JD Client

* connect to the jd-server
* connect to the template-provider
The JD Client receives custom block templates from a Template Provider and declares use of the template with the pool using the Job Declaration Protocol. Further distributes the jobs to Mining Proxy (or Proxies) using the Job Distribution Protocol. ```
* transparently relay the `OpenExtendedChannel` to upstream 

## Setup

### Configuration File

The configuration file contains the following information:

1. The downstream socket information, which includes the listening IP address (`downstream_address`) and port (`downstream_port`).
2. The maximum and minimum SRI versions (`max_supported_version` and `min_supported_version`) with size as (`min_extranonce2_size`)
3. The authentication keys for the downstream connection (`authority_public_key`, `authority_secret_key`)
4. A `retry` parameter which tells JDC the number of times to reinitialize itself after a failure.
6. The Template Provider address (`tp_address`).
7. Optionally, you may want to verify that your TP connection is authentic. You may get `tp_authority_public_key` from the logs of your TP, for example:

# 2024-02-13T14:59:24Z Template Provider authority key: EguTM8URcZDQVeEBsM4B5vg9weqEUnufA8pm85fG4bZd

### Run

Run the Job Declarator Client (JDC):
There are two files when you cd into roles/jd-client/config-examples/

1. `jdc-config-hosted-example.toml` connects to the community-hosted roles.
2. `jdc-config-local-example.toml` connects to self-hosted Job Declarator Client (JDC) and Translator Proxy

``` bash
cd roles/jd-client/config-examples/
cargo run -- -c jdc-config-hosted-example.toml
```
