use crate::mining_sv2::NewMiningJob;
use binary_sv2::U256;

pub struct StandardChannel {
    channel_id: u32,
    user_identity: &'static str,
    extranonce_prefix: u32,
    target: U256<'static>,
    nominal_hashrate: f32,
    current_job: Option<NewMiningJob<'static>>,
    future_job: Option<NewMiningJob<'static>>,
    past_jobs: Vec<NewMiningJob<'static>>,
    last_share_sequence_number: Option<u32>,
    share_work_sum: Option<f32>,
    share_batch_size: u8,
}

impl StandardChannel {}
