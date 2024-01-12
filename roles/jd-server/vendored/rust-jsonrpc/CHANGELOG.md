# 0.16.0 - 2023-06-29

* Re-export the `minreq` crate when the feature is set
  [#102](https://github.com/apoelstra/rust-jsonrpc/pull/102)
* Don't treat HTTP errors with no JSON as JSON parsing errors
  [#103](https://github.com/apoelstra/rust-jsonrpc/pull/103)

# 0.15.0 - 2023-05-28

* Add new transport that uses `minreq`
  [#94](https://github.com/apoelstra/rust-jsonrpc/pull/94)
* Bump MSRV to rust 1.48.0
  [#91](https://github.com/apoelstra/rust-jsonrpc/pull/91)

# 0.14.1 - 2023-04-03

* simple_http: fix "re-open socket on write failure" behavior
  [#84](https://github.com/apoelstra/rust-jsonrpc/pull/84)
  [#86](https://github.com/apoelstra/rust-jsonrpc/pull/86)
* simple_http: add "host" header (required by HTTP 1.1)
  [#85](https://github.com/apoelstra/rust-jsonrpc/pull/85)
* simple_http: add ability to replace URL/path; minor ergonomic improvements
  [#89](https://github.com/apoelstra/rust-jsonrpc/pull/89)

# 0.14.0 - 2022-11-28

This release significantly improves our `simple_http` client, though at the
apparent cost of a performance regression when making repeated RPC calls to
a local bitcoind. We are unsure what to make of this, since our code now uses
fewer sockets, less memory and does less redundant processing.

The highlights are:

* Support JSON replies that span multiple lines
  [#70](https://github.com/apoelstra/rust-jsonrpc/pull/69)
* Add feature-gated support for using a SOCKS proxy
  [#70](https://github.com/apoelstra/rust-jsonrpc/pull/70)
* Fix resource exhaustive bug on MacOS by reusing sockets
  [#72](https://github.com/apoelstra/rust-jsonrpc/pull/72)
  [#76](https://github.com/apoelstra/rust-jsonrpc/pull/76)

As well as improvements to our code quality and test infrastructure.

# 0.13.0 - 2022-07-21 "Edition 2018 Release"

This release increases the MSRV to 1.41.1, bringing with it a bunch of new language features.

Some highlights:

- The MSRV bump [#58](https://github.com/apoelstra/rust-jsonrpc/pull/58)
- Add IPv6 support [#63](https://github.com/apoelstra/rust-jsonrpc/pull/63)
- Remove `serder_derive` dependency [#61](https://github.com/apoelstra/rust-jsonrpc/pull/61)

# 0.12.1 - 2022-01-20

## Features

* A new set of transports were added for JSONRPC over raw TCP sockets (one using `SocketAddr`, and
  one UNIX-only using Unix Domain Sockets)

## Bug fixes

* The `Content-Type` HTTP header is now correctly set to `application/json`
* The `Connection: Close` HTTP header is now sent for requests

# 0.12.0 - 2020-12-16

* Remove `http` and `hyper` dependencies
* Implement our own simple HTTP transport for Bitcoin Core
* But allow use of generic transports

# 0.11.0 - 2019-04-05

* [Clean up the API](https://github.com/apoelstra/rust-jsonrpc/pull/19)
* [Set the content-type header to json]((https://github.com/apoelstra/rust-jsonrpc/pull/21)
* [Allow no `result` field in responses](https://github.com/apoelstra/rust-jsonrpc/pull/16)
* [Add batch request support](https://github.com/apoelstra/rust-jsonrpc/pull/24)

