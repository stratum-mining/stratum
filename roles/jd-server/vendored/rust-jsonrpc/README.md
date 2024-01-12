[![Status](https://travis-ci.org/apoelstra/rust-jsonrpc.png?branch=master)](https://travis-ci.org/apoelstra/rust-jsonrpc)

# Rust Version compatibility

This library is compatible with Rust **1.48.0** or higher.

However, if you want to use the library with 1.41, you will need to pin a couple of
our dependencies:

```
cargo update -p serde --precise 1.0.156
cargo update -p syn --precise 1.0.107
```

before building. (And if your code is a library, your downstream users will need to
run these commands, and so on.)

# Rust JSONRPC Client

Rudimentary support for sending JSONRPC 2.0 requests and receiving responses.

As an example, hit a local bitcoind JSON-RPC endpoint and call the `uptime` command.

```rust
use jsonrpc::Client;
use jsonrpc::simple_http::{self, SimpleHttpTransport};

fn client(url: &str, user: &str, pass: &str) -> Result<Client, simple_http::Error> {
    let t = SimpleHttpTransport::builder()
        .url(url)?
        .auth(user, Some(pass))
        .build();

    Ok(Client::with_transport(t))
}

// Demonstrate an example JSON-RCP call against bitcoind.
fn main() {
    let client = client("localhost:18443", "user", "pass").expect("failed to create client");
    let request = client.build_request("uptime", &[]);
    let response = client.send_request(request).expect("send_request failed");

    // For other commands this would be a struct matching the returned json.
    let result: u64 = response.result().expect("response is an error, use check_error");
    println!("bitcoind uptime: {}", result);
}
```

## Githooks

To assist devs in catching errors _before_ running CI we provide some githooks. If you do not
already have locally configured githooks you can use the ones in this repository by running, in the
root directory of the repository:
```
git config --local core.hooksPath githooks/
```

Alternatively add symlinks in your `.git/hooks` directory to any of the githooks we provide.
