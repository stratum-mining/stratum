# Fuzzing

This crate uses **cargo-fuzz** to test the robustness of the codebase.

## Requirements

Before running, install LLVM tools. Instructions:
[https://doc.rust-lang.org/stable/rustc/instrument-coverage.html#installing-llvm-coverage-tool](https://doc.rust-lang.org/stable/rustc/instrument-coverage.html#installing-llvm-coverage-tool)

Then install `cargo-fuzz`:

```sh
cargo install cargo-fuzz
```

Also, make sure you are on the nightly toolchain.

## Running Fuzz Targets

All fuzz targets live under:

```
fuzz/fuzz_targets/
```

To run one:

```sh
cargo +nightly fuzz run <target-name>
```

Example:

```sh
cargo +nightly fuzz run deserialize_setup_connection
```

Artifacts and crash cases are written to:

```
fuzz/artifacts/<target-name>/
```

If you find a crash that might indicate a **security issue**, please report it through our responsible disclosure process.
See [SECURITY](../SECURITY.md).

## Listing Available Targets

You can list all existing targets with:

```sh
cargo +nightly fuzz list
```

This is useful when adding new targets or exploring what’s already covered.

## Working With the Seed Corpus

Each fuzz target has a corpus under:

```
fuzz/corpus/<target-name>/
```

These seeds guide fuzzing toward interesting states. Improving the corpus is one of the most impactful contributions you can make here.

To stay in sync with the shared corpus used across Stratum projects, follow the instructions here:

[https://github.com/stratum-mining/stratum-fuzzing-corpus/blob/main/scripts/readme.md](https://github.com/stratum-mining/stratum-fuzzing-corpus/blob/main/scripts/readme.md)

That repository covers:

* How to fetch and sync the shared corpus
* How to add new seeds
* How to run corpus-merging scripts
* How to submit contributions upstream

