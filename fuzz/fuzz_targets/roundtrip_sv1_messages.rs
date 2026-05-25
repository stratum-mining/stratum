#![no_main]

use {
    arbitrary::Arbitrary,
    libfuzzer_sys::fuzz_target,
    std::convert::{TryFrom, TryInto},
    sv1_api::{
        client_to_server,
        json_rpc::Message,
        methods::Method,
        server_to_client,
        utils::{Extranonce, HexBytes, HexU32Be, MerkleNode, PrevHash},
    },
};

#[derive(Arbitrary, Debug)]
enum FuzzInput {
    Authorize {
        id: u64,
        name: String,
        password: String,
    },
    Configure {
        id: u64,
        mask: Option<u32>,
        min_bit_count: Option<u32>,
    },
    Notify {
        job_id: String,
        prev_hash: [u8; 32],
        coin_base1: Vec<u8>,
        coin_base2: Vec<u8>,
        merkle_branch: Vec<[u8; 32]>,
        version: u32,
        bits: u32,
        time: u32,
        clean_jobs: bool,
    },
    SetDifficulty {
        value: f64,
    },
    SetExtranonce {
        extra_nonce1: Vec<u8>,
        extra_nonce2_size: u8,
    },
    Submit {
        id: u64,
        user_name: String,
        job_id: String,
        extra_nonce2: Vec<u8>,
        time: u32,
        nonce: u32,
        version_bits: Option<u32>,
    },
    Subscribe {
        id: u64,
        agent_signature: String,
        extranonce1: Option<Vec<u8>>,
    },
}

fn bounded_b032(bytes: Vec<u8>) -> Extranonce<'static> {
    let mut bytes = bytes;
    bytes.truncate(32);
    Extranonce::try_from(bytes).expect("bounded extranonce fits B032")
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn roundtrip(message: Message) {
    if let Ok(serialized) = serde_json::to_vec(&message) {
        let reparsed = serde_json::from_slice::<Message>(&serialized)
            .expect("serialized SV1 JSON-RPC message should parse");
        let _method: Result<Method, _> = reparsed.try_into();
    }
}

fuzz_target!(|input: FuzzInput| {
    match input {
        FuzzInput::Authorize { id, name, password } => {
            let message = client_to_server::Authorize { id, name, password }.into();
            roundtrip(message);
        }
        FuzzInput::Configure {
            id,
            mask,
            min_bit_count,
        } => {
            let message = client_to_server::Configure::new(
                id,
                mask.map(HexU32Be),
                min_bit_count.map(HexU32Be),
            )
            .into();
            roundtrip(message);
        }
        FuzzInput::Notify {
            job_id,
            prev_hash,
            coin_base1,
            coin_base2,
            merkle_branch,
            version,
            bits,
            time,
            clean_jobs,
        } => {
            let merkle_branch = merkle_branch
                .into_iter()
                .take(32)
                .map(|node| MerkleNode::try_from(node.to_vec()).expect("array fits U256"))
                .collect();

            let message = server_to_client::Notify {
                job_id,
                prev_hash: PrevHash::try_from(hex_lower(&prev_hash).as_str())
                    .expect("array hex fits U256"),
                coin_base1: HexBytes::from(coin_base1),
                coin_base2: HexBytes::from(coin_base2),
                merkle_branch,
                version: HexU32Be(version),
                bits: HexU32Be(bits),
                time: HexU32Be(time),
                clean_jobs,
            }
            .into();
            roundtrip(message);
        }
        FuzzInput::SetDifficulty { value } => {
            if value.is_finite() {
                let message = server_to_client::SetDifficulty { value }.into();
                roundtrip(message);
            }
        }
        FuzzInput::SetExtranonce {
            extra_nonce1,
            extra_nonce2_size,
        } => {
            let message = server_to_client::SetExtranonce {
                extra_nonce1: bounded_b032(extra_nonce1),
                extra_nonce2_size: usize::from(extra_nonce2_size),
            }
            .into();
            roundtrip(message);
        }
        FuzzInput::Submit {
            id,
            user_name,
            job_id,
            extra_nonce2,
            time,
            nonce,
            version_bits,
        } => {
            let message = client_to_server::Submit {
                user_name,
                job_id,
                extra_nonce2: bounded_b032(extra_nonce2),
                time: HexU32Be(time),
                nonce: HexU32Be(nonce),
                version_bits: version_bits.map(HexU32Be),
                id,
            }
            .into();
            roundtrip(message);
        }
        FuzzInput::Subscribe {
            id,
            agent_signature,
            extranonce1,
        } => {
            let subscribe = client_to_server::Subscribe {
                id,
                agent_signature,
                extranonce1: extranonce1.map(bounded_b032),
            };

            if let Ok(message) = Message::try_from(subscribe) {
                roundtrip(message);
            }
        }
    }
});
