use async_channel::Sender;
use binary_sv2::{Seq0255, U256};
use roles_logic_sv2::template_distribution_sv2::{NewTemplate, SetNewPrevHash};
use std::convert::TryInto;
use tokio::task;

pub struct TestTemplateRx {}

impl TestTemplateRx {
    pub async fn start(
        templ_sender: Sender<NewTemplate<'static>>,
        prev_h_sender: Sender<SetNewPrevHash<'static>>,
    ) {
        let template_id = 1645800496;
        let merkle_path = vec![
            160, 143, 112, 229, 220, 31, 192, 74, 203, 122, 142, 130, 166, 171, 230, 144, 131, 143,
            120, 245, 157, 13, 149, 68, 207, 35, 63, 209, 126, 178, 175, 139,
        ];
        let merkle_path: U256 = merkle_path.try_into().unwrap();
        let new_template = NewTemplate {
            template_id,
            future_template: false,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![104, 3, 94, 49, 22].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: 0,
            coinbase_tx_outputs_count: 0,
            coinbase_tx_outputs: vec![].try_into().unwrap(),
            coinbase_tx_locktime: 0,
            merkle_path: Seq0255::new(vec![merkle_path]).unwrap(),
        };
        let new_prev_hash = SetNewPrevHash {
            template_id,
            prev_hash: vec![6; 32].try_into().unwrap(),
            header_timestamp: 0,
            n_bits: 0,
            target: vec![3; 32].try_into().unwrap(),
        };
        task::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(20)).await;
            templ_sender.send(new_template).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            prev_h_sender.send(new_prev_hash).await.unwrap();
        });
    }
}
