use serde::{Deserialize, Serialize};
use serde_sv2::{U256, U32, U64};

/// ## SetNewPrevHash (Server -> Client)
/// Upon successful validation of a new best block, the server MUST immediately provide a
/// SetNewPrevHash message. If a [NewWork](TODO link) message has previously been sent with the
/// [future_job](TODO link) flag set, which is valid work based on the prev_hash contained in this message, the
/// template_id field SHOULD be set to the job_id present in that NewTemplate message
/// indicating the client MUST begin mining on that template as soon as possible.
/// TODO: Define how many previous works the client has to track (2? 3?), and require that the
/// server reference one of those in SetNewPrevHash.
#[derive(Serialize, Deserialize, Debug)]
pub struct SetNewPrevHash<'a> {
    /// template_id referenced in a previous NewTemplate message.
    template_id: U64,
    /// Previous block’s hash, as it must appear in the next block’s header.
    #[serde(borrow)]
    prev_hash: U256<'a>,
    /// The nTime field in the block header at which the client should start
    /// (usually current time). This is NOT the minimum valid nTime value.
    header_timestamp: U32,
    /// Block header field.
    n_bits: U32,
    /// The maximum double-SHA256 hash value which would represent a valid
    /// block. Note that this may be lower than the target implied by nBits in
    /// several cases, including weak-block based block propagation.
    #[serde(borrow)]
    target: U256<'a>,
}
