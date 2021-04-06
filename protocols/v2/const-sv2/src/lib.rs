//! Central repository for all the sv2 constants

pub const SV2_FRAME_HEADER_SIZE: usize = 6;
pub const SV2_FRAME_HEADER_LEN_OFFSET: usize = 3;
pub const SV2_FRAME_HEADER_LEN_END: usize = 3;

pub const NOISE_FRAME_HEADER_SIZE: usize = 2;
pub const NOISE_FRAME_HEADER_LEN_OFFSET: usize = 0;
pub const NOISE_FRAME_HEADER_LEN_END: usize = 2;
pub const NOISE_FRAME_MAX_SIZE: usize = u16::MAX as usize;

pub const NOISE_PARAMS: &'static str = "Noise_NX_25519_ChaChaPoly_BLAKE2s";
pub const SNOW_PSKLEN: usize = 32;
pub const SNOW_TAGLEN: usize = 16;
