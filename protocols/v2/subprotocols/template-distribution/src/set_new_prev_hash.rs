#[cfg(not(feature = "with_serde"))]
use alloc::vec::Vec;
#[cfg(not(feature = "with_serde"))]
use binary_sv2::codec;
use binary_sv2::U256;
use binary_sv2::{Deserialize, Serialize};

///// ## SetNewPrevHash (Server -> Client)
///// Upon successful validation of a new best block, the server MUST immediately provide a
///// SetNewPrevHash message. If a [NewWork](TODO link) message has previously been sent with the
///// [future_job](TODO link) flag set, which is valid work based on the prev_hash contained in this message, the
///// template_id field SHOULD be set to the job_id present in that NewTemplate message
///// indicating the client MUST begin mining on that template as soon as possible.
///// TODO: Define how many previous works the client has to track (2? 3?), and require that the
///// server reference one of those in SetNewPrevHash.
#[derive(Serialize, Deserialize, Debug)]
pub struct SetNewPrevHash<'decoder> {
    /// template_id referenced in a previous NewTemplate message.
    template_id: u64,
    /// Previous block’s hash, as it must appear in the next block’s header.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    prev_hash: U256<'decoder>,
    /// The nTime field in the block header at which the client should start
    /// (usually current time). This is NOT the minimum valid nTime value.
    header_timestamp: u32,
    /// Block header field.
    n_bits: u32,
    /// The maximum double-SHA256 hash value which would represent a valid
    /// block. Note that this may be lower than the target implied by nBits in
    /// several cases, including weak-block based block propagation.
    #[cfg_attr(feature = "with_serde", serde(borrow))]
    target: U256<'decoder>,
}
