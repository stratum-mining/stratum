# SV2 Integration Tests

This is a test crate and it can be used in order to test the behavior of different roles when
working together. Each role should have a `start_[role_name]` function under `common` folder that
can be called in order to run the role. In order to assert the behavior of the role or the messages
it exchanges with other roles, you can use the `Sniffer` helper in order to listen to the messages
exchanged between the roles, and assert those messages using the `assert_message_[message_type]`
function. For examples on how to use the `Sniffer` helper, you can check the
`sniffer_integration.rs` module or other tests in the `tests` folder.

All of our tests run in regtest network. We download the Template Provider node from
https://github.com/Sjors/bitcoin/releases/download. This is a pre-built binary that we use to run an
Stratum V2 compatible bitcoin node. Note that this is the only external dependency(and Role) that we
have in our tests.

## Running Instructions

In order to run the integration tests, you can use the following command:

```bash
$ git clone git@github.com:stratum-mining/stratum.git
$ cargo test --manifest-path=test/integration-tests/Cargo.toml --verbose --test '*' -- --nocapture
```

Note: during the execution of the tests, a new directory called `template-provider` is created.
This directory holds the executable for Template Provider node, as well as the different data 
directories created for each execution.

## License
MIT OR Apache-2.0
