# Interoperability tests (DRAFT)

How to test Sv2 compliant software against the SRI implementation.

## With Message Generator

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

## Using the SRI role implementations

TODO
