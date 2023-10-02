extern crate jsonrpc;

// Note, tests are if empty "jsonrpc_fuzz" is not set but still show up in output of `cargo test --workspace`.

#[allow(unused_variables)] // `data` is not used when "jsonrpc_fuzz" is not set.
fn do_test(data: &[u8]) {
    #[cfg(jsonrpc_fuzz)]
    {
        use std::io;

        use jsonrpc::simple_http::SimpleHttpTransport;
        use jsonrpc::simple_http::FUZZ_TCP_SOCK;
        use jsonrpc::Client;

        *FUZZ_TCP_SOCK.lock().unwrap() = Some(io::Cursor::new(data.to_vec()));

        let t = SimpleHttpTransport::builder()
            .url("localhost:123")
            .expect("parse url")
            .auth("", None)
            .build();

        let client = Client::with_transport(t);
        let request = client.build_request("uptime", &[]);
        let _ = client.send_request(request);
    }
}

fn main() {
    loop {
        honggfuzz::fuzz!(|data| {
            do_test(data);
        });
    }
}

#[cfg(test)]
mod tests {
    fn extend_vec_from_hex(hex: &str) -> Vec<u8> {
        let mut out = vec![];
        let mut b = 0;
        for (idx, c) in hex.as_bytes().iter().enumerate() {
            b <<= 4;
            match *c {
                b'A'..=b'F' => b |= c - b'A' + 10,
                b'a'..=b'f' => b |= c - b'a' + 10,
                b'0'..=b'9' => b |= c - b'0',
                _ => panic!("Bad hex"),
            }
            if (idx & 1) == 1 {
                out.push(b);
                b = 0;
            }
        }
        out
    }

    #[test]
    fn duplicate_crash() {
        super::do_test(&extend_vec_from_hex("00"));
    }
}
