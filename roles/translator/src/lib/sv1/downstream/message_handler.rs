use tracing::{debug, error, info, warn};
use v1::{
    client_to_server, json_rpc, server_to_client,
    utils::{Extranonce, HexU32Be},
    IsServer,
};

use crate::{
    sv1::downstream::{data::DownstreamData, SubmitShareWithChannelId},
    utils::validate_sv1_share,
};

// Implements `IsServer` for `Downstream` to handle the Sv1 messages.
impl IsServer<'static> for DownstreamData {
    fn handle_configure(
        &mut self,
        request: &client_to_server::Configure,
    ) -> (Option<server_to_client::VersionRollingParams>, Option<bool>) {
        info!("Received mining.configure from Sv1 downstream");
        debug!("Down: Handling mining.configure: {:?}", request);
        self.version_rolling_mask = request
            .version_rolling_mask()
            .map(|mask| HexU32Be(mask & 0x1FFFE000));
        self.version_rolling_min_bit = request.version_rolling_min_bit_count();

        debug!(
            "Negotiated version_rolling_mask is {:?}",
            self.version_rolling_mask
        );
        (
            Some(server_to_client::VersionRollingParams::new(
                self.version_rolling_mask.clone().unwrap_or(HexU32Be(0)),
                self.version_rolling_min_bit.clone().unwrap_or(HexU32Be(0)),
            ).expect("Version mask invalid, automatic version mask selection not supported, please change it in crate::downstream::mod.rs")),
            Some(false),
        )
    }

    fn handle_subscribe(&self, request: &client_to_server::Subscribe) -> Vec<(String, String)> {
        info!("Received mining.subscribe from Sv1 downstream");
        debug!("Down: Handling mining.subscribe: {:?}", request);

        let set_difficulty_sub = (
            "mining.set_difficulty".to_string(),
            self.downstream_id.to_string(),
        );

        let notify_sub = (
            "mining.notify".to_string(),
            "ae6812eb4cd7735a302a8a9dd95cf71f".to_string(),
        );

        vec![set_difficulty_sub, notify_sub]
    }

    fn handle_authorize(&self, request: &client_to_server::Authorize) -> bool {
        info!("Received mining.authorize from Sv1 downstream");
        debug!("Down: Handling mining.authorize: {:?}", request);
        true
    }

    fn handle_submit(&self, request: &client_to_server::Submit<'static>) -> bool {
        if let Some(channel_id) = self.channel_id {
            info!(
                "Received mining.submit from SV1 downstream for channel id: {}",
                channel_id
            );
            let is_valid_share = validate_sv1_share(
                request,
                self.target.clone(),
                self.extranonce1.clone(),
                self.version_rolling_mask.clone(),
                self.sv1_server_data.clone(),
                channel_id,
            )
            .unwrap_or(false);
            if !is_valid_share {
                error!("Invalid share for channel id: {}", channel_id);
                return false;
            }
            let to_send: SubmitShareWithChannelId = SubmitShareWithChannelId {
                channel_id,
                downstream_id: self.downstream_id,
                share: request.clone(),
                extranonce: self.extranonce1.clone(),
                extranonce2_len: self.extranonce2_len,
                version_rolling_mask: self.version_rolling_mask.clone(),
                job_version: self.last_job_version_field,
            };
            // Store the share to be sent to the Sv1Server
            self.pending_share.replace(Some(to_send));
            true
        } else {
            error!("Cannot submit share: channel_id is None (waiting for OpenExtendedMiningChannelSuccess)");
            false
        }
    }

    /// Indicates to the server that the client supports the mining.set_extranonce method.
    fn handle_extranonce_subscribe(&self) {}

    /// Checks if a Downstream role is authorized.
    fn is_authorized(&self, name: &str) -> bool {
        self.authorized_worker_name == *name
    }

    /// Authorizes a Downstream role.
    fn authorize(&mut self, name: &str) {
        let name: String = name.into();
        if !self.is_authorized(&name) {
            self.authorized_worker_name = name.to_string();
        }
    }

    /// Sets the `extranonce1` field sent in the SV1 `mining.notify` message to the value specified
    /// by the SV2 `OpenExtendedMiningChannelSuccess` message sent from the Upstream role.
    fn set_extranonce1(
        &mut self,
        _extranonce1: Option<Extranonce<'static>>,
    ) -> Extranonce<'static> {
        self.extranonce1.clone().try_into().unwrap()
    }

    /// Returns the `Downstream`'s `extranonce1` value.
    fn extranonce1(&self) -> Extranonce<'static> {
        self.extranonce1.clone().try_into().unwrap()
    }

    /// Sets the `extranonce2_size` field sent in the SV1 `mining.notify` message to the value
    /// specified by the SV2 `OpenExtendedMiningChannelSuccess` message sent from the Upstream role.
    fn set_extranonce2_size(&mut self, _extra_nonce2_size: Option<usize>) -> usize {
        self.extranonce2_len
    }

    /// Returns the `Downstream`'s `extranonce2_size` value.
    fn extranonce2_size(&self) -> usize {
        self.extranonce2_len
    }

    /// Returns the version rolling mask.
    fn version_rolling_mask(&self) -> Option<HexU32Be> {
        self.version_rolling_mask.clone()
    }

    /// Sets the version rolling mask.
    fn set_version_rolling_mask(&mut self, mask: Option<HexU32Be>) {
        self.version_rolling_mask = mask;
    }

    /// Sets the minimum version rolling bit.
    fn set_version_rolling_min_bit(&mut self, mask: Option<HexU32Be>) {
        self.version_rolling_min_bit = mask
    }

    fn notify(&'_ mut self) -> Result<json_rpc::Message, v1::error::Error<'_>> {
        warn!("notify() called on DownstreamData - this method is not implemented for the translator proxy");
        Err(v1::error::Error::UnexpectedMessage("notify".to_string()))
    }
}
