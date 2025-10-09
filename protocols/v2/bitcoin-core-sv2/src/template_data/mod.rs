mod error;

use bitcoin_capnp::{
    mining_capnp::block_template::Client as BlockTemplateIpcClient,
    proxy_capnp::thread::Client as ThreadIpcClient,
};
use error::TemplateDataError;
use roles_logic_sv2::bitcoin::{
    Target, Transaction, TxOut,
    amount::{Amount, CheckedSum},
    block::{Block, Header, Version},
    consensus::{deserialize, serialize},
    hashes::{Hash, HashEngine, sha256d},
};

use binary_sv2::{B016M, B064K, B0255, Seq064K, Seq0255, U256};
use template_distribution_sv2::{
    NewTemplate, RequestTransactionDataSuccess, SetNewPrevHash, SubmitSolution,
};

#[derive(Clone)]
pub struct TemplateData {
    template_id: u64,
    block: Block,
    template_ipc_client: BlockTemplateIpcClient,
}

// impl block for public methods
impl TemplateData {
    pub fn new(
        template_id: u64,
        block: Block,
        template_ipc_client: BlockTemplateIpcClient,
    ) -> Self {
        Self {
            template_id,
            block,
            template_ipc_client,
        }
    }

    pub async fn destroy_ipc_client(
        &self,
        thread_ipc_client: ThreadIpcClient,
    ) -> Result<(), TemplateDataError> {
        tracing::debug!("Destroying template IPC client: {}", self.template_id);
        let mut destroy_ipc_client_request = self.template_ipc_client.destroy_request();
        let destroy_ipc_client_request_params = destroy_ipc_client_request.get();

        destroy_ipc_client_request_params
            .get_context()?
            .set_thread(thread_ipc_client);

        destroy_ipc_client_request.send().promise.await?;

        Ok(())
    }

    pub fn get_template_id(&self) -> u64 {
        self.template_id
    }

    pub fn get_new_template_message(&self, future_template: bool) -> NewTemplate<'static> {
        let new_template = NewTemplate {
            template_id: self.template_id,
            future_template,
            version: self.get_version(),
            coinbase_tx_version: self.get_coinbase_tx_version(),
            coinbase_prefix: self.get_coinbase_script_sig(),
            coinbase_tx_input_sequence: self.get_coinbase_input_sequence(),
            coinbase_tx_value_remaining: self.get_coinbase_tx_value_remaining(),
            coinbase_tx_outputs_count: self.get_empty_coinbase_outputs().len() as u32,
            coinbase_tx_outputs: self.get_serialized_empty_coinbase_outputs(),
            coinbase_tx_locktime: self.get_coinbase_tx_lock_time(),
            merkle_path: self.get_merkle_path(),
        };
        new_template.into_static()
    }

    // please note that `SetNewPrevHash.target` is consensus and not weak-block
    // so it's essentially redundant with `SetNewPrevHash.n_bits`
    pub fn get_set_new_prev_hash_message(&self) -> SetNewPrevHash<'static> {
        let set_new_prev_hash = SetNewPrevHash {
            template_id: self.template_id,
            prev_hash: self.get_prev_hash(),
            header_timestamp: self.get_ntime(),
            n_bits: self.get_nbits(),
            target: self.get_target(),
        };
        set_new_prev_hash.into_static()
    }

    pub fn get_request_transaction_data_success_message(
        &self,
    ) -> RequestTransactionDataSuccess<'static> {
        let request_transaction_data_success = RequestTransactionDataSuccess {
            template_id: self.template_id,
            transaction_list: self.get_tx_data(),
            excess_data: vec![]
                .try_into()
                .expect("empty vec should always be valid for B064K"),
        };
        request_transaction_data_success.into_static()
    }

    pub fn get_prev_hash(&self) -> U256<'static> {
        self.block.header.prev_blockhash.to_byte_array().into()
    }

    pub async fn submit_solution(
        &self,
        submit_solution: SubmitSolution<'static>,
        thread_ipc_client: ThreadIpcClient,
    ) -> Result<(), TemplateDataError> {
        let coinbase_tx_bytes: Vec<u8> = submit_solution.coinbase_tx.to_vec();

        let coinbase_tx: Transaction = match deserialize(&coinbase_tx_bytes) {
            Ok(coinbase_tx) => coinbase_tx,
            Err(e) => {
                tracing::error!("SubmitSolution.coinbase_tx is invalid: {}", e);
                return Err(TemplateDataError::InvalidCoinbaseTx(e));
            }
        };

        let solution_header = Header {
            version: Version::from_consensus(submit_solution.version as i32),
            prev_blockhash: self.block.header.prev_blockhash,
            merkle_root: {
                let mut tmp_block = self.block.clone();
                // replace dummy coinbase tx with the actual coinbase tx from the solution
                tmp_block.txdata[0] = coinbase_tx;
                tmp_block
                    .compute_merkle_root()
                    .ok_or(TemplateDataError::InvalidMerkleRoot)?
            },
            time: submit_solution.header_timestamp,
            nonce: submit_solution.header_nonce,
            bits: self.block.header.bits,
        };

        if let Err(e) = solution_header.validate_pow(solution_header.target()) {
            tracing::error!("SubmitSolution solution header is invalid: {}", e);
            return Err(TemplateDataError::InvalidSolution(e));
        }

        let mut submit_solution_request = self.template_ipc_client.submit_solution_request();
        let mut submit_solution_request_params = submit_solution_request.get();

        submit_solution_request_params.set_version(submit_solution.version);
        submit_solution_request_params.set_timestamp(submit_solution.header_timestamp);
        submit_solution_request_params.set_nonce(submit_solution.header_nonce);
        submit_solution_request_params.set_coinbase(&coinbase_tx_bytes);

        submit_solution_request_params
            .get_context()?
            .set_thread(thread_ipc_client.clone());

        let submit_solution_response = submit_solution_request.send().promise.await?;

        if !submit_solution_response.get()?.get_result() {
            return Err(TemplateDataError::FailedIpcSubmitSolution);
        }

        Ok(())
    }
}

// impl block for private methods
impl TemplateData {
    fn get_nbits(&self) -> u32 {
        self.block.header.bits.to_consensus()
    }

    fn get_target(&self) -> U256 {
        let target = Target::from(self.block.header.bits);
        let target_bytes: [u8; 32] = target.to_le_bytes();
        U256::from(target_bytes)
    }

    fn get_ntime(&self) -> u32 {
        self.block.header.time
    }

    fn get_version(&self) -> u32 {
        self.block
            .header
            .version
            .to_consensus()
            .try_into()
            .expect("block version conversion to u32 should never fail")
    }

    fn get_coinbase_tx_version(&self) -> u32 {
        self.block.txdata[0]
            .version
            .0
            .try_into()
            .expect("coinbase version conversion to u32 should never fail")
    }

    fn get_coinbase_script_sig(&self) -> B0255 {
        let coinbase_script_sig: B0255 = self.block.txdata[0].input[0]
            .script_sig
            .to_bytes()
            .try_into()
            .expect("coinbase script sig should always be valid for B0255");
        coinbase_script_sig
    }

    fn get_coinbase_input_sequence(&self) -> u32 {
        self.block.txdata[0].input[0].sequence.to_consensus_u32()
    }

    fn get_empty_coinbase_outputs(&self) -> Vec<TxOut> {
        self.block.txdata[0]
            .output
            .iter()
            .filter(|output| output.value == Amount::from_sat(0))
            .cloned()
            .collect()
    }

    fn get_serialized_empty_coinbase_outputs(&self) -> B064K {
        let empty_coinbase_outputs = self.get_empty_coinbase_outputs();
        let mut serialized_empty_coinbase_outputs = Vec::new();
        for output in empty_coinbase_outputs {
            serialized_empty_coinbase_outputs.extend_from_slice(&serialize(&output));
        }
        let serialized_empty_coinbase_outputs: B064K = serialized_empty_coinbase_outputs
            .try_into()
            .expect("serialized empty coinbase outputs should always be valid for B064K");
        serialized_empty_coinbase_outputs
    }

    fn get_coinbase_tx_value_remaining(&self) -> u64 {
        self.block.txdata[0]
            .output
            .iter()
            .map(|output| output.value)
            .checked_sum()
            .expect("coinbase output value should never overflow")
            .to_sat()
    }

    fn get_coinbase_tx_lock_time(&self) -> u32 {
        self.block.txdata[0].lock_time.to_consensus_u32()
    }

    fn get_tx_data(&self) -> Seq064K<B016M<'static>> {
        let tx_data: Vec<B016M<'static>> = self
            .block
            .txdata
            .iter()
            .map(|tx| {
                serialize(tx)
                    .try_into()
                    .expect("tx data should always be valid for B016M")
            })
            .collect();
        Seq064K::new(tx_data).expect("tx data should always be valid for Seq064K")
    }

    fn get_merkle_path(&self) -> Seq0255<U256> {
        let tx_hashes: Vec<sha256d::Hash> = self
            .block
            .txdata
            .iter()
            .map(|tx| tx.compute_txid().to_raw_hash())
            .collect();

        if tx_hashes.len() == 1 {
            // If there's only the coinbase transaction, the path is empty
            return Seq0255::new(Vec::new())
                .expect("Empty vector should always be valid for Seq0255");
        }

        let mut merkle_path = Vec::new();
        let mut current_level = tx_hashes;
        let mut target_index = 0; // Start with coinbase at index 0

        while current_level.len() > 1 {
            let mut next_level = Vec::new();
            let next_target_index = target_index / 2;

            // Find the sibling of the target transaction at this level
            let sibling_index = if target_index % 2 == 0 {
                // Target is left child, sibling is right child
                target_index + 1
            } else {
                // Target is right child, sibling is left child
                target_index - 1
            };

            // Add the sibling hash to the merkle path
            if sibling_index < current_level.len() {
                let hash_bytes: [u8; 32] = *current_level[sibling_index].as_byte_array();
                let u256_hash = U256::try_from(hash_bytes.to_vec())
                    .expect("32-byte hash should always be valid for U256");
                merkle_path.push(u256_hash);
            } else {
                // If no sibling (odd number of nodes), duplicate the last hash
                let hash_bytes: [u8; 32] = *current_level[target_index].as_byte_array();
                let u256_hash = U256::try_from(hash_bytes.to_vec())
                    .expect("32-byte hash should always be valid for U256");
                merkle_path.push(u256_hash);
            }

            // Calculate the next level of the merkle tree
            for i in (0..current_level.len()).step_by(2) {
                let left = current_level[i];
                let right = if i + 1 < current_level.len() {
                    current_level[i + 1]
                } else {
                    left // Duplicate if odd number of hashes
                };

                // Compute parent hash: SHA256(SHA256(left || right))
                let mut hasher = sha256d::Hash::engine();
                HashEngine::input(&mut hasher, left.as_byte_array());
                HashEngine::input(&mut hasher, right.as_byte_array());
                let parent_hash = sha256d::Hash::from_engine(hasher);
                next_level.push(parent_hash);
            }

            current_level = next_level;
            target_index = next_target_index;
        }

        Seq0255::new(merkle_path).expect("Merkle path should always be valid for Seq0255")
    }
}
