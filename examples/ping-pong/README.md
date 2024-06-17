`ping-pong` is an example of how to encode and decode SV2 binary frames (without any encryption layer) while leveraging the following crates:
- [`binary_sv2`](http://docs.rs/binary_sv2)
- [`codec_sv2`](http://docs.rs/codec_sv2)
- [`framing_sv2`](http://docs.rs/framing_sv2) (which is actually just re-exported by `codec_sv2`)

We establish a simple `Ping`-`Pong` protocol with a server and a client communicating over a TCP socket.

The server expects to receive a `Ping` message encoded as a SV2 binary frame.
The `Ping` message contains a `nonce`, which is a `u8` generated randomly by the client.

The client expects to get a `Pong` message in response, also encoded as a SV2 binary frame, with the same `nonce`.

The messages are assigned arbitrary values for binary encoding:
```rust
pub const PING_MSG_TYPE: u8 = 0xfe;
pub const PONG_MSG_TYPE: u8 = 0xff;
```