use roles_logic_sv2::{
    common_messages_sv2::{
        ChannelEndpointChanged, SetupConnection, SetupConnectionError, SetupConnectionSuccess,
    },
    job_declaration_sv2::{
        AllocateMiningJobToken, AllocateMiningJobTokenSuccess, DeclareMiningJob,
        DeclareMiningJobError, DeclareMiningJobSuccess, IdentifyTransactions,
        IdentifyTransactionsSuccess, ProvideMissingTransactions, ProvideMissingTransactionsSuccess,
    },
    mining_sv2::{
        CloseChannel, NewExtendedMiningJob, NewMiningJob, OpenExtendedMiningChannel,
        OpenExtendedMiningChannelSuccess, OpenMiningChannelError, OpenStandardMiningChannel,
        OpenStandardMiningChannelSuccess, Reconnect, SetCustomMiningJob, SetCustomMiningJobError,
        SetCustomMiningJobSuccess, SetExtranoncePrefix, SetGroupChannel,
        SetNewPrevHash as MiningSetNewPrevHash, SetTarget, SubmitSharesError, SubmitSharesExtended,
        SubmitSharesStandard, SubmitSharesSuccess, UpdateChannel, UpdateChannelError,
    },
    parsers::{self, AnyMessage, CommonMessages, PoolMessages},
    template_distribution_sv2::{
        CoinbaseOutputDataSize, NewTemplate, RequestTransactionData, RequestTransactionDataError,
        RequestTransactionDataSuccess, SubmitSolution,
    },
};

pub fn into_static(m: AnyMessage<'_>) -> AnyMessage<'static> {
    match m {
        PoolMessages::Common(m) => match m {
            CommonMessages::ChannelEndpointChanged(m) => PoolMessages::Common(
                CommonMessages::ChannelEndpointChanged(ChannelEndpointChanged {
                    channel_id: m.channel_id,
                }),
            ),
            CommonMessages::SetupConnection(m) => {
                let m = SetupConnection {
                    protocol: m.protocol,
                    min_version: m.min_version,
                    max_version: m.max_version,
                    flags: m.flags,
                    endpoint_host: m.endpoint_host.into_static(),
                    endpoint_port: m.endpoint_port,
                    vendor: m.vendor.into_static(),
                    hardware_version: m.hardware_version.into_static(),
                    firmware: m.firmware.into_static(),
                    device_id: m.device_id.into_static(),
                };
                PoolMessages::Common(CommonMessages::SetupConnection(m))
            }
            CommonMessages::SetupConnectionError(m) => {
                let m = SetupConnectionError {
                    flags: m.flags,
                    error_code: m.error_code.into_static(),
                };
                PoolMessages::Common(CommonMessages::SetupConnectionError(m))
            }
            CommonMessages::SetupConnectionSuccess(m) => {
                let m = SetupConnectionSuccess {
                    used_version: m.used_version,
                    flags: m.flags,
                };
                PoolMessages::Common(CommonMessages::SetupConnectionSuccess(m))
            }
        },
        PoolMessages::Mining(m) => match m {
            parsers::Mining::CloseChannel(m) => {
                let m = CloseChannel {
                    channel_id: m.channel_id,
                    reason_code: m.reason_code.into_static(),
                };
                PoolMessages::Mining(parsers::Mining::CloseChannel(m))
            }
            parsers::Mining::NewExtendedMiningJob(m) => {
                let m = NewExtendedMiningJob {
                    channel_id: m.channel_id,
                    job_id: m.job_id,
                    min_ntime: m.min_ntime.into_static(),
                    version: m.version,
                    version_rolling_allowed: m.version_rolling_allowed,
                    merkle_path: m.merkle_path.into_static(),
                    coinbase_tx_prefix: m.coinbase_tx_prefix.into_static(),
                    coinbase_tx_suffix: m.coinbase_tx_suffix.into_static(),
                };
                PoolMessages::Mining(parsers::Mining::NewExtendedMiningJob(m))
            }
            parsers::Mining::NewMiningJob(m) => {
                let m = NewMiningJob {
                    channel_id: m.channel_id,
                    job_id: m.job_id,
                    min_ntime: m.min_ntime.into_static(),
                    version: m.version,
                    merkle_root: m.merkle_root.into_static(),
                };
                PoolMessages::Mining(parsers::Mining::NewMiningJob(m))
            }
            parsers::Mining::OpenExtendedMiningChannel(m) => {
                let m = OpenExtendedMiningChannel {
                    request_id: m.request_id,
                    user_identity: m.user_identity.into_static(),
                    nominal_hash_rate: m.nominal_hash_rate,
                    max_target: m.max_target.into_static(),
                    min_extranonce_size: m.min_extranonce_size,
                };
                PoolMessages::Mining(parsers::Mining::OpenExtendedMiningChannel(m))
            }
            parsers::Mining::OpenExtendedMiningChannelSuccess(m) => {
                let m = OpenExtendedMiningChannelSuccess {
                    request_id: m.request_id,
                    channel_id: m.channel_id,
                    target: m.target.into_static(),
                    extranonce_size: m.extranonce_size,
                    extranonce_prefix: m.extranonce_prefix.into_static(),
                };
                PoolMessages::Mining(parsers::Mining::OpenExtendedMiningChannelSuccess(m))
            }
            parsers::Mining::OpenMiningChannelError(m) => {
                let m = OpenMiningChannelError {
                    request_id: m.request_id,
                    error_code: m.error_code.into_static(),
                };
                PoolMessages::Mining(parsers::Mining::OpenMiningChannelError(m))
            }
            parsers::Mining::OpenStandardMiningChannel(m) => {
                let m = OpenStandardMiningChannel {
                    request_id: m.request_id,
                    user_identity: m.user_identity.into_static(),
                    nominal_hash_rate: m.nominal_hash_rate,
                    max_target: m.max_target.into_static(),
                };
                PoolMessages::Mining(parsers::Mining::OpenStandardMiningChannel(m))
            }
            parsers::Mining::OpenStandardMiningChannelSuccess(m) => {
                let m = OpenStandardMiningChannelSuccess {
                    request_id: m.request_id,
                    channel_id: m.channel_id,
                    target: m.target.into_static(),
                    extranonce_prefix: m.extranonce_prefix.into_static(),
                    group_channel_id: m.group_channel_id,
                };
                PoolMessages::Mining(parsers::Mining::OpenStandardMiningChannelSuccess(m))
            }
            parsers::Mining::Reconnect(m) => {
                let m = Reconnect {
                    new_host: m.new_host.into_static(),
                    new_port: m.new_port,
                };
                PoolMessages::Mining(parsers::Mining::Reconnect(m))
            }
            parsers::Mining::SetCustomMiningJob(m) => {
                let m = SetCustomMiningJob {
                    channel_id: m.channel_id,
                    request_id: m.request_id,
                    token: m.token.into_static(),
                    version: m.version,
                    prev_hash: m.prev_hash.into_static(),
                    min_ntime: m.min_ntime,
                    nbits: m.nbits,
                    coinbase_tx_version: m.coinbase_tx_version,
                    coinbase_prefix: m.coinbase_prefix.into_static(),
                    coinbase_tx_input_n_sequence: m.coinbase_tx_input_n_sequence,
                    coinbase_tx_value_remaining: m.coinbase_tx_value_remaining,
                    coinbase_tx_outputs: m.coinbase_tx_outputs.into_static(),
                    coinbase_tx_locktime: m.coinbase_tx_locktime,
                    merkle_path: m.merkle_path.into_static(),
                    extranonce_size: m.extranonce_size,
                };
                PoolMessages::Mining(parsers::Mining::SetCustomMiningJob(m))
            }
            parsers::Mining::SetCustomMiningJobError(m) => {
                let m = SetCustomMiningJobError {
                    channel_id: m.channel_id,
                    request_id: m.request_id,
                    error_code: m.error_code.into_static(),
                };
                PoolMessages::Mining(parsers::Mining::SetCustomMiningJobError(m))
            }
            parsers::Mining::SetCustomMiningJobSuccess(m) => {
                let m = SetCustomMiningJobSuccess {
                    channel_id: m.channel_id,
                    request_id: m.request_id,
                    job_id: m.job_id,
                };
                PoolMessages::Mining(parsers::Mining::SetCustomMiningJobSuccess(m))
            }
            parsers::Mining::SetExtranoncePrefix(m) => {
                let m = SetExtranoncePrefix {
                    channel_id: m.channel_id,
                    extranonce_prefix: m.extranonce_prefix.into_static(),
                };
                PoolMessages::Mining(parsers::Mining::SetExtranoncePrefix(m))
            }
            parsers::Mining::SetGroupChannel(m) => {
                let m = SetGroupChannel {
                    group_channel_id: m.group_channel_id,
                    channel_ids: m.channel_ids.into_static(),
                };
                PoolMessages::Mining(parsers::Mining::SetGroupChannel(m))
            }
            parsers::Mining::SetNewPrevHash(m) => {
                let m = MiningSetNewPrevHash {
                    channel_id: m.channel_id,
                    job_id: m.job_id,
                    prev_hash: m.prev_hash.into_static(),
                    min_ntime: m.min_ntime,
                    nbits: m.nbits,
                };
                PoolMessages::Mining(parsers::Mining::SetNewPrevHash(m))
            }
            parsers::Mining::SetTarget(m) => {
                let m = SetTarget {
                    channel_id: m.channel_id,
                    maximum_target: m.maximum_target.into_static(),
                };
                PoolMessages::Mining(parsers::Mining::SetTarget(m))
            }
            parsers::Mining::SubmitSharesError(m) => {
                let m = SubmitSharesError {
                    channel_id: m.channel_id,
                    sequence_number: m.sequence_number,
                    error_code: m.error_code.into_static(),
                };
                PoolMessages::Mining(parsers::Mining::SubmitSharesError(m))
            }
            parsers::Mining::SubmitSharesExtended(m) => {
                let m = SubmitSharesExtended {
                    channel_id: m.channel_id,
                    sequence_number: m.sequence_number,
                    job_id: m.job_id,
                    nonce: m.nonce,
                    ntime: m.ntime,
                    version: m.version,
                    extranonce: m.extranonce.into_static(),
                };
                PoolMessages::Mining(parsers::Mining::SubmitSharesExtended(m))
            }
            parsers::Mining::SubmitSharesStandard(m) => {
                let m = SubmitSharesStandard {
                    channel_id: m.channel_id,
                    sequence_number: m.sequence_number,
                    job_id: m.job_id,
                    nonce: m.nonce,
                    ntime: m.ntime,
                    version: m.version,
                };
                PoolMessages::Mining(parsers::Mining::SubmitSharesStandard(m))
            }
            parsers::Mining::SubmitSharesSuccess(m) => {
                let m = SubmitSharesSuccess {
                    channel_id: m.channel_id,
                    last_sequence_number: m.last_sequence_number,
                    new_submits_accepted_count: m.new_submits_accepted_count,
                    new_shares_sum: m.new_shares_sum,
                };
                PoolMessages::Mining(parsers::Mining::SubmitSharesSuccess(m))
            }
            parsers::Mining::UpdateChannel(m) => {
                let m = UpdateChannel {
                    channel_id: m.channel_id,
                    nominal_hash_rate: m.nominal_hash_rate,
                    maximum_target: m.maximum_target.into_static(),
                };
                PoolMessages::Mining(parsers::Mining::UpdateChannel(m))
            }
            parsers::Mining::UpdateChannelError(m) => {
                let m = UpdateChannelError {
                    channel_id: m.channel_id,
                    error_code: m.error_code.into_static(),
                };
                PoolMessages::Mining(parsers::Mining::UpdateChannelError(m))
            }
        },
        PoolMessages::JobDeclaration(m) => match m {
            parsers::JobDeclaration::AllocateMiningJobToken(m) => {
                let m = AllocateMiningJobToken {
                    user_identifier: m.user_identifier.into_static(),
                    request_id: m.request_id,
                };
                PoolMessages::JobDeclaration(parsers::JobDeclaration::AllocateMiningJobToken(m))
            }
            parsers::JobDeclaration::AllocateMiningJobTokenSuccess(m) => {
                let m = AllocateMiningJobTokenSuccess {
                    request_id: m.request_id,
                    mining_job_token: m.mining_job_token.into_static(),
                    coinbase_output_max_additional_size: m.coinbase_output_max_additional_size,
                    coinbase_output: m.coinbase_output.into_static(),
                    async_mining_allowed: m.async_mining_allowed,
                };
                PoolMessages::JobDeclaration(
                    parsers::JobDeclaration::AllocateMiningJobTokenSuccess(m),
                )
            }
            parsers::JobDeclaration::DeclareMiningJob(m) => {
                let m = DeclareMiningJob {
                    request_id: m.request_id,
                    mining_job_token: m.mining_job_token.into_static(),
                    version: m.version,
                    coinbase_prefix: m.coinbase_prefix.into_static(),
                    coinbase_suffix: m.coinbase_suffix.into_static(),
                    tx_short_hash_nonce: m.tx_short_hash_nonce,
                    tx_short_hash_list: m.tx_short_hash_list.into_static(),
                    tx_hash_list_hash: m.tx_hash_list_hash.into_static(),
                    excess_data: m.excess_data.into_static(),
                };
                PoolMessages::JobDeclaration(parsers::JobDeclaration::DeclareMiningJob(m))
            }
            parsers::JobDeclaration::DeclareMiningJobSuccess(m) => {
                let m = DeclareMiningJobSuccess {
                    request_id: m.request_id,
                    new_mining_job_token: m.new_mining_job_token.into_static(),
                };
                PoolMessages::JobDeclaration(parsers::JobDeclaration::DeclareMiningJobSuccess(m))
            }
            parsers::JobDeclaration::DeclareMiningJobError(m) => {
                let m = DeclareMiningJobError {
                    request_id: m.request_id,
                    error_code: m.error_code.into_static(),
                    error_details: m.error_details.into_static(),
                };
                PoolMessages::JobDeclaration(parsers::JobDeclaration::DeclareMiningJobError(m))
            }
            parsers::JobDeclaration::IdentifyTransactions(m) => {
                let m = IdentifyTransactions {
                    request_id: m.request_id,
                };
                PoolMessages::JobDeclaration(parsers::JobDeclaration::IdentifyTransactions(m))
            }
            parsers::JobDeclaration::IdentifyTransactionsSuccess(m) => {
                let m = IdentifyTransactionsSuccess {
                    request_id: m.request_id,
                    tx_data_hashes: m.tx_data_hashes.into_static(),
                };
                PoolMessages::JobDeclaration(parsers::JobDeclaration::IdentifyTransactionsSuccess(
                    m,
                ))
            }
            parsers::JobDeclaration::ProvideMissingTransactions(m) => {
                let m = ProvideMissingTransactions {
                    request_id: m.request_id,
                    unknown_tx_position_list: m.unknown_tx_position_list.into_static(),
                };
                PoolMessages::JobDeclaration(parsers::JobDeclaration::ProvideMissingTransactions(m))
            }
            parsers::JobDeclaration::ProvideMissingTransactionsSuccess(m) => {
                let m = ProvideMissingTransactionsSuccess {
                    request_id: m.request_id,
                    transaction_list: m.transaction_list.into_static(),
                };
                PoolMessages::JobDeclaration(
                    parsers::JobDeclaration::ProvideMissingTransactionsSuccess(m),
                )
            }
            parsers::JobDeclaration::SubmitSolution(_m) => {
                todo!()
            }
        },
        PoolMessages::TemplateDistribution(m) => match m {
            parsers::TemplateDistribution::CoinbaseOutputDataSize(m) => {
                let m = CoinbaseOutputDataSize {
                    coinbase_output_max_additional_size: m.coinbase_output_max_additional_size,
                };
                PoolMessages::TemplateDistribution(
                    parsers::TemplateDistribution::CoinbaseOutputDataSize(m),
                )
            }
            parsers::TemplateDistribution::NewTemplate(m) => {
                let m = NewTemplate {
                    template_id: m.template_id,
                    future_template: m.future_template,
                    version: m.version,
                    coinbase_tx_version: m.coinbase_tx_version,
                    coinbase_prefix: m.coinbase_prefix.into_static(),
                    coinbase_tx_input_sequence: m.coinbase_tx_input_sequence,
                    coinbase_tx_value_remaining: m.coinbase_tx_value_remaining,
                    coinbase_tx_outputs_count: m.coinbase_tx_outputs_count,
                    coinbase_tx_outputs: m.coinbase_tx_outputs.into_static(),
                    coinbase_tx_locktime: m.coinbase_tx_locktime,
                    merkle_path: m.merkle_path.into_static(),
                };
                PoolMessages::TemplateDistribution(parsers::TemplateDistribution::NewTemplate(m))
            }
            parsers::TemplateDistribution::RequestTransactionData(m) => {
                let m = RequestTransactionData {
                    template_id: m.template_id,
                };
                PoolMessages::TemplateDistribution(
                    parsers::TemplateDistribution::RequestTransactionData(m),
                )
            }
            parsers::TemplateDistribution::RequestTransactionDataError(m) => {
                let m = RequestTransactionDataError {
                    template_id: m.template_id,
                    error_code: m.error_code.into_static(),
                };
                PoolMessages::TemplateDistribution(
                    parsers::TemplateDistribution::RequestTransactionDataError(m),
                )
            }
            parsers::TemplateDistribution::RequestTransactionDataSuccess(m) => {
                let m = RequestTransactionDataSuccess {
                    template_id: m.template_id,
                    excess_data: m.excess_data.into_static(),
                    transaction_list: m.transaction_list.into_static(), // TODO! DA RIVEDERE!
                };
                PoolMessages::TemplateDistribution(
                    parsers::TemplateDistribution::RequestTransactionDataSuccess(m),
                )
            }
            parsers::TemplateDistribution::SetNewPrevHash(m) => {
                let m = roles_logic_sv2::template_distribution_sv2::SetNewPrevHash {
                    template_id: m.template_id,
                    prev_hash: m.prev_hash.into_static(),
                    header_timestamp: m.header_timestamp,
                    n_bits: m.n_bits,
                    target: m.target.into_static(),
                };
                PoolMessages::TemplateDistribution(parsers::TemplateDistribution::SetNewPrevHash(m))
            }
            parsers::TemplateDistribution::SubmitSolution(m) => {
                let m = SubmitSolution {
                    template_id: m.template_id,
                    version: m.version,
                    header_timestamp: m.header_timestamp,
                    header_nonce: m.header_nonce,
                    coinbase_tx: m.coinbase_tx.into_static(),
                };
                PoolMessages::TemplateDistribution(parsers::TemplateDistribution::SubmitSolution(m))
            }
        },
    }
}
