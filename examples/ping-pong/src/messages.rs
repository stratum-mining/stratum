use crate::error::Error;
use network_helpers_sv2::roles_logic_sv2::codec_sv2::binary_sv2::{
    binary_codec_sv2, Deserialize, Serialize,
};

use rand::Rng;

pub const PING_MSG_TYPE: u8 = 0xfe;
pub const PONG_MSG_TYPE: u8 = 0xff;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Ping {
    nonce: u8,
}

impl Ping {
    pub fn new() -> Result<Self, Error> {
        let mut rng = rand::thread_rng();
        let random: u8 = rng.gen();
        Ok(Self { nonce: random })
    }

    pub fn get_nonce(&self) -> u8 {
        self.nonce
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Pong {
    nonce: u8,
}

impl<'decoder> Pong {
    pub fn new(nonce: u8) -> Result<Self, Error> {
        Ok(Self { nonce })
    }

    pub fn get_nonce(&self) -> u8 {
        self.nonce
    }
}
