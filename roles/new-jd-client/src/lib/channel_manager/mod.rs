pub struct ChannelManager;
mod downstream_message_handler;
mod jd_message_handler;
mod template_message_handler;
mod upstream_message_handler;

impl ChannelManager {
    pub fn new() -> Self {
        ChannelManager
    }

    pub async fn start(&self) {}
}
