use stratum_apps::stratum_core::channels_sv2::persistence::{
    PersistenceHandler, ShareAccountingEvent,
};
use tokio::io::AsyncWriteExt;
use tracing::error;

use crate::status::{self, StatusSender};

pub struct ShareFileHandler {
    file: tokio::fs::File,
    receiver: async_channel::Receiver<ShareAccountingEvent>,
    sender: async_channel::Sender<ShareAccountingEvent>,
    status_tx: StatusSender,
}

impl ShareFileHandler {
    pub async fn new(path: &str, status_tx: StatusSender) -> Self {
        let file = tokio::fs::File::create(path).await.unwrap();
        let (sender, receiver) = async_channel::bounded(1024);
        Self {
            file,
            receiver,
            sender,
            status_tx,
        }
    }

    pub fn get_receiver(&self) -> async_channel::Receiver<ShareAccountingEvent> {
        self.receiver.clone()
    }

    pub fn get_sender(&self) -> async_channel::Sender<ShareAccountingEvent> {
        self.sender.clone()
    }

    pub async fn write_event_to_file(&mut self, event: ShareAccountingEvent) {
        match event {
            ShareAccountingEvent::ShareAccepted {
                channel_id,
                user_identity,
                share_work,
                share_sequence_number,
                share_hash,
                total_shares_accepted,
                total_share_work_sum,
                timestamp,
                block_found,
            } => {
                let result = self.file.write_all(
                    format!(
                        "ShareAccepted: channel_id: {}, user_identity: {}, share_work: {}, share_sequence_number: {}, share_hash: {}, total_shares_accepted: {}, total_share_work_sum: {}, timestamp: {:?}, block_found: {}\n",
                        channel_id,
                        user_identity,
                        share_work,
                        share_sequence_number,
                        share_hash,
                        total_shares_accepted,
                        total_share_work_sum,
                        timestamp,
                        block_found
                    )
                    .as_bytes(),
                ).await;

                if let Err(e) = result {
                    error!(
                        target = "share_file_handler",
                        "Failed to write share event: {}", e
                    );
                    let _ = self
                        .status_tx
                        .send(status::Status {
                            state: status::State::SharePersistenceError(format!(
                                "Failed to write share event: {}",
                                e
                            )),
                        })
                        .await;
                } else if block_found {
                    let _ = self
                        .status_tx
                        .send(status::Status {
                            state: status::State::Healthy(format!(
                                "Block found! channel_id: {}, user: {}",
                                channel_id, user_identity
                            )),
                        })
                        .await;
                }
            }
            ShareAccountingEvent::BestDifficultyUpdated {
                channel_id,
                new_best_diff,
                previous_best_diff,
                timestamp,
            } => {
                let result = self.file.write_all(
                    format!(
                        "BestDifficultyUpdated: channel_id: {}, new_best_diff: {}, previous_best_diff: {}, timestamp: {:?}\n",
                        channel_id,
                        new_best_diff,
                        previous_best_diff,
                        timestamp
                    )
                    .as_bytes(),
                ).await;

                if let Err(e) = result {
                    error!(
                        target = "share_file_handler",
                        "Failed to write difficulty update: {}", e
                    );
                    let _ = self
                        .status_tx
                        .send(status::Status {
                            state: status::State::SharePersistenceError(format!(
                                "Failed to write difficulty update: {}",
                                e
                            )),
                        })
                        .await;
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct ShareFilePersistence {
    sender: async_channel::Sender<ShareAccountingEvent>,
}

impl ShareFilePersistence {
    pub fn new(sender: async_channel::Sender<ShareAccountingEvent>) -> Self {
        Self { sender }
    }
}

impl PersistenceHandler for ShareFilePersistence {
    fn persist_event(&self, event: ShareAccountingEvent) {
        let _ = self
            .sender
            .try_send(event)
            .map_err(|e| error!(target = "share_file_persistence", "{}", e));
    }
}
