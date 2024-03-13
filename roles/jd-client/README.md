# JD Client

* connect to the jd-server
* connect to the template-provider
* listen for an `OpenExtendedChannel` from downstream
* transparently relay the `OpenExtendedChannel` to upstream 
After the setup phase, it will start to negotiate jobs upstream and send the negotiated job
downstream, so that everything downstream does not need to know that the job is negotiated.

## Setup

### Configuration File

The configuration file contains the following information:

1. The downstream connection information includes  connection address (`downstream_address`) and port (`downstream_port`).
2. The maximum and minimum SRI versions (`max_supported_version` and `min_supported_version`) with size as (`min_extranonce2_size`)
3. In this file there is a withhold (`withhold`) with a boolean value.
4. The authentication keys for open encrypted connection for the downstream (`authority_public_key`, `authority_secret_key` and `cert_validity_sec`)
5. The retry that tells JDC the number of times to retry itself after a failure.
6. The Job Declarator information includes the Template Provider connection address to which to connect (`tp_address`).
7. You may want to verify that your TP connection is authentic. You may get `tp_authority_public_key` from the logs of your.

### Run

Run the Job Declarator Client (JDC):
There are two files when you cd into roles/jd-client/config-examples/

1. `jdc-config-hosted-example.toml` connects to the community-hosted roles.
2. `jdc-config-local-example.toml` connects to self-hosted Job Declarator Client (JDC) and Translator Proxy

``` bash
cd roles/jd-client/config-examples/
cargo run -- -c jdc-config-hosted-example.toml
```

