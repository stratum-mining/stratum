Benchmark for 2000 samples with criterion:

```
sample-size-example/with pool
                        time:   [7.4963 ms 7.5006 ms 7.5051 ms]
                        change: [+6.1229% +6.3176% +6.5036%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 26 outliers among 2000 measurements (1.30%)
  3 (0.15%) low mild
  20 (1.00%) high mild
  3 (0.15%) high severe
Benchmarking sample-size-example/without pool: Warming up for 3.0000 s
Warning: Unable to complete 2000 samples in 5.0s. You may wish to increase target time to 20.4s, or reduce sample count to 490.
sample-size-example/without pool
                        time:   [10.268 ms 10.274 ms 10.279 ms]
                        change: [+2.6310% +2.7545% +2.8775%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 29 outliers among 2000 measurements (1.45%)
  5 (0.25%) low mild
  18 (0.90%) high mild
  6 (0.30%) high severe
Benchmarking sample-size-example/with control struct: Warming up for 3.0000 s
Warning: Unable to complete 2000 samples in 5.0s. You may wish to increase target time to 65.8s, or reduce sample count to 150.
sample-size-example/with control struct
                        time:   [32.577 ms 32.593 ms 32.609 ms]
                        change: [+3.6492% +3.7783% +3.9036%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 61 outliers among 2000 measurements (3.05%)
  4 (0.20%) low mild
  51 (2.55%) high mild
  6 (0.30%) high severe
sample-size-example/with control struct max e
                        time:   [1.2608 ms 1.2618 ms 1.2629 ms]
                        change: [-0.3202% -0.1721% -0.0231%] (p = 0.01 < 0.05)
                        Change within noise threshold.
Found 140 outliers among 2000 measurements (7.00%)
  7 (0.35%) low mild
  42 (2.10%) high mild
  91 (4.55%) high severe
Benchmarking sample-size-example/with pool thread 1: Warming up for 3.0000 s
Warning: Unable to complete 2000 samples in 5.0s. You may wish to increase target time to 68.8s, or reduce sample count to 140.
sample-size-example/with pool thread 1
                        time:   [34.626 ms 34.660 ms 34.693 ms]
                        change: [-0.3871% -0.1594% +0.0680%] (p = 0.17 > 0.05)
                        No change in performance detected.
Found 42 outliers among 2000 measurements (2.10%)
  6 (0.30%) low severe
  29 (1.45%) low mild
  6 (0.30%) high mild
  1 (0.05%) high severe
Benchmarking sample-size-example/without pool threaded 1: Warming up for 3.0000 s
Warning: Unable to complete 2000 samples in 5.0s. You may wish to increase target time to 285.4s, or reduce sample count to 30.
sample-size-example/without pool threaded 1
                        time:   [142.02 ms 142.23 ms 142.49 ms]
                        change: [+0.2557% +0.4149% +0.5979%] (p = 0.00 < 0.05)
                        Change within noise threshold.
Found 66 outliers among 2000 measurements (3.30%)
  28 (1.40%) high mild
  38 (1.90%) high severe
Benchmarking sample-size-example/with control threaded 1: Warming up for 3.0000 s
Warning: Unable to complete 2000 samples in 5.0s. You may wish to increase target time to 98.8s, or reduce sample count to 100.
sample-size-example/with control threaded 1
                        time:   [49.762 ms 49.790 ms 49.819 ms]
                        change: [+0.7355% +0.8427% +0.9496%] (p = 0.00 < 0.05)
                        Change within noise threshold.
Found 15 outliers among 2000 measurements (0.75%)
  5 (0.25%) high mild
  10 (0.50%) high severe
Benchmarking sample-size-example/with control threaded max: Warming up for 3.0000 s
Warning: Unable to complete 2000 samples in 5.0s. You may wish to increase target time to 36.4s, or reduce sample count to 270.
sample-size-example/with control threaded max
                        time:   [18.177 ms 18.201 ms 18.225 ms]
                        change: [+0.3845% +0.6791% +0.9724%] (p = 0.00 < 0.05)
                        Change within noise threshold.
Found 8 outliers among 2000 measurements (0.40%)
  6 (0.30%) high mild
  2 (0.10%) high severe
Benchmarking sample-size-example/with pool thread 2: Warming up for 3.0000 s
Warning: Unable to complete 2000 samples in 5.0s. You may wish to increase target time to 161.8s, or reduce sample count to 60.
sample-size-example/with pool thread 2
                        time:   [80.684 ms 80.869 ms 81.052 ms]
                        change: [+0.9153% +1.4562% +2.0137%] (p = 0.00 < 0.05)
                        Change within noise threshold.
Benchmarking sample-size-example/without pool threaded 2: Warming up for 3.0000 s
Warning: Unable to complete 2000 samples in 5.0s. You may wish to increase target time to 389.1s, or reduce sample count to 20.
sample-size-example/without pool threaded 2
                        time:   [194.18 ms 194.24 ms 194.29 ms]
                        change: [+0.6562% +0.7258% +0.7900%] (p = 0.00 < 0.05)
                        Change within noise threshold.
Found 46 outliers among 2000 measurements (2.30%)
  43 (2.15%) high mild
  3 (0.15%) high severe
Benchmarking sample-size-example/with control threaded 2: Warming up for 3.0000 s
Warning: Unable to complete 2000 samples in 5.0s. You may wish to increase target time to 204.2s, or reduce sample count to 40.
sample-size-example/with control threaded 2
                        time:   [101.71 ms 101.75 ms 101.79 ms]
                        change: [+0.1046% +0.1841% +0.2619%] (p = 0.00 < 0.05)
                        Change within noise threshold.
Found 22 outliers among 2000 measurements (1.10%)
  6 (0.30%) low mild
  14 (0.70%) high mild
  2 (0.10%) high severe
Benchmarking sample-size-example/with control threaded max 2: Warming up for 3.0000 s
Warning: Unable to complete 2000 samples in 5.0s. You may wish to increase target time to 133.9s, or reduce sample count to 70.
sample-size-example/with control threaded max 2
                        time:   [66.953 ms 66.972 ms 66.992 ms]
                        change: [-0.0483% +0.0157% +0.0792%] (p = 0.64 > 0.05)
                        No change in performance detected.
Found 2 outliers among 2000 measurements (0.10%)
  1 (0.05%) high mild
  1 (0.05%) high severe


```

Benchmark with iai:
```
with_pool
  Instructions:           131725449 (-0.570813%)
  L1 Accesses:            199120813 (-0.570775%)
  L2 Accesses:                   63 (-10.00000%)
  RAM Accesses:                 445 (No change)
  Estimated Cycles:       199136703 (-0.570748%)

without_pool
  Instructions:           149678362 (+0.605157%)
  L1 Accesses:            201824534 (+0.604935%)
  L2 Accesses:                   53 (-5.357143%)
  RAM Accesses:                 422 (No change)
  Estimated Cycles:       201839569 (+0.604882%)

with_contro_struct
  Instructions:           398958604 (-2.922689%)
  L1 Accesses:            486039573 (-2.880062%)
  L2 Accesses:               268253 (-2.837863%)
  RAM Accesses:               60270 (-0.881492%)
  Estimated Cycles:       489490288 (-2.871507%)

with_contro_struct_max_e
  Instructions:            10530711 (-0.443323%)
  L1 Accesses:             18891089 (-0.448614%)
  L2 Accesses:                   31 (-11.42857%)
  RAM Accesses:                 147 (-2.000000%)
  Estimated Cycles:        18896389 (-0.449144%)

with_pool_trreaded_1
  Instructions:           315589669 (+6.871786%)
  L1 Accesses:            476893082 (+7.340861%)
  L2 Accesses:              1013857 (+5.119001%)
  RAM Accesses:                4562 (+7.950781%)
  Estimated Cycles:       482122037 (+7.317211%)

without_pool_threaded_1
  Instructions:          1592248076 (-0.126900%)
  L1 Accesses:           2275109426 (-0.126566%)
  L2 Accesses:               690657 (-1.331895%)
  RAM Accesses:             1869740 (-0.093935%)
  Estimated Cycles:      2344003611 (-0.127453%)

with_control_threaded
  Instructions:           422849601 (+2.644515%)
  L1 Accesses:            517408546 (+2.586379%)
  L2 Accesses:               544627 (+7.458802%)
  RAM Accesses:               61171 (+0.468088%)
  Estimated Cycles:       522272666 (+2.601768%)

with_control_max_threaded
  Instructions:            25307689 (+1.314665%)
  L1 Accesses:             39575297 (+1.421190%)
  L2 Accesses:               133930 (+31.34770%)
  RAM Accesses:                1032 (-0.096805%)
  Estimated Cycles:        40281067 (+1.805416%)

with_pool_trreaded_2
  Instructions:           143394509 (-62.39048%)
  L1 Accesses:            214663837 (-59.65541%)
  L2 Accesses:               573324 (-0.901409%)
  RAM Accesses:                2404 (-3.955254%)
  Estimated Cycles:       217614597 (-59.32865%)

without_pool_threaded_2
  Instructions:          1593243729
  L1 Accesses:           2282509490
  L2 Accesses:               656238
  RAM Accesses:             1869806
  Estimated Cycles:      2351233890

with_control_threaded_2
  Instructions:           420788729 (-1.703631%)
  L1 Accesses:            515173403 (-1.684962%)
  L2 Accesses:               513862 (+7.084478%)
  RAM Accesses:               61745 (-0.221389%)
  Estimated Cycles:       519903788 (-1.639158%)

with_control_max_threaded_2
  Instructions:            25177171 (-0.562458%)
  L1 Accesses:             39298946 (-0.696887%)
  L2 Accesses:                98796 (+18.25059%)
  RAM Accesses:                 857 (-4.671858%)
  Estimated Cycles:        39822921 (-0.502252%)

```
