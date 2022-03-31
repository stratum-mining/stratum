use crate::lib::mining_pool::Downstream;
use binary_sv2::u256_from_int;
use messages_sv2::{
    errors::Error,
    handlers::mining::{ChannelType, ParseDownstreamMiningMessages, SendTo},
    mining_sv2::*,
    parsers::Mining,
    routing_logic::NoRouting,
    selectors::NullDownstreamMiningSelector,
    utils::Mutex,
};
use std::{convert::TryInto, sync::Arc};

impl ParseDownstreamMiningMessages<(), NullDownstreamMiningSelector, NoRouting> for Downstream {
    fn get_channel_type(&self) -> ChannelType {
        ChannelType::Group
    }

    fn is_work_selection_enabled(&self) -> bool {
        false
    }

    fn handle_open_standard_mining_channel(
        &mut self,
        incoming: OpenStandardMiningChannel,
        _m: Option<Arc<Mutex<()>>>,
    ) -> Result<SendTo<()>, Error> {
        let request_id = incoming.request_id;
        let message = match (self.downstream_data.header_only, self.id) {
            (false, group_channel_id) => {
                let channel_id = self.channel_ids.next();
                OpenStandardMiningChannelSuccess {
                    request_id,
                    channel_id,
                    group_channel_id,
                    target: u256_from_int(45_u32),
                    extranonce_prefix: vec![0_u8; 32].try_into().unwrap(),
                }
            }
            (true, channel_id) => OpenStandardMiningChannelSuccess {
                request_id,
                channel_id,
                group_channel_id: crate::HOM_GROUP_ID,
                target: u256_from_int(45_u32),
                extranonce_prefix: vec![0_u8; 32].try_into().unwrap(),
            },
        };
        Ok(SendTo::RelayNewMessage(
            Arc::new(Mutex::new(())),
            Mining::OpenStandardMiningChannelSuccess(message),
        ))
    }

    fn handle_open_extended_mining_channel(
        &mut self,
        _: OpenExtendedMiningChannel,
    ) -> Result<SendTo<()>, Error> {
        todo!()
    }

    fn handle_update_channel(&mut self, _: UpdateChannel) -> Result<SendTo<()>, Error> {
        todo!()
    }

    fn handle_submit_shares_standard(
        &mut self,
        _: SubmitSharesStandard,
    ) -> Result<SendTo<()>, Error> {
        todo!()
    }

    fn handle_submit_shares_extended(
        &mut self,
        _: SubmitSharesExtended,
    ) -> Result<SendTo<()>, Error> {
        todo!()
    }

    fn handle_set_custom_mining_job(&mut self, _: SetCustomMiningJob) -> Result<SendTo<()>, Error> {
        todo!()
    }
}
