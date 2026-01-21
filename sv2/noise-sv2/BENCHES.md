# Benchmarks

This document records the output of `cargo bench` for the `noise_sv2` crate.
The benchmarks measure the performance characteristics of the Noise protocol
implementation used by Stratum V2.

The results are intended to:
- Quantify the cost of the Noise handshake
- Measure post-handshake encryption and decryption overhead
- Serve as a reference point for future comparison and regression detection

All measurements are environment-dependent and should not be interpreted as
absolute performance guarantees.

## Benchmark Output

The following section contains the **raw output** of `cargo bench` captured
during a single run.

### Handshake Benchmarks

```text
Benchmarking handshake/step_0_initiator
Benchmarking handshake/step_0_initiator: Warming up for 3.0000 s
Benchmarking handshake/step_0_initiator: Collecting 100 samples in estimated 5.3787 s (56k iterations)
Benchmarking handshake/step_0_initiator: Analyzing
handshake/step_0_initiator
                        time:   [18.560 µs 18.737 µs 18.981 µs]
                        change: [-50.218% -49.023% -47.697%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 17 outliers among 100 measurements (17.00%)
  11 (11.00%) high mild
  6 (6.00%) high severe

Benchmarking handshake/step_1_responder
Benchmarking handshake/step_1_responder: Warming up for 3.0000 s
Benchmarking handshake/step_1_responder: Collecting 100 samples in estimated 7.4286 s (15k iterations)
Benchmarking handshake/step_1_responder: Analyzing
handshake/step_1_responder
                        time:   [177.39 µs 178.07 µs 178.93 µs]
                        change: [-54.298% -53.548% -52.733%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 10 outliers among 100 measurements (10.00%)
  1 (1.00%) low mild
  2 (2.00%) high mild
  7 (7.00%) high severe

Benchmarking handshake/step_2_initiator
Benchmarking handshake/step_2_initiator: Warming up for 3.0000 s
Benchmarking handshake/step_2_initiator: Collecting 100 samples in estimated 6.1452 s (10k iterations)
Benchmarking handshake/step_2_initiator: Analyzing
handshake/step_2_initiator
                        time:   [120.55 µs 120.91 µs 121.32 µs]
                        change: [-44.862% -43.680% -42.592%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 10 outliers among 100 measurements (10.00%)
  1 (1.00%) low mild
  6 (6.00%) high mild
  3 (3.00%) high severe

Benchmarking handshake/handshake
Benchmarking handshake/handshake: Warming up for 3.0000 s
Benchmarking handshake/handshake: Collecting 100 samples in estimated 6.1256 s (10k iterations)
Benchmarking handshake/handshake: Analyzing
handshake/handshake     time:   [316.58 µs 317.41 µs 318.34 µs]
                        change: [-43.167% -39.954% -36.598%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 7 outliers among 100 measurements (7.00%)
  1 (1.00%) low mild
  5 (5.00%) high mild
  1 (1.00%) high severe
````

---

### Transport Roundtrip Benchmarks

```text
Gnuplot not found, using plotters backend

Benchmarking transport/roundtrip/64B
Benchmarking transport/roundtrip/64B: Warming up for 3.0000 s
Benchmarking transport/roundtrip/64B: Collecting 100 samples in estimated 5.0026 s (914k iterations)
Benchmarking transport/roundtrip/64B: Analyzing
transport/roundtrip/64B time:   [5.1966 µs 5.2037 µs 5.2126 µs]
                        change: [-34.332% -31.654% -28.918%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 15 outliers among 100 measurements (15.00%)
  3 (3.00%) high mild
  12 (12.00%) high severe

Benchmarking transport/roundtrip/256B
Benchmarking transport/roundtrip/256B: Warming up for 3.0000 s
Benchmarking transport/roundtrip/256B: Collecting 100 samples in estimated 5.0128 s (858k iterations)
Benchmarking transport/roundtrip/256B: Analyzing
transport/roundtrip/256B
                        time:   [5.4840 µs 5.5004 µs 5.5190 µs]
                        change: [-37.044% -34.403% -31.675%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 10 outliers among 100 measurements (10.00%)
  4 (4.00%) high mild
  6 (6.00%) high severe

Benchmarking transport/roundtrip/1024B
Benchmarking transport/roundtrip/1024B: Warming up for 3.0000 s
Benchmarking transport/roundtrip/1024B: Collecting 100 samples in estimated 5.0300 s (621k iterations)
Benchmarking transport/roundtrip/1024B: Analyzing
transport/roundtrip/1024B
                        time:   [7.2587 µs 7.2827 µs 7.3112 µs]
                        change: [-32.654% -29.950% -27.189%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 18 outliers among 100 measurements (18.00%)
  7 (7.00%) high mild
  11 (11.00%) high severe

Benchmarking transport/roundtrip/4096B
Benchmarking transport/roundtrip/4096B: Warming up for 3.0000 s
Benchmarking transport/roundtrip/4096B: Collecting 100 samples in estimated 5.0042 s (268k iterations)
Benchmarking transport/roundtrip/4096B: Analyzing
transport/roundtrip/4096B
                        time:   [16.213 µs 16.388 µs 16.600 µs]
                        change: [-27.110% -23.533% -19.897%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 13 outliers among 100 measurements (13.00%)
  6 (6.00%) high mild
  7 (7.00%) high severe
```

---

## Interpretation Notes

* Handshake cost is dominated by responder-side processing and key exchange.
* Post-handshake transport costs scale predictably with payload size and remain
  within single-digit microseconds for small messages.

---

## Reproducing

Run benchmarks locally with:

```bash
cargo bench
```
