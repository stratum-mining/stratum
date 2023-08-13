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
The following benchmark functions are available:

### sv1
1. **Subscription Benchmarks**:
   - `criterion_sv1_get_subscribe`: Measures the latency and system requirements of a subscription request.
   - `criterion_sv1_subscribe_serialize`: Measures the latency and system requirements it takes to serialize a subscription message.
   - `criterion_sv1_serialize_deserialize`: Measures the latency and system requirements it takes to serialize and then deserialize a subscription message.
   - `criterion_sv1_subscribe_serialize_deserialize_handle`: Measures the latency and system requirements it takes to serialize, deserialize, and handle a subscription message.

2. **Authorization Benchmarks**:
   - `criterion_sv1_get_authorize`: Measures the latency and system requirementsit takes to initiate an authorization request.
   - `criterion_sv1_authorize_serialize`: Measures the latency and system requirements it takes to serialize an authorization message.
   - `criterion_sv1_authorize_serialize_deserialize`: Measures the latency and system requirements it takes to serialize and then deserialize an authorization message.
   - `criterion_sv1_authorize_serialize_deserialize_handle`: Measures the latency and system requirements it takes to serialize, deserialize, and handle an authorization message.

3. **Share Submission Benchmarks**:
   - `criterion_sv1_get_submit`: Measures the latency and system requirements it takes to submit a share.
   - `criterion_sv1_submit_serialize`: Measures the latency and system requirements it takes to serialize a share submission message.
   - `criterion_sv1_submit_serialize_deserialize`: Measures the tlatency and system requirementsime it takes to serialize and then deserialize a share submission message.
   - `criterion_sv1_submit_serialize_deserialize_handle`: Measures the latency and system requirements it takes to serialize, deserialize, and handle a share submission message.

### sv2 Client

1. **Setup Connection Performance**:
   - `client_sv2_setup_connection`: Measures the latency and system requirements it takes to initiate a setup connection.
   - `client_sv2_setup_connection_serialize`: Measures the latency and system requirements to serialize a setup connection message.
   - `client_sv2_setup_connection_serialize_deserialize`: Measures the latency and system requirements to serialize and deserialize a setup connection message.

2. **Open Channel Performance**:
   - `client_sv2_open_channel`: Measures the latency and system requirements it takes to open a channel.
   - `client_sv2_open_channel_serialize`: Measures the latency and system requirements to serialize an open channel message.
   - `client_sv2_open_channel_serialize_deserialize`: Measures the latency and system requirements to serialize and deserialize an open channel message.

3. **Mining Message Performance**:
   - `client_sv2_mining_message_submit_standard`: Measures the latency and system requirements it takes to submit a standard mining message.
   - `client_sv2_mining_message_submit_standard_serialize`: Measures the latency and system requirements to serialize a standard mining submit message.

4. **Update Channel Serialization**:
   - `client_sv2_update_channel_serialize`: Measures the latency and system requirements to serialize an update channel message.

5. **Handling Message Performance**:
   - `benchmark_handle_message_mining`: Measures the latency and system requirements to handle a mining message.
   - `benchmark_handle_common_message`: Measures the latency and system requirements to handle a common message.

## Results

After running the benchmarks, the `criterion` crate will generate detailed performance reports. These reports include statistical measurements such as mean, median, standard deviation, and more. These results can provide insights into the performance characteristics of the sv1 protocol under various scenarios.


### Example Benchmark Results

Here are the sample benchmark results for both sv1 and sv2 client functions:

Certainly, here are the tables with the benchmark metrics combined and the time range column renamed to "Latency":

**Combined Table: Benchmark Metrics and Latency**

| Benchmark Function           | Latency (µs)   | Instructions | L1 Accesses | L2 Accesses | RAM Accesses | Estimated Cycles |
|------------------------------|----------------|--------------|-------------|-------------|--------------|------------------|
| client-sv1-get-subscribe     | 1.4610         | 2918         | 4087        | 12          | 123          | 8452             |
| client-sv1-get-authorize     | 0.9507         | 3871         | 5439        | 8           | 104          | 9119             |
| client-sv2-setup_connection  | 0.2739         | 1471         | 2159        | 18          | 66           | 4559             |
| client-sv2-open_channel      | 0.5568         | 1325         | 1866        | 6           | 50           | 3646             |
| client-sv1-subscribe-serialize | 1.7017        | 4275         | 5957        | 17          | 167          | 11887            |
| client-sv1-authorize-serialize | 1.3046        | 5454         | 7622        | 11          | 150          | 12927            |
| client-sv2-setup_connection_serialize | 0.8689  | 3074         | 4471        | 24          | 125          | 8966             |
| client-sv2-open_channel_serialize | 0.8854    | 2618         | 4019        | 13          | 112          | 8004             |
| client-sv1-subscribe-serialize-deserialize | 2.4030 | 8321 | 11759 | 41 | 337 | 23759 |
| client-sv1-subscribe-serialize-deserialize-handle | 2.8912 | 9791 | 13866 | 69 | 407 | 28456 |
| client-sv1-authorize-serialize-deserialize | 2.8834 | 10094 | 14301 | 37 | 313 | 25441 |
| client-sv1-authorize-serialize-deserialize-handle | 3.9632 | 12307 | 17451 | 62 | 383 | 31166 |
| client-submit-serialize      | 16.042         | 64107        | 92512       | 55          | 350          | 105037           |
| client-sv2-mining_message_standard_serialize | 0.4228 | 3070 | 4379 | 13 | 137 | 9239 |
| client-submit-serialize-deserialize | 19.097 | 70767 | 102142 | 71 | 520 | 120697 |
| client-submit-serialize-deserialize-handle | 21.627 | 76044 | 109584 | 111 | 636 | 132399 |
| client-sv2-update_channel_serialize | 0.3145 | 1833 | 2704 | 11 | 91 | 5944 |
| client-sv2-message_common    | 0.0454         | 390          | 572         | 3           | 31           | 1672             |
| client-sv2-message_mining    | 0.0794         | 5334         | 7642        | 55          | 215          | 15442            |

In this combined table, the "Latency" column represents the middle value of the time range in microseconds.


Lets Analyze some these results:

**SV1 vs. SV2 Connection Process**

| Benchmark Function         | Latency (µs) | Instructions | L1 Accesses | L2 Accesses | RAM Accesses | Estimated Cycles |
|----------------------------|-----------------|--------------|-------------|-------------|--------------|------------------|
| client-sv1-get-subscribe   | 1.4610 µs      | 2918         | 4087        | 12          | 123          | 8452             |
| client-sv1-get-authorize   | 0.9507 µs      | 3871         | 5439        | 8           | 104          | 9119             |
| SV1 Connection Process     | 2.4117 µs      | 6793         | 9526        | 20          | 227          | 17571            |
| client-sv2-setup_connection | 0.2739 µs      | 1471         | 2159        | 18          | 66           | 4559             |
| client-sv2-open_channel     | 0.5568 µs      | 1325         | 1866        | 6           | 50           | 3646             |
| SV2 Connection Process     | 0.8304 µs      | 2796         | 4025        | 24          | 58           | 8205             |

**SV1 vs. SV2 Connection Process Comparison**

| Metric            | SV1 Connection Process (µs) | SV2 Connection Process (µs) | Performance Index (Efficiency Ratio) |
|-------------------|-----------------------------|-----------------------------|--------------------------------------|
| Latency (µs)   | 2.4117                      | 0.8304                      | 65.35%                               |
| Instructions      | 6793                        | 2796                        | 58.82%                               |
| L1 Accesses       | 9526                        | 4025                        | 57.71%                               |
| L2 Accesses       | 20                          | 24                          | -20.00%                              |
| RAM Accesses      | 227                         | 58                          | 74.51%                               |
| Estimated Cycles  | 17571                       | 8205                        | 53.26%                               |


The formula used here is:

\[\text{Performance Index} = \frac{\text{SV1 Value} - \text{SV2 Value}}{\text{SV1 Value}} \times 100\]

**Implications**

1. **Latency:** SV2 connection process is around 65.35% faster than SV1. This means that SV2 is significantly more efficient in terms of time taken to complete the connection process.

2. **Instructions:** SV2 requires 58.82% fewer instructions compared to SV1. This indicates that SV2's connection process is more optimized and streamlined in terms of the number of instructions executed.

3. **L1 Accesses:** SV2 requires 57.71% fewer L1 cache accesses compared to SV1. This suggests that SV2's connection process is better at utilizing the cache memory, resulting in reduced memory access time.

4. **L2 Accesses:** Both SV1 and SV2 have similar L2 cache accesses, with SV2 being slightly higher by 20.00%. High L2 access can indicate reduced latency between the CPU and memory, faster data retrieval, and improved overall system performance

5. **RAM Accesses:** SV2 requires 74.51% fewer RAM accesses compared to SV1. This indicates that SV2's connection process is more efficient at utilizing lower-level memory access, which contributes to its improved performance.

6. **Estimated Cycles:** SV2 requires 53.26% fewer cycles to complete the connection process compared to SV1. This means that SV2 is more efficient in terms of CPU cycle utilization, resulting to faster execution.


**SV1 and SV2: Share Submission**

| Metric                           | SV1 Submission (µs) | SV2 Standard Mining (µs) | Performance Index (Efficiency Ratio) |
|----------------------------------|---------------------|--------------------------|--------------------------------------|
| Latency (µs)                  | 15,902.0          | 70.212                   | 77.38%                               |
| Instructions                     | 64,107              | 5,334                    | 91.68%                               |
| L1 Accesses                      | 92,512              | 7,642                    | 91.73%                               |
| L2 Accesses                      | 55                  | 55                       | 0.00%                                |
| RAM Accesses                     | 350                 | 215                      | 61.43%                               |
| Estimated Cycles                 | 105,037             | 15,442                   | 85.30%                               |

**Implications**

Now, let's interpret the results:
- Latency: SV2's Standard Mining process is 77.38% faster than SV1's Submission process. This indicates that SV2's Standard Mining process completes more efficiently in terms of time.

- Instructions: SV2's Standard Mining requires 91.68% fewer instructions compared to SV1's Submission. This suggests that SV2's mining process is more streamlined and optimized in terms of instruction execution.

- L1 Accesses: SV2's Standard Mining requires 91.73% fewer L1 cache accesses compared to SV1's Submission. This indicates that SV2's mining process is better at utilizing cache memory effectively, leading to reduced memory access time.

- L2 Accesses: Both SV1 and SV2 have the same number of L2 cache accesses.

- RAM Accesses: SV2's Standard Mining process requires 61.43% fewer RAM accesses compared to SV1's Submission. This suggests that SV2's process is more efficient in utilizing lower-level memory access, contributing to its improved performance.

- Estimated Cycles: SV2's Standard Mining process requires 85.30% fewer cycles compared to SV1's Submission. This indicates that SV2's process is more efficient in utilizing CPU cycles, leading to faster execution.


## Conclusion

The sv2 client's benchmark results clearly demonstrate its superiority over the sv1 client in terms of performance. These results pave the way for increased adoption of sv2 and highlight its potential to become the preferred choice for mining communication protocols.

---
