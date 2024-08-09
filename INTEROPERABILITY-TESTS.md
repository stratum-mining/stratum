# Interoperability tests (DRAFT)

How to test Sv2 compliant software against the SRI implementation.

## Requirements

- [Cargo LLVM Cov](https://github.com/taiki-e/cargo-llvm-cov#installation)

## With Message Generator (MG)

First thing you need to write a test that can be executed by the message generator. In order to do
that read the [message generator doc](https://github.com/stratum-mining/stratum/blob/main/utils/message-generator/README.md)
and look at the [test example](https://github.com/stratum-mining/stratum/blob/main/utils/message-generator/test.json)
Or just ask into the [discord channel](https://discord.com/channels/950687892169195530/1027542903293214720)
Another option would be to use a test template (TODO) that will mock a particular device.

You can either test against a binary of your application or against a remote endpoint.

When the test has be written you can open a PR to this repo in order to add the test to our tests
or fork the project and add the [test here](https://github.com/stratum-mining/stratum/tree/main/test/message-generator)
and if needed the binaries [here](https://github.com/stratum-mining/stratum/tree/main/test/bin). Then you can ran
the test with `./message-generator-tests.sh`. If you keep the codebase unchanged you should be able
to easly merge the updates from upstream main so that test are always against the last SRI version.

### TProxy test with MG
As many people may try the SRI with a sv1 mining device (MD), a MG test for the translation proxy has
been added:
`test/message-generator/test/transation-proxy.json`
You can try this test with your desired sv1 mining device in two ways.
 1. launching it from the test itself, editing the part of `execution_commands` relative to the
    launch of the MD.
 2. removing the part of the test that launches the `/examples/sv1-mining-device` within the SRI.
    Then launch the test and therefore launch separately your desired sv1 MD. 

## Using the SRI role implementations

The easiest way to test various configurations is to use the SRI role implementations. For example,
here is how you would configure/run a cpuminer -sv1-> Translation Proxy -sv2-> Pool role -> (Template Provider *hosted) 

### Pool role startup
The Pool role should be configured to point to the hosted Template Provider. In the `pool-config.toml` file 
you should see this: `tp_address = "75.119.150.111:8442"` The default pool-config should have appropriate
defaults set up for everything else. To run the pool role simply run:
`RUST_LOG=debug cargo run -p pool -- -c roles/v2/pool/pool-config.toml`
NOTE: here we are starting with `debug` log level - the default is `info`.
If the pool propely starts you should see the following log lines:
```
2023-01-11T21:21:14.635902Z  INFO pool::lib::template_receiver: Connected to template distribution server at 75.119.150.111:8442
2023-01-11T21:21:14.827071Z  INFO pool::lib::template_receiver::setup_connection: Setup template provider connection success!
2023-01-11T21:21:14.846946Z  INFO pool::lib::mining_pool: Starting up pool listener
2023-01-11T21:21:14.847238Z  INFO pool::lib::mining_pool: Listening for encrypted connection on: 0.0.0.0:34254
```

### Translation Proxy startup
Once the pool role is running you can start up the Translation Proxy. The translation proxy's default configuration at 
`proxy-config.toml` points to a locally hosted pool role. To run the translation proxy 
1. move to the `roles/translator` directory
    1. `cd roles/translator`
1. run the translator
   1. `RUST_LOG=debug cargo run`
   
If this successfully starts up you'll see these messages as part of the output:
```
2023-01-11T21:29:05.468529Z  INFO translator: PC: ProxyConfig { upstream_address: "127.0.0.1", upstream_port: 34254, upstream_authority_pubkey: "2di19GHYQnAZJmEpoUeP7C3Eg9TCcksHr23rZCC83dvUiZgiDL", downstream_address: "0.0.0.0", downstream_port: 34255, max_supported_version: 2, min_supported_version: 2, min_extranonce2_size: 16 }
2023-01-11T21:29:05.469467Z  INFO translator::upstream_sv2::upstream: PROXY SERVER - ACCEPTING FROM UPSTREAM: 127.0.0.1:34254
2023-01-11T21:29:05.477542Z  INFO translator::upstream_sv2::upstream: Up: Sending: SetupConnection { protocol: MiningProtocol, min_version: 2, max_version: 2, flags: 14, endpoint_host: Owned([48, 46, 48, 46, 48, 46, 48]), endpoint_port: 50, vendor: Owned([]), hardware_version: Owned([]), firmware: Owned([]), device_id: Owned([]) }
2023-01-11T21:29:05.479122Z DEBUG translator::upstream_sv2::upstream: Up: Handling SetupConnectionSuccess
```

### Miner
Lastly you can start up a sv1 miner of your choice. We have been testing with the [cpuminer](https://github.com/pooler/cpuminer). Once you have this installed
just run: `./minerd -a sha256d -o stratum+tcp://localhost:34255 -q -D -P`. This will connect to the translator proxy
and speak sv1. If this is successful you should see the following output:
```
[2023-01-11 09:36:57] 1 miner threads started, using 'sha256d' algorithm.
[2023-01-11 09:36:57] Starting Stratum on stratum+tcp://localhost:34255
*   Trying 127.0.0.1:34255...
* TCP_NODELAY set
* Connected to localhost (127.0.0.1) port 34255 (#0)
* Connection #0 to host localhost left intact
[2023-01-11 09:36:57] > {"id": 1, "method": "mining.subscribe", "params": ["cpuminer/2.5.1"]}
```
and then eventually it will submit a share:
```
[2023-01-11 09:37:00] > {"method": "mining.submit", "params": ["", "2940", "0000000000000000000000000000", "63bdfe63", "c48fe801"], "id":4}
[2023-01-11 09:37:00] < {"id":"4","error":null,"result":true}
[2023-01-11 09:37:00] accepted: 1/1 (100.00%), 15333 khash/s (yay!!!)
```

## Testing sv2 miner -insecure-> pool
The other configuration that can be tested is connecting an sv2 ready miner directly to the pool role. The pool role 
by default assumes a noise handshake but this can be bypassed for testing purposes.

### Pool Role startup
Starting the pool role is the same in both tests:
`RUST_LOG=debug cargo run -p pool -- -c roles/v2/pool/pool-config.toml` 
If you want to test using the insecure (non-encrypted) mode then start the pool with:
`RUST_LOG=debug cargo run --features test_only_allow_unencrypted -p pool -- -c roles/v2/pool/pool-config.toml`
NOTE: this uses the `test_only_allow_unencrypted` feature. 

### Miner startup
Start up your miner pointing either at the secure port `34254` or insecure port: `34250` 

## Optional: To Locally run the Template Provider (TP)
Do this if you'd like to run your own TP instead of connecting to the hosted TP at 75.119.150.111:8442

1. Clone and build core
   1. clone git@github.com:Fi3/bitcoin.git 
   1. checkout AddCoinbaseOutputAdditionalSize branch
   1. `./autogen.sh && ./configure --enable-template-provider`
   1. `make`
1. Start and initialize bitcoind
   1. `./src/bitcoind -regtest` // this will start bitcoind in regtest mode
   1. create at least 16 blocks with `./src/bitcoin-cli -regtest generatetoaddress 16 bcrt1qttuwhmpa7a0ls5kr3ye6pjc24ng685jvdrksxx`
