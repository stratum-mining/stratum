use crate::mining_sv2::NewExtendedMiningJob;
use binary_sv2::U256;

pub struct ExtendedChannel {
    channel_id: u32,
    user_identity: &'static str,
    extranonce_prefix: u32,
    extranonce_size: u16,
    target: U256<'static>,
    nominal_hashrate: f32,
    current_job: Option<NewExtendedMiningJob<'static>>,
    future_job: Option<NewExtendedMiningJob<'static>>,
    past_jobs: Vec<NewExtendedMiningJob<'static>>,
    last_share_sequence_number: Option<u32>,
    share_work_sum: Option<f32>,
    share_batch_size: u8,
}

impl ExtendedChannel {}
