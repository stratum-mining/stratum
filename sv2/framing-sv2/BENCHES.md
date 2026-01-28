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
Benchmarking sv2frame::from_message::vec/64
Benchmarking sv2frame::from_message::vec/64: Warming up for 3.0000 s
Benchmarking sv2frame::from_message::vec/64: Collecting 100 samples in estimated 5.0000 s (287M iterations)
Benchmarking sv2frame::from_message::vec/64: Analyzing
sv2frame::from_message::vec/64
                        time:   [16.005 ns 16.121 ns 16.265 ns]
                        change: [-16.906% -16.318% -15.726%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 12 outliers among 100 measurements (12.00%)
  9 (9.00%) high mild
  3 (3.00%) high severe
Benchmarking sv2frame::from_message::vec/1024
Benchmarking sv2frame::from_message::vec/1024: Warming up for 3.0000 s
Benchmarking sv2frame::from_message::vec/1024: Collecting 100 samples in estimated 5.0001 s (188M iterations)
Benchmarking sv2frame::from_message::vec/1024: Analyzing
sv2frame::from_message::vec/1024
                        time:   [28.787 ns 29.372 ns 29.991 ns]
                        change: [-4.7365% -1.6103% +1.6359%] (p = 0.31 > 0.05)
                        No change in performance detected.
Benchmarking sv2frame::from_message::vec/16384
Benchmarking sv2frame::from_message::vec/16384: Warming up for 3.0000 s
Benchmarking sv2frame::from_message::vec/16384: Collecting 100 samples in estimated 5.0011 s (14M iterations)
Benchmarking sv2frame::from_message::vec/16384: Analyzing
sv2frame::from_message::vec/16384
                        time:   [302.67 ns 303.85 ns 305.28 ns]
                        change: [+6.6514% +8.4153% +10.056%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high mild
Benchmarking sv2frame::from_message::vec/61440
Benchmarking sv2frame::from_message::vec/61440: Warming up for 3.0000 s
Benchmarking sv2frame::from_message::vec/61440: Collecting 100 samples in estimated 5.0019 s (4.0M iterations)
Benchmarking sv2frame::from_message::vec/61440: Analyzing
sv2frame::from_message::vec/61440
                        time:   [1.2282 µs 1.2348 µs 1.2424 µs]
                        change: [-3.7041% -2.6819% -1.6447%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 13 outliers among 100 measurements (13.00%)
  7 (7.00%) high mild
  6 (6.00%) high severe

Benchmarking sv2frame::serialize_fresh::vec/64
Benchmarking sv2frame::serialize_fresh::vec/64: Warming up for 3.0000 s
Benchmarking sv2frame::serialize_fresh::vec/64: Collecting 100 samples in estimated 5.0008 s (6.2M iterations)
Benchmarking sv2frame::serialize_fresh::vec/64: Analyzing
sv2frame::serialize_fresh::vec/64
                        time:   [760.34 ns 765.33 ns 770.40 ns]
                        change: [-11.303% -8.2892% -5.2405%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high severe
Benchmarking sv2frame::serialize_fresh::vec/1024
Benchmarking sv2frame::serialize_fresh::vec/1024: Warming up for 3.0000 s
Benchmarking sv2frame::serialize_fresh::vec/1024: Collecting 100 samples in estimated 5.0275 s (414k iterations)
Benchmarking sv2frame::serialize_fresh::vec/1024: Analyzing
sv2frame::serialize_fresh::vec/1024
                        time:   [11.598 µs 12.017 µs 12.490 µs]
                        change: [-12.712% -9.6323% -6.3808%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 7 outliers among 100 measurements (7.00%)
  5 (5.00%) high mild
  2 (2.00%) high severe
Benchmarking sv2frame::serialize_fresh::vec/16384
Benchmarking sv2frame::serialize_fresh::vec/16384: Warming up for 3.0000 s
Benchmarking sv2frame::serialize_fresh::vec/16384: Collecting 100 samples in estimated 5.1514 s (25k iterations)
Benchmarking sv2frame::serialize_fresh::vec/16384: Analyzing
sv2frame::serialize_fresh::vec/16384
                        time:   [233.73 µs 247.32 µs 263.40 µs]
                        change: [+35.181% +41.317% +48.858%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 6 outliers among 100 measurements (6.00%)
  2 (2.00%) high mild
  4 (4.00%) high severe
Benchmarking sv2frame::serialize_fresh::vec/61440
Benchmarking sv2frame::serialize_fresh::vec/61440: Warming up for 3.0000 s
Benchmarking sv2frame::serialize_fresh::vec/61440: Collecting 100 samples in estimated 8.5863 s (10k iterations)
Benchmarking sv2frame::serialize_fresh::vec/61440: Analyzing
sv2frame::serialize_fresh::vec/61440
                        time:   [722.40 µs 728.72 µs 735.26 µs]
                        change: [+1.9021% +4.2198% +6.6092%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 5 outliers among 100 measurements (5.00%)
  4 (4.00%) high mild
  1 (1.00%) high severe

Benchmarking sv2frame::serialize_fast::vec/64
Benchmarking sv2frame::serialize_fast::vec/64: Warming up for 3.0000 s
Benchmarking sv2frame::serialize_fast::vec/64: Collecting 100 samples in estimated 5.0048 s (5.2M iterations)
Benchmarking sv2frame::serialize_fast::vec/64: Analyzing
sv2frame::serialize_fast::vec/64
                        time:   [923.88 ns 946.66 ns 973.58 ns]
                        change: [+11.849% +14.879% +17.844%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high mild
Benchmarking sv2frame::serialize_fast::vec/1024
Benchmarking sv2frame::serialize_fast::vec/1024: Warming up for 3.0000 s
Benchmarking sv2frame::serialize_fast::vec/1024: Collecting 100 samples in estimated 5.0121 s (434k iterations)
Benchmarking sv2frame::serialize_fast::vec/1024: Analyzing
sv2frame::serialize_fast::vec/1024
                        time:   [11.568 µs 11.788 µs 12.042 µs]
                        change: [+14.563% +17.139% +19.869%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 5 outliers among 100 measurements (5.00%)
  3 (3.00%) high mild
  2 (2.00%) high severe
Benchmarking sv2frame::serialize_fast::vec/16384
Benchmarking sv2frame::serialize_fast::vec/16384: Warming up for 3.0000 s
Benchmarking sv2frame::serialize_fast::vec/16384: Collecting 100 samples in estimated 5.3221 s (25k iterations)
Benchmarking sv2frame::serialize_fast::vec/16384: Analyzing
sv2frame::serialize_fast::vec/16384
                        time:   [173.56 µs 175.45 µs 177.36 µs]
                        change: [-14.689% -10.709% -6.6970%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 5 outliers among 100 measurements (5.00%)
  4 (4.00%) high mild
  1 (1.00%) high severe
Benchmarking sv2frame::serialize_fast::vec/61440
Benchmarking sv2frame::serialize_fast::vec/61440: Warming up for 3.0000 s
Benchmarking sv2frame::serialize_fast::vec/61440: Collecting 100 samples in estimated 6.8972 s (10k iterations)
Benchmarking sv2frame::serialize_fast::vec/61440: Analyzing
sv2frame::serialize_fast::vec/61440
                        time:   [739.68 µs 760.57 µs 785.10 µs]
                        change: [-10.022% -4.8557% +0.5843%] (p = 0.09 > 0.05)
                        No change in performance detected.
Found 8 outliers among 100 measurements (8.00%)
  4 (4.00%) high mild
  4 (4.00%) high severe

Benchmarking sv2frame::from_bytes::vec/64
Benchmarking sv2frame::from_bytes::vec/64: Warming up for 3.0000 s
Benchmarking sv2frame::from_bytes::vec/64: Collecting 100 samples in estimated 5.0001 s (158M iterations)
Benchmarking sv2frame::from_bytes::vec/64: Analyzing
sv2frame::from_bytes::vec/64
                        time:   [32.907 ns 33.383 ns 33.851 ns]
                        change: [+22.600% +27.073% +32.014%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 5 outliers among 100 measurements (5.00%)
  4 (4.00%) high mild
  1 (1.00%) high severe
Benchmarking sv2frame::from_bytes::vec/1024
Benchmarking sv2frame::from_bytes::vec/1024: Warming up for 3.0000 s
Benchmarking sv2frame::from_bytes::vec/1024: Collecting 100 samples in estimated 5.0001 s (110M iterations)
Benchmarking sv2frame::from_bytes::vec/1024: Analyzing
sv2frame::from_bytes::vec/1024
                        time:   [44.017 ns 45.110 ns 46.519 ns]
                        change: [+30.710% +32.800% +35.130%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 8 outliers among 100 measurements (8.00%)
  3 (3.00%) high mild
  5 (5.00%) high severe
Benchmarking sv2frame::from_bytes::vec/16384
Benchmarking sv2frame::from_bytes::vec/16384: Warming up for 3.0000 s
Benchmarking sv2frame::from_bytes::vec/16384: Collecting 100 samples in estimated 5.0018 s (13M iterations)
Benchmarking sv2frame::from_bytes::vec/16384: Analyzing
sv2frame::from_bytes::vec/16384
                        time:   [386.56 ns 393.41 ns 401.73 ns]
                        change: [+34.640% +38.522% +42.086%] (p = 0.00 < 0.05)
                        Performance has regressed.
Benchmarking sv2frame::from_bytes::vec/61440
Benchmarking sv2frame::from_bytes::vec/61440: Warming up for 3.0000 s
Benchmarking sv2frame::from_bytes::vec/61440: Collecting 100 samples in estimated 5.0076 s (2.8M iterations)
Benchmarking sv2frame::from_bytes::vec/61440: Analyzing
sv2frame::from_bytes::vec/61440
                        time:   [1.5122 µs 1.5292 µs 1.5513 µs]
                        change: [+29.858% +32.942% +35.963%] (p = 0.00 < 0.05)
                        Performance has regressed.

Benchmarking sv2frame::size_hint::vec/64
Benchmarking sv2frame::size_hint::vec/64: Warming up for 3.0000 s
Benchmarking sv2frame::size_hint::vec/64: Collecting 100 samples in estimated 5.0000 s (214M iterations)
Benchmarking sv2frame::size_hint::vec/64: Analyzing
sv2frame::size_hint::vec/64
                        time:   [24.066 ns 24.975 ns 25.974 ns]
                        change: [+27.532% +32.184% +36.712%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 4 outliers among 100 measurements (4.00%)
  4 (4.00%) high mild
Benchmarking sv2frame::size_hint::vec/1024
Benchmarking sv2frame::size_hint::vec/1024: Warming up for 3.0000 s
Benchmarking sv2frame::size_hint::vec/1024: Collecting 100 samples in estimated 5.0001 s (147M iterations)
Benchmarking sv2frame::size_hint::vec/1024: Analyzing
sv2frame::size_hint::vec/1024
                        time:   [34.975 ns 35.922 ns 37.020 ns]
                        change: [+54.081% +58.035% +62.203%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 5 outliers among 100 measurements (5.00%)
  4 (4.00%) high mild
  1 (1.00%) high severe
Benchmarking sv2frame::size_hint::vec/16384
Benchmarking sv2frame::size_hint::vec/16384: Warming up for 3.0000 s
Benchmarking sv2frame::size_hint::vec/16384: Collecting 100 samples in estimated 5.0013 s (13M iterations)
Benchmarking sv2frame::size_hint::vec/16384: Analyzing
sv2frame::size_hint::vec/16384
                        time:   [384.60 ns 395.88 ns 408.20 ns]
                        change: [+21.151% +25.173% +29.023%] (p = 0.00 < 0.05)
                        Performance has regressed.
Benchmarking sv2frame::size_hint::vec/61440
Benchmarking sv2frame::size_hint::vec/61440: Warming up for 3.0000 s
Benchmarking sv2frame::size_hint::vec/61440: Collecting 100 samples in estimated 5.0021 s (3.4M iterations)
Benchmarking sv2frame::size_hint::vec/61440: Analyzing
sv2frame::size_hint::vec/61440
                        time:   [1.3370 µs 1.3466 µs 1.3563 µs]
                        change: [-14.299% -11.641% -8.9229%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high severe

Benchmarking sv2frame::encrypted_len::vec/64
Benchmarking sv2frame::encrypted_len::vec/64: Warming up for 3.0000 s
Benchmarking sv2frame::encrypted_len::vec/64: Collecting 100 samples in estimated 5.0000 s (8.1B iterations)
Benchmarking sv2frame::encrypted_len::vec/64: Analyzing
sv2frame::encrypted_len::vec/64
                        time:   [571.64 ps 577.20 ps 583.02 ps]
                        change: [+4.1448% +5.4648% +6.4955%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 26 outliers among 100 measurements (26.00%)
  11 (11.00%) low severe
  2 (2.00%) low mild
  6 (6.00%) high mild
  7 (7.00%) high severe
Benchmarking sv2frame::encrypted_len::vec/1024
Benchmarking sv2frame::encrypted_len::vec/1024: Warming up for 3.0000 s
Benchmarking sv2frame::encrypted_len::vec/1024: Collecting 100 samples in estimated 5.0000 s (8.6B iterations)
Benchmarking sv2frame::encrypted_len::vec/1024: Analyzing
sv2frame::encrypted_len::vec/1024
                        time:   [555.96 ps 558.76 ps 561.84 ps]
                        change: [+1.9104% +2.9838% +3.8808%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 5 outliers among 100 measurements (5.00%)
  1 (1.00%) low mild
  4 (4.00%) high mild
Benchmarking sv2frame::encrypted_len::vec/16384
Benchmarking sv2frame::encrypted_len::vec/16384: Warming up for 3.0000 s
Benchmarking sv2frame::encrypted_len::vec/16384: Collecting 100 samples in estimated 5.0000 s (8.7B iterations)
Benchmarking sv2frame::encrypted_len::vec/16384: Analyzing
sv2frame::encrypted_len::vec/16384
                        time:   [701.21 ps 748.92 ps 799.06 ps]
                        change: [+1.8786% +7.6796% +15.316%] (p = 0.02 < 0.05)
                        Performance has regressed.
Found 5 outliers among 100 measurements (5.00%)
  4 (4.00%) high mild
  1 (1.00%) high severe
Benchmarking sv2frame::encrypted_len::vec/61440
Benchmarking sv2frame::encrypted_len::vec/61440: Warming up for 3.0000 s
Benchmarking sv2frame::encrypted_len::vec/61440: Collecting 100 samples in estimated 5.0000 s (6.8B iterations)
Benchmarking sv2frame::encrypted_len::vec/61440: Analyzing
sv2frame::encrypted_len::vec/61440
                        time:   [670.90 ps 682.83 ps 695.66 ps]
                        change: [-13.774% -11.133% -8.6696%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high mild

```

The following section contains the **raw output** of a single `cargo bench --features with_buffer_pool`
run. These benchmarks measure the same code paths as above, but use the buffer pool
backing where applicable.

```
nuplot not found, using plotters backend
sv2frame::from_message::buffer_pool/64                                                                             
                        time:   [16.161 ns 16.200 ns 16.243 ns]
                        change: [-10.609% -8.7663% -6.2835%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 14 outliers among 100 measurements (14.00%)
  5 (5.00%) high mild
  9 (9.00%) high severe
sv2frame::from_message::buffer_pool/1024                                                                             
                        time:   [24.781 ns 24.872 ns 24.992 ns]
                        change: [-13.348% -12.242% -10.817%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 13 outliers among 100 measurements (13.00%)
  3 (3.00%) high mild
  10 (10.00%) high severe
sv2frame::from_message::buffer_pool/16384                                                                            
                        time:   [290.95 ns 292.23 ns 293.81 ns]
                        change: [+4.2996% +5.1349% +6.0328%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 3 outliers among 100 measurements (3.00%)
  2 (2.00%) high mild
  1 (1.00%) high severe
sv2frame::from_message::buffer_pool/61440                                                                             
                        time:   [1.2221 µs 1.2288 µs 1.2364 µs]
                        change: [-10.077% -8.6335% -7.1456%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 3 outliers among 100 measurements (3.00%)
  2 (2.00%) high mild
  1 (1.00%) high severe

sv2frame::serialize_fresh::buffer_pool/64                                                                             
                        time:   [741.85 ns 743.63 ns 745.69 ns]
Found 3 outliers among 100 measurements (3.00%)
  3 (3.00%) high mild
sv2frame::serialize_fresh::buffer_pool/1024                                                                             
                        time:   [9.5345 µs 9.5696 µs 9.6102 µs]
Found 6 outliers among 100 measurements (6.00%)
  4 (4.00%) high mild
  2 (2.00%) high severe
sv2frame::serialize_fresh::buffer_pool/16384                                                                            
                        time:   [152.06 µs 152.75 µs 153.47 µs]
Found 5 outliers among 100 measurements (5.00%)
  1 (1.00%) low mild
  2 (2.00%) high mild
  2 (2.00%) high severe
sv2frame::serialize_fresh::buffer_pool/61440                                                                            
                        time:   [562.49 µs 566.43 µs 571.82 µs]
Found 13 outliers among 100 measurements (13.00%)
  9 (9.00%) high mild
  4 (4.00%) high severe

sv2frame::serialize_fast::buffer_pool/64                                                                             
                        time:   [730.01 ns 731.62 ns 733.12 ns]
Found 3 outliers among 100 measurements (3.00%)
  2 (2.00%) high mild
  1 (1.00%) high severe
sv2frame::serialize_fast::buffer_pool/1024                                                                             
                        time:   [9.4794 µs 9.5278 µs 9.5792 µs]
Found 5 outliers among 100 measurements (5.00%)
  4 (4.00%) high mild
  1 (1.00%) high severe
sv2frame::serialize_fast::buffer_pool/16384                                                                            
                        time:   [157.47 µs 159.99 µs 163.14 µs]
Found 4 outliers among 100 measurements (4.00%)
  2 (2.00%) high mild
  2 (2.00%) high severe
sv2frame::serialize_fast::buffer_pool/61440                                                                            
                        time:   [562.69 µs 564.76 µs 566.82 µs]
Found 14 outliers among 100 measurements (14.00%)
  7 (7.00%) low mild
  2 (2.00%) high mild
  5 (5.00%) high severe

sv2frame::from_bytes::buffer_pool/64                                                                             
                        time:   [24.148 ns 24.221 ns 24.293 ns]
Found 34 outliers among 100 measurements (34.00%)
  11 (11.00%) low severe
  3 (3.00%) low mild
  8 (8.00%) high mild
  12 (12.00%) high severe
sv2frame::from_bytes::buffer_pool/1024                                                                             
                        time:   [34.789 ns 34.856 ns 34.914 ns]
Found 23 outliers among 100 measurements (23.00%)
  10 (10.00%) low severe
  4 (4.00%) low mild
  2 (2.00%) high mild
  7 (7.00%) high severe
sv2frame::from_bytes::buffer_pool/16384                                                                            
                        time:   [295.43 ns 296.78 ns 298.58 ns]
Found 5 outliers among 100 measurements (5.00%)
  2 (2.00%) high mild
  3 (3.00%) high severe
sv2frame::from_bytes::buffer_pool/61440                                                                             
                        time:   [1.2171 µs 1.2245 µs 1.2323 µs]
Found 4 outliers among 100 measurements (4.00%)
  1 (1.00%) high mild
  3 (3.00%) high severe

sv2frame::size_hint::buffer_pool/64                                                                             
                        time:   [18.300 ns 18.366 ns 18.433 ns]
Found 3 outliers among 100 measurements (3.00%)
  3 (3.00%) high severe
sv2frame::size_hint::buffer_pool/1024                                                                             
                        time:   [23.320 ns 23.450 ns 23.604 ns]
Found 37 outliers among 100 measurements (37.00%)
  13 (13.00%) low severe
  7 (7.00%) low mild
  6 (6.00%) high mild
  11 (11.00%) high severe
sv2frame::size_hint::buffer_pool/16384                                                                            
                        time:   [277.95 ns 279.09 ns 280.27 ns]
Found 4 outliers among 100 measurements (4.00%)
  1 (1.00%) high mild
  3 (3.00%) high severe
sv2frame::size_hint::buffer_pool/61440                                                                             
                        time:   [1.2804 µs 1.2910 µs 1.3026 µs]
Found 5 outliers among 100 measurements (5.00%)
  2 (2.00%) high mild
  3 (3.00%) high severe

sv2frame::encrypted_len::buffer_pool/64                                                                             
                        time:   [550.76 ps 552.71 ps 554.66 ps]
Found 4 outliers among 100 measurements (4.00%)
  1 (1.00%) low mild
  3 (3.00%) high mild
sv2frame::encrypted_len::buffer_pool/1024                                                                             
                        time:   [575.54 ps 577.51 ps 579.45 ps]
Found 3 outliers among 100 measurements (3.00%)
  1 (1.00%) low mild
  1 (1.00%) high mild
  1 (1.00%) high severe
sv2frame::encrypted_len::buffer_pool/16384                                                                             
                        time:   [583.70 ps 586.59 ps 590.26 ps]
Found 3 outliers among 100 measurements (3.00%)
  1 (1.00%) low severe
  2 (2.00%) high mild
sv2frame::encrypted_len::buffer_pool/61440                                                                             
                        time:   [560.69 ps 567.25 ps 575.11 ps]
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high severe

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
