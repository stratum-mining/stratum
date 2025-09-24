use stratum_common::roles_logic_sv2::channels_sv2::persistence::{
    Persistence, ShareAccountingEvent,
};
use tokio::io::AsyncWriteExt;
use tracing::error;

use crate::status::Status;

pub struct ShareFileHandler {
    file: tokio::fs::File,
    receiver: async_channel::Receiver<ShareAccountingEvent>,
    sender: async_channel::Sender<ShareAccountingEvent>,
    status_tx: async_channel::Sender<Status>,
}

impl ShareFileHandler {
    pub async fn new(path: &str, status_tx: async_channel::Sender<Status>) -> Self {
        let file = tokio::fs::File::create(path).await.unwrap();
        let (sender, receiver) = async_channel::bounded(1024);
        Self {
            file,
            receiver,
            sender,
            status_tx
        }
    }

    pub fn get_receiver(self) -> async_channel::Receiver<ShareAccountingEvent> {
        self.receiver
    }

    pub fn get_sender(self) -> async_channel::Sender<ShareAccountingEvent> {
        self.sender
    }

    pub async fn write_event_to_file(&mut self, event: ShareAccountingEvent) {
        match event {
            ShareAccountingEvent::ShareAccepted {
                channel_id,
                share_work,
                share_sequence_number,
                share_hash,
                total_shares_accepted,
                total_share_work_sum,
                timestamp,
            } => {
                let _ = self.file.write_all(
                    format!(
                        "ShareAccepted: channel_id: {}, share_work: {}, share_sequence_number: {}, share_hash: {}, total_shares_accepted: {}, total_share_work_sum: {}, timestamp: {:#?}\n",
                        channel_id,
                        share_work,
                        share_sequence_number,
                        share_hash,
                        total_shares_accepted,
                        total_share_work_sum,
                        timestamp
                    )
                    .as_bytes(),
                ).await.map_err(|e| {
                    error!(target = "share_file_handler", "{}", e);
                });
            }
            ShareAccountingEvent::BestDifficultyUpdated {
                channel_id,
                new_best_diff,
                previous_best_diff,
                timestamp,
            } => {
                let _ = self.file.write_all(
                    format!(
                        "BestDifficultyUpdated: channel_id: {}, new_best_diff: {}, previous_best_diff: {}, timestamp: {:#?}\n",
                        channel_id,
                        new_best_diff,
                        previous_best_diff,
                        timestamp
                    )
                    .as_bytes(),
                ).await
                .map_err(|e| {
                    error!(target = "share_file_handler", "{}", e);
                });
            },
        }
    }
}

pub struct ShareFilePersistence {
    sender: async_channel::Sender<ShareAccountingEvent>,
}

impl Persistence for ShareFilePersistence {
    type Sender = async_channel::Sender<ShareAccountingEvent>;

    fn persist_event(&self, event: ShareAccountingEvent) {
        let _ = self
            .sender
            .try_send(event)
            .map_err(|e| error!(target = "share_file_persistence", "{}", e));
    }

    fn new(sender: Self::Sender) -> Self {
        Self { sender }
    }
}
