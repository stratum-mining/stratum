use crate::utils::Id;
use mining_sv2::{target_from_hr, Extranonce, OpenStandardMiningChannelSuccess};
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct UpstreamWithGroups {
    groups: HashMap<u32, Id>,
    ids: Id,
    extranonces: Extranonce,
}

impl UpstreamWithGroups {
    pub fn new() -> Self {
        Self {
            groups: HashMap::new(),
            ids: Id::new(),
            extranonces: Extranonce::new(),
        }
    }

    pub fn new_group_channel(&mut self) -> u32 {
        let g_id = self.ids.next();
        self.groups.insert(g_id, Id::new());
        g_id
    }

    pub fn new_standard_channel(
        &mut self,
        request_id: u32,
        downstream_hr: f32,
        group_id: u32,
    ) -> OpenStandardMiningChannelSuccess {
        OpenStandardMiningChannelSuccess {
            request_id,
            channel_id: self.groups.get_mut(&group_id).unwrap().next(),
            target: target_from_hr(downstream_hr),
            extranonce_prefix: self.extranonces.next(),
            group_channel_id: group_id,
        }
    }
}
