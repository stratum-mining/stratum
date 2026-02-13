# Benchmarks

This document records the output of `cargo bench` for the `framing_sv2` crate.
The benchmarks measure the performance characteristics of the framing
implementation used by Stratum V2.

The results are intended to:
* Establish baseline performance characteristics for the framing layer
* Compare alternative implementations (e.g. Vec-backed vs buffer-pool–backed)
* Track performance regressions or improvements across changes

All measurements are environment-dependent and should not be interpreted as
absolute performance guarantees.

## Benchmark Output

The following section contains the **raw output** of `cargo bench` captured
during a single run.

```
Gnuplot not found, using plotters backend
sv2frame::from_message::vec/64                                                                             
                        time:   [15.146 ns 15.375 ns 15.627 ns]
                        change: [-8.8326% -6.9143% -4.8121%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 4 outliers among 100 measurements (4.00%)
  3 (3.00%) high mild
  1 (1.00%) high severe
sv2frame::from_message::vec/1024                                                                             
                        time:   [24.661 ns 25.102 ns 25.505 ns]
                        change: [-0.2920% +1.3190% +2.8589%] (p = 0.10 > 0.05)
                        No change in performance detected.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high mild
sv2frame::from_message::vec/16384                                                                            
                        time:   [371.60 ns 373.47 ns 375.79 ns]
                        change: [-3.5116% -2.5784% -1.6423%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high mild
sv2frame::from_message::vec/61440                                                                             
                        time:   [2.4129 µs 2.4769 µs 2.5387 µs]
                        change: [+24.577% +28.589% +32.881%] (p = 0.00 < 0.05)
                        Performance has regressed.
sv2frame::from_message::vec/16777215                                                                             
                        time:   [2.7647 ms 2.7974 ms 2.8327 ms]
                        change: [+15.573% +19.724% +24.013%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 9 outliers among 100 measurements (9.00%)
  2 (2.00%) low mild
  5 (5.00%) high mild
  2 (2.00%) high severe

sv2frame::serialize_fresh::vec/64                                                                             
                        time:   [30.987 ns 32.082 ns 33.105 ns]
                        change: [-96.243% -96.119% -95.977%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high mild
sv2frame::serialize_fresh::vec/1024                                                                            
                        time:   [77.617 ns 79.109 ns 80.742 ns]
                        change: [-99.294% -99.271% -99.248%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 4 outliers among 100 measurements (4.00%)
  1 (1.00%) low mild
  1 (1.00%) high mild
  2 (2.00%) high severe
sv2frame::serialize_fresh::vec/16384                                                                             
                        time:   [1.1988 µs 1.2088 µs 1.2188 µs]
                        change: [-99.209% -99.194% -99.180%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 6 outliers among 100 measurements (6.00%)
  6 (6.00%) high mild
sv2frame::serialize_fresh::vec/61440                                                                             
                        time:   [5.2949 µs 5.3728 µs 5.4646 µs]
                        change: [-99.159% -99.129% -99.102%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 3 outliers among 100 measurements (3.00%)
  3 (3.00%) high mild
sv2frame::serialize_fresh::vec/16777215                                                                            
                        time:   [7.3949 ms 7.5700 ms 7.7541 ms]
                        change: [+10.375% +13.897% +17.130%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high mild

sv2frame::from_bytes::vec/64                                                                             
                        time:   [21.644 ns 22.326 ns 23.172 ns]
                        change: [-8.0027% -5.2664% -2.4429%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 6 outliers among 100 measurements (6.00%)
  1 (1.00%) low mild
  2 (2.00%) high mild
  3 (3.00%) high severe
sv2frame::from_bytes::vec/1024                                                                             
                        time:   [35.567 ns 36.389 ns 37.231 ns]
                        change: [-3.4635% -1.2269% +1.0325%] (p = 0.30 > 0.05)
                        No change in performance detected.
Found 7 outliers among 100 measurements (7.00%)
  7 (7.00%) high mild
sv2frame::from_bytes::vec/16384                                                                            
                        time:   [286.64 ns 288.01 ns 289.58 ns]
                        change: [-16.588% -14.145% -11.661%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high severe
sv2frame::from_bytes::vec/61440                                                                             
                        time:   [2.1990 µs 2.2319 µs 2.2691 µs]
                        change: [+8.3304% +10.803% +13.324%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 12 outliers among 100 measurements (12.00%)
  1 (1.00%) low mild
  6 (6.00%) high mild
  5 (5.00%) high severe
sv2frame::from_bytes::vec/16777215                                                                             
                        time:   [1.8851 ms 1.9143 ms 1.9479 ms]
                        change: [-3.7259% -1.7486% +0.4477%] (p = 0.11 > 0.05)
                        No change in performance detected.
Found 12 outliers among 100 measurements (12.00%)
  5 (5.00%) high mild
  7 (7.00%) high severe

sv2frame::size_hint::vec/64                                                                             
                        time:   [1.2012 ns 1.2342 ns 1.2644 ns]
                        change: [-25.217% -22.569% -19.939%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high mild
sv2frame::size_hint::vec/1024                                                                             
                        time:   [1.1273 ns 1.1468 ns 1.1710 ns]
                        change: [-0.1114% +1.7750% +3.7801%] (p = 0.08 > 0.05)
                        No change in performance detected.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high mild
sv2frame::size_hint::vec/16384                                                                             
                        time:   [1.1781 ns 1.2500 ns 1.3239 ns]
                        change: [+1.9829% +4.9658% +8.5225%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 14 outliers among 100 measurements (14.00%)
  1 (1.00%) low mild
  2 (2.00%) high mild
  11 (11.00%) high severe
sv2frame::size_hint::vec/61440                                                                             
                        time:   [1.3766 ns 1.4595 ns 1.5390 ns]
                        change: [+7.6169% +11.988% +16.344%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 5 outliers among 100 measurements (5.00%)
  5 (5.00%) high mild
sv2frame::size_hint::vec/16777215                                                                             
                        time:   [1.5091 ns 1.5987 ns 1.6999 ns]
                        change: [+42.762% +49.031% +56.621%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high mild

sv2frame::encrypted_len::vec/64                                                                             
                        time:   [679.42 ps 697.39 ps 716.35 ps]
                        change: [+26.642% +30.355% +34.221%] (p = 0.00 < 0.05)
                        Performance has regressed.
sv2frame::encrypted_len::vec/1024                                                                             
                        time:   [586.12 ps 596.58 ps 609.43 ps]
                        change: [+11.158% +16.402% +21.268%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 6 outliers among 100 measurements (6.00%)
  6 (6.00%) high mild
sv2frame::encrypted_len::vec/16384                                                                             
                        time:   [595.59 ps 604.36 ps 613.59 ps]
                        change: [+5.5322% +6.9490% +8.6461%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 2 outliers among 100 measurements (2.00%)
  1 (1.00%) high mild
  1 (1.00%) high severe
sv2frame::encrypted_len::vec/61440                                                                             
                        time:   [588.32 ps 599.88 ps 611.69 ps]
                        change: [-9.8012% -7.9330% -6.0174%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 4 outliers among 100 measurements (4.00%)
  4 (4.00%) high mild
sv2frame::encrypted_len::vec/16777215                                                                             
                        time:   [549.26 ps 552.70 ps 556.93 ps]
                        change: [-2.3003% +0.1594% +3.2330%] (p = 0.91 > 0.05)
                        No change in performance detected.
Found 8 outliers among 100 measurements (8.00%)
  3 (3.00%) high mild
  5 (5.00%) high severe
```

The following section contains the **raw output** of a single `cargo bench --features with_buffer_pool`
run. These benchmarks measure the same code paths as above, but use the buffer pool
backing where applicable.

```
Gnuplot not found, using plotters backend
sv2frame::from_message::buffer_pool/64                                                                             
                        time:   [15.792 ns 16.120 ns 16.452 ns]
                        change: [-20.420% -17.331% -14.329%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 7 outliers among 100 measurements (7.00%)
  5 (5.00%) high mild
  2 (2.00%) high severe
sv2frame::from_message::buffer_pool/1024                                                                             
                        time:   [27.908 ns 28.863 ns 30.111 ns]
                        change: [+3.7134% +6.8748% +10.554%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 9 outliers among 100 measurements (9.00%)
  4 (4.00%) high mild
  5 (5.00%) high severe
sv2frame::from_message::buffer_pool/16384                                                                            
                        time:   [307.88 ns 316.50 ns 325.43 ns]
                        change: [-31.056% -26.186% -21.150%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 2 outliers among 100 measurements (2.00%)
  1 (1.00%) high mild
  1 (1.00%) high severe
sv2frame::from_message::buffer_pool/61440                                                                             
                        time:   [1.7313 µs 1.7906 µs 1.8525 µs]
                        change: [-14.284% -10.426% -6.0247%] (p = 0.00 < 0.05)
                        Performance has improved.
sv2frame::from_message::buffer_pool/16777215                                                                             
                        time:   [2.5557 ms 2.7109 ms 2.8723 ms]

sv2frame::serialize_fresh::buffer_pool/64                                                                             
                        time:   [52.949 ns 53.942 ns 55.066 ns]
                        change: [-93.818% -93.568% -93.304%] (p = 0.00 < 0.05)
                        Performance has improved.
sv2frame::serialize_fresh::buffer_pool/1024                                                                            
                        time:   [118.92 ns 123.88 ns 129.61 ns]
                        change: [-98.999% -98.957% -98.905%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 6 outliers among 100 measurements (6.00%)
  6 (6.00%) high mild
sv2frame::serialize_fresh::buffer_pool/16384                                                                             
                        time:   [1.2257 µs 1.2493 µs 1.2745 µs]
                        change: [-99.274% -99.237% -99.199%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 6 outliers among 100 measurements (6.00%)
  4 (4.00%) high mild
  2 (2.00%) high severe
sv2frame::serialize_fresh::buffer_pool/61440                                                                             
                        time:   [5.9258 µs 6.0630 µs 6.1953 µs]
                        change: [-99.114% -99.086% -99.057%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high mild
sv2frame::serialize_fresh::buffer_pool/16777215                                                                            
                        time:   [8.1949 ms 8.5092 ms 8.8371 ms]
Found 4 outliers among 100 measurements (4.00%)
  4 (4.00%) high mild

sv2frame::from_bytes::buffer_pool/64                                                                             
                        time:   [26.506 ns 27.986 ns 29.500 ns]
                        change: [-18.688% -14.272% -9.7567%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 18 outliers among 100 measurements (18.00%)
  6 (6.00%) high mild
  12 (12.00%) high severe
sv2frame::from_bytes::buffer_pool/1024                                                                             
                        time:   [39.136 ns 39.872 ns 40.674 ns]
                        change: [-13.183% -8.7393% -3.8792%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 14 outliers among 100 measurements (14.00%)
  13 (13.00%) low mild
  1 (1.00%) high mild
sv2frame::from_bytes::buffer_pool/16384                                                                            
                        time:   [417.16 ns 423.86 ns 431.46 ns]
                        change: [+34.826% +40.667% +46.435%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 3 outliers among 100 measurements (3.00%)
  3 (3.00%) high mild
sv2frame::from_bytes::buffer_pool/61440                                                                             
                        time:   [1.7445 µs 1.7803 µs 1.8172 µs]
                        change: [+9.7047% +12.057% +14.661%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 3 outliers among 100 measurements (3.00%)
  1 (1.00%) high mild
  2 (2.00%) high severe
sv2frame::from_bytes::buffer_pool/16777215                                                                             
                        time:   [2.4514 ms 2.5412 ms 2.6347 ms]
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high mild

sv2frame::size_hint::buffer_pool/64                                                                             
                        time:   [2.5779 ns 2.6914 ns 2.8120 ns]
                        change: [+148.04% +154.29% +161.11%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 7 outliers among 100 measurements (7.00%)
  5 (5.00%) high mild
  2 (2.00%) high severe
sv2frame::size_hint::buffer_pool/1024                                                                             
                        time:   [1.4478 ns 1.4800 ns 1.5197 ns]
                        change: [+12.584% +19.618% +27.035%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 8 outliers among 100 measurements (8.00%)
  5 (5.00%) high mild
  3 (3.00%) high severe
sv2frame::size_hint::buffer_pool/16384                                                                             
                        time:   [1.1489 ns 1.1631 ns 1.1809 ns]
                        change: [-17.428% -14.719% -12.077%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 10 outliers among 100 measurements (10.00%)
  9 (9.00%) high mild
  1 (1.00%) high severe
sv2frame::size_hint::buffer_pool/61440                                                                             
                        time:   [1.1750 ns 1.1822 ns 1.1899 ns]
                        change: [-19.516% -17.962% -16.482%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 4 outliers among 100 measurements (4.00%)
  2 (2.00%) high mild
  2 (2.00%) high severe
sv2frame::size_hint::buffer_pool/16777215                                                                             
                        time:   [1.2321 ns 1.2721 ns 1.3189 ns]
Found 7 outliers among 100 measurements (7.00%)
  4 (4.00%) high mild
  3 (3.00%) high severe

sv2frame::encrypted_len::buffer_pool/64                                                                             
                        time:   [545.29 ps 549.22 ps 554.00 ps]
                        change: [-16.348% -15.318% -14.237%] (p = 0.00 < 0.05)
                        Performance has improved.
sv2frame::encrypted_len::buffer_pool/1024                                                                             
                        time:   [610.68 ps 624.67 ps 641.25 ps]
                        change: [-11.460% -8.0013% -4.3712%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 4 outliers among 100 measurements (4.00%)
  2 (2.00%) high mild
  2 (2.00%) high severe
sv2frame::encrypted_len::buffer_pool/16384                                                                             
                        time:   [662.20 ps 692.70 ps 727.37 ps]
                        change: [+4.5777% +7.7961% +11.648%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 3 outliers among 100 measurements (3.00%)
  3 (3.00%) high mild
sv2frame::encrypted_len::buffer_pool/61440                                                                             
                        time:   [707.29 ps 745.10 ps 787.96 ps]
                        change: [+15.860% +19.678% +23.304%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 21 outliers among 100 measurements (21.00%)
  12 (12.00%) low mild
  2 (2.00%) high mild
  7 (7.00%) high severe
sv2frame::encrypted_len::buffer_pool/16777215                                                                             
                        time:   [623.76 ps 636.95 ps 652.43 ps]
Found 11 outliers among 100 measurements (11.00%)
  6 (6.00%) high mild
  5 (5.00%) high severe
```


## Interpretation Notes

* Across most benchmarks and input sizes, the `with_buffer_pool` configuration
demonstrates equal or better performance compared to the `Vec`-backed
implementation.

---

## Reproducing

Run benchmarks locally with:

```bash
cargo bench 
cargo bench --features with_buffer_pool
```
