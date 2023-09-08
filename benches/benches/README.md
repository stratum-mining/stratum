# SV1 and SV2 clients benchmarks
This directory contains code that benchmarks the performance of sv1 and sv2 clients using the `criterion` and `iai` library.

The benchmarking project aims to compare the performance of sv2 against sv1 and demonstrate the improvements of sv2. The benchmarks measure various aspects of the clients' performance, including  subscription latency, share submission time, systems requirements such as RAM access, Instructions per cycle and more..

## Project Structure
The project is structured as follows:

- The benches/ directory contains the main project files.
- The src/ directory contains subdirectories for sv1 and sv2 clients.
- Each client directory contains benchmark files criterion_svX_benchmark.rs and iai_svX_benchmark.rs for sv1 and sv2 respectively.
- The lib/client.rs file within each client directory contains additional code relating to each client implementation.


- criterion_svX_benchmark.rs: Uses the criterion crate for latency benchmarking.
- iai_benchmarks.rs: Uses the iai crate to measure system requirements and performance.

## Running Benchmarks
- To run the benchmarks, follow these steps:

- Install Rust and Cargo if you haven't already.
- Clone this repository
```sh
git clone https://github.com/stratum-mining/stratum
```
- Navigate to the benches directory.
```sh
cd benches
```
- Open a terminal and run the following command to execute the benchmarks:
```sh
cargo bench
```

The benchmark results will be displayed in the terminal. `target/criterion` and `target/iai` will also be created, which contains more detailed results.

## Benchmarking
It is important to note that for now, sv2 does not count encryption and decryption.

The following benchmark functions are available:

### sv1
1. **Subscription Benchmarks**:
   - `client_sv1_get_subscribe`: Measures the latency and system requirements of a subscription request.
   - `client_sv1_subscribe_serialize`: Measures the latency and system requirements it takes to serialize a subscription message.
   - `client_sv1_subscribe_serialize_deserialize`: Measures the latency and system requirements it takes to serialize and then deserialize a subscription message.
   - `client_sv1_subscribe_serialize_deserialize_handle`: Measures the latency and system requirements it takes to serialize, deserialize, and handle a subscription message.

2. **Authorization Benchmarks**:
   - `client_sv1_get_authorize`: Measures the latency and system requirementsit takes to initiate an authorization request.
   - `client_sv1_authorize_serialize`: Measures the latency and system requirements it takes to serialize an authorization message.
   - `client_sv1_authorize_serialize_deserialize`: Measures the latency and system requirements it takes to serialize and then deserialize an authorization message.
   - `client_sv1_authorize_serialize_deserialize_handle`: Measures the latency and system requirements it takes to serialize, deserialize, and handle an authorization message.

3. **Share Submission Benchmarks**:
   - `client_sv1_get_submit`: Measures the latency and system requirements it takes to submit a share.
   - `client_sv1_submit_serialize`: Measures the latency and system requirements it takes to serialize a share submission message.
   - `client_sv1_submit_serialize_deserialize`: Measures the tlatency and system requirementsime it takes to serialize and then deserialize a share submission message.
   - `client_sv1_submit_serialize_deserialize_handle`: Measures the latency and system requirements it takes to serialize, deserialize, and handle a share submission message.

### sv2 Client

1. **Setup Connection Performance**:
   - `client_sv2_setup_connection`: Measures the latency and system requirements it takes to initiate a setup connection.
   - `client_sv2_setup_connection_serialize`: Measures the latency and system requirements to serialize a setup connection message.
   - `client_sv2_setup_connection_serialize_deserialize`: Measures the latency and system requirements to serialize and deserialize a setup connection message.

2. **Open Channel Performance**:
   - `client_sv2_open_channel`: Measures the latency and system requirements it takes to open a channel.
   - `client_sv2_open_channel_serialize`: Measures the latency and system requirements to serialize an open channel message.
   - `client_sv2_open_channel_serialize_deserialize`: Measures the latency and system requirements to serialize and deserialize an open channel message.

3. **Mining Message Submit Performance**:
   - `client_sv2_mining_message_submit_standard`: Measures the latency and system requirements it takes to submit a standard mining message.
   - `client_sv2_mining_message_submit_standard_serialize`: Measures the latency and system requirements to serialize a standard mining submit message process..
   - `client_sv2_mining_message_submit_standard_serialize_deserialize`:  Measures the latency and system requirements to serialize and deserialize standard mining submit message process.

4. **Handling Message Performance**:
   - `client_sv2_handle_message_mining`: Measures the latency and system requirements to handle a mining message.
   - `client_sv2_handle_message_common`: Measures the latency and system requirements to handle a common message.

## Results

After running the benchmarks, the `criterion` crate will generate detailed performance reports. These reports include statistical measurements such as mean, median, standard deviation, and more. These results can provide insights into the performance characteristics of the sv1 protocol under various scenarios.


### Example Benchmark Results

Here are the sample benchmark results for both sv1 and sv2 client functions:

**Combined Table: Benchmark Metrics**


| Function                                               | Latency (µs) | Instructions | L1 Accesses | L2 Accesses | RAM Accesses | Estimated Cycles |
|--------------------------------------------------------|------------------|--------------|-------------|-------------|--------------|-----------------|
| client-sv1-get-subscribe                              | 1.2064           | 2918         | 4087        | 12          | 123          | 8452            |
| client-sv1-subscribe-serialize                        | 0.7506           | 4275         | 5957        | 17          | 167          | 11887           |
| client-sv1-subscribe-serialize-deserialize            | 1.1619           | 8321         | 11759       | 41          | 337          | 23759           |
| client-sv1-subscribe-serialize-deserialize-handle     | 1.0221           | 9791         | 13867       | 67          | 408          | 28482           |
| client-sv1-get-authorize                              | 0.9172           | 3871         | 5439        | 8           | 104          | 9119            |
| client-sv1-authorize-serialize                        | 0.7965           | 5454         | 7622        | 11          | 150          | 12927           |
| client-sv1-authorize-serialize-deserialize            | 1.4216           | 10094        | 14301       | 37          | 313          | 25441           |
| client-sv1-authorize-serialize-deserialize-handle     | 1.5415           | 12307        | 17451       | 62          | 383          | 31166           |
| client-sv1-get-submit                                 | 2.3447           | 62046        | 89643       | 50          | 306          | 100603          |
| client-submit-serialize                               | 1.3495           | 64107        | 92513       | 54          | 350          | 105033          |
| client-submit-serialize-deserialize                   | 2.2996           | 70767        | 102142      | 71          | 520          | 120697          |
| client-submit-serialize-deserialize-handle            | 3.0982           | 76044        | 109584      | 111         | 636          | 132399          |
| client_sv2_setup_connection                           | 0.2792           | 1665         | 2537        | 14          | 74           | 5197            |
| client_sv2_setup_connection_serialize                 | 1.5738           | 7619         | 11266       | 57          | 247          | 20196           |
| client_sv2_setup_connection_serialize_deserialize     | 3.7460           | 17478        | 25993       | 106         | 461          | 42658           |
| client_sv2_open_channel                               | 0.6050               | 1518       | 2244      | 9            | 65     | 4564 |
| client_sv2_open_channel_serialize                     | 1.0275               | 5746       | 8470      | 39        | 202       | 15735     |
| client_sv2_open_channel_serialize_deserialize         | 1.5892               | 9102       | 13419     | 84        | 373       | 26894     |
| client_sv2_mining_message_submit_standard             | 0.0621           | 2173         | 3190        | 10          | 98           | 6670            |
| client_sv2_mining_message_submit_standard_serialize   | 0.6413           | 5455         | 8049        | 53          | 226          | 16224           |
| client_sv2_mining_message_submit_standard_serialize_deserialize | 2.3043     | 12059        | 17797       | 94          | 380          | 31567           |
| client_sv2_handle_message_common                             | 0.0468           | 389          | 570         | 4           | 31           | 1675            |
| client_sv2_handle_message_mining                     | 0.0778           | 5396         | 7740        | 58          | 211          | 15415           |


Lets Analyze some these results:


**Table 1: Performance Comparison (Setup Process)**

Function                            | Median Time (µs) | Instructions | L1 Accesses | L2 Accesses | RAM Accesses | Estimated Cycles | Performance Index (%)
----------------------------------- | ---------------- | ------------ | ----------- | ----------- | ------------ | ---------------- | -----------------------
client-sv1-get-subscribe            | 1.2064           | 2918         | 4087        | 12          | 123          | 8452             | -
client-sv1-get-authorize            | 0.9172           | 3871         | 5439        | 8           | 104          | 9119             | -
**SV1 Setup Process**               | **2.1236**       | **6798**     | **9526**    | **20**      | **227**      | **17571**        | **-**
client-sv2-setup_connection         | 0.2792           | 1665         | 2537        | 14          | 74           | 5197             | -
client-sv2-open_channel             | 0.6050           | 1518         | 2244        | 9           | 65           | 4564             | -
**SV2 Setup Process**               | **0.8842**       | **3183**     | **4781**    | **23**      | **139**      | **9761**         | **-**

Please note that the "SV1 Setup Process" is the sum of the metrics for `client-sv1-get-subscribe` and `client-sv1-get-authorize`, and the "SV2 Setup Process" is the sum of the metrics for `client-sv2-setup_connection` and `client-sv2-open_channel`.


| Setup Process         | Latency (µs) | Instructions | L1 Accesses | L2 Accesses | RAM Accesses | Estimated Cycles | Latency (%) | L1 Access Improvement (%) | L2 Access Improvement (%) | RAM Access Improvement (%) | Estimated Cycle Improvement (%) | Instruction Cycle Improvement (%) |
|-----------------------|------------------|--------------|-------------|-------------|--------------|-----------------|----------------------|---------------------------|--------------------------|---------------------------|--------------------------------|-----------------------------------|
| SV1 Setup Process     | 2.1236           | 6798         | 9526        | 20          | 227          | 17571           | -                    | -                         | -                        | -                         | -                              | -                                 |
| SV2 Setup Process     | 0.8842           | 3183         | 4781        | 23          | 139          | 9761            | 58.36                | 49.81                     | -15                    | 38.77                     | 44.51                          | 53.12                             |


The Performance Index is calculated using the formula:

```markdown
Performance Index = (SV1 Value - SV2 Value) / SV1 Value * 100

```

**Implications**

- **Latency**: The SV2 connection process demonstrates a significant improvement in latency compared to the SV1 connection process, with an approximately 58.36% improvement. This indicates that SV2's connection process is more efficient and faster.

- **Instructions**: SV2's setup process requires far fewer instructions compared to SV1, resulting in a 53.12% improvement. This implies that SV2's setup process is more streamlined and optimized.

- **L1 Accesses**: SV2's setup process significantly reduces L1 cache accesses compared to SV1, with a 49.81% improvement. This suggests that SV2's setup process is more cache-friendly and reduces memory overhead.

- **L2 Accesses:** Both SV1 and SV2 have similar L2 cache accesses, with sv2 being slightly higher by 20.00%. High L2 access can indicate reduced latency between the CPU and memory, faster data retrieval, and improved overall system performance

- **RAM Accesses**: SV2's setup process drastically reduces RAM accesses compared to SV1, showing a 61.23% improvement. This indicates that SV2's process is designed to minimize main memory interactions.

- **Estimated Cycles**: SV2's setup process requires significantly fewer estimated cycles than SV1, with a 44.51% improvement. This suggests that SV2's setup process is more optimized and requires less computational effort.


**Table 2: Performance Comparison (Submission)**

| Submission                | Median Time (µs) | Instructions | L1 Accesses | L2 Accesses | RAM Accesses | Estimated Cycles | Time Improvement (%) | L1 Access Improvement (%) | L2 Access Improvement (%) | RAM Access Improvement (%) |
|---------------------------|------------------|--------------|-------------|-------------|--------------|-----------------|----------------------|---------------------------|--------------------------|---------------------------|
| SV1 Submission            | 2.3447           | 62046        | 89643       | 50          | 306          | 100603          | -                    | -                         | -                        | -                         |
| SV2 Submission            | 0.0621           | 2173         | 3190        | 10          | 98           | 6670            | 97.35                | 64.33                     | 80.00                    | 67.32                     |


**Implications**

- **Latency**: The SV2 Mining Message Standard demonstrates a remarkable improvement in latency compared to SV1 Get Submit, with an approximately 97.35% reduction. This implies that SV2's mining message submission process is highly efficient and faster.

- **Instructions**: SV2's mining message submission requires far fewer instructions compared to SV1 Get Submit, resulting in a 96.50% improvement. This suggests that SV2's process is more streamlined and requires less computational effort.

- **L1 Accesses**: SV2's mining message submission process significantly reduces L1 cache accesses compared to SV1 Get Submit, showing a 96.43% improvement. This implies that SV2's process minimizes memory overhead and is more cache-friendly.

- **L2 Accesses**: Both SV1 Get Submit and SV2 Mining Message Standard have minimal L2 cache accesses. The slight difference in favor of SV2 (80.00% reduction) is not as significant as other improvements.

- **RAM Accesses**: SV2's mining message submission process significantly reduces RAM accesses compared to SV1 Get Submit, with a 67.32% improvement. This suggests that SV2's process minimizes main memory interactions.

- **Estimated Cycles**: SV2's mining message submission process requires far fewer estimated cycles than SV1 Get Submit, showing a 93.37% improvement. This indicates that SV2's process is highly optimized and requires less computational effort.



## Conclusion

The sv2 client's benchmark results clearly demonstrate its superiority over the sv1 client in terms of performance. These results pave the way for increased adoption of sv2 and highlight its potential to become the preferred choice for mining communication protocols.

---
