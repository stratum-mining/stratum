use binary_sv2::{self, Deserialize, Serialize, B064K, U256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestMsg {
    pub data: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZeroCopyMsg<'decoder> {
    pub channel_id: u32,
    pub merkle_root: U256<'decoder>,
    pub coinbase_suffix: B064K<'decoder>,
}

impl<'a> ZeroCopyMsg<'a> {
    pub fn new_owned(channel_id: u32, coinbase_size: usize) -> Self {
        let merkle_root = U256::try_from(vec![0x42u8; 32]).expect("U256 is exactly 32 bytes");
        let coinbase_suffix =
            B064K::try_from(vec![0xABu8; coinbase_size]).expect("coinbase_size <= 65535");
        Self {
            channel_id,
            merkle_root,
            coinbase_suffix,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OwnedMsg {
    pub channel_id: u32,
    pub merkle_root: [u8; 32],
    pub coinbase_suffix: Vec<u8>,
}

impl OwnedMsg {
    #[allow(dead_code)]
    pub fn from_zc(msg: ZeroCopyMsg<'_>) -> Self {
        let mut merkle_root = [0u8; 32];
        merkle_root.copy_from_slice(msg.merkle_root.inner_as_ref());
        OwnedMsg {
            channel_id: msg.channel_id,
            merkle_root,
            coinbase_suffix: msg.coinbase_suffix.inner_as_ref().to_vec(),
        }
    }
}
