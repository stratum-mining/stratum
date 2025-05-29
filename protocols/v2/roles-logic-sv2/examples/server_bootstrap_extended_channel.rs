/// This example shows how a Mining Server can bootstrap an extended channel,
/// after receiving an OpenExtendedMiningChannel request from a client.
///
/// We abstract AppContext as the global context of an application that will:
/// - open connections with Mining Clients
/// - receive templates from a Template Provider
/// - use those templates to create jobs for Mining Clients
/// - receive requests to open new channels
///
/// For example: a mining server under the infrastructure of a Pool, or a Job Declaration
/// Client.
///
/// The most important aspects of AppContext are:
/// - it ensures unique extranonce prefix allocation space across all channels, which is
///   crucial for avoiding search space collisions
/// - it has at least one cached future template and a SetNewPrevHash message associated with
///   this future template, which is used to create and activate the future job during the
///   process of bootstrapping a new channel
/// - it provides the script pubkey(s) for the coinbase reward outputs that will be used on the
///   jobs
///
/// We abstract ConnectionContext as the context of a single connection with a specific client,
/// where multiple channels can be created and managed.
///
/// The most important aspects of ConnectionContext are:
/// - it ensures unique channel id allocation space across all channels
/// - it has a share_batch_size parameter, which is used to establish after how many valid
///   shares a SubmitSharesSuccess message is sent to the client
/// - it has an expected_share_per_minute parameter, which is used to calculate the initial
///   channel target based on the advertised nominal hashrate
///
/// Both share_batch_size and expected_share_per_minute could also be defined at AppContext
/// level and shared across the ConnnectionContext of each client.
///
/// When a new extended channel is created, the following messages need to be sent to the
/// client:
/// - OpenExtendedMiningChannelSuccess
/// - NewExtendedMiningJob (no min_ntime, making it a future job)
/// - SetNewPrevHash (mining protocol variant) (job_id field argets the future job sent
///   immediately before)
///
/// Note: we make no assumptions about:
/// - how the network stack is implemented
/// - how collections of channels are managed
/// - how concurrency safety is handled
use core::convert::TryInto;
use mining_sv2::{
    ExtendedExtranonce, OpenExtendedMiningChannel, OpenExtendedMiningChannelSuccess,
    OpenMiningChannelError, SetNewPrevHash as SetNewPrevHashMp, MAX_EXTRANONCE_LEN,
};
use roles_logic_sv2::{
    channels::server::{error::ExtendedChannelError, extended::ExtendedChannel},
    utils::Id as IdFactory,
};
use stratum_common::bitcoin::{transaction::TxOut, Amount, ScriptBuf};
use template_distribution_sv2::{NewTemplate, SetNewPrevHash as SetNewPrevHashTdp};

fn main() {
    // ------------------------------------------------------------
    // basic context of a channel creation

    // app context ensures:
    // - unique extranonce prefix allocation space
    // - cached future template and SetNewPrevHash message associated with this future template
    // - script pubkey(s) for the coinbase reward outputs
    let mut app_context = AppContext::new();

    // ------------------------------------------------------------

    // imagine the connection with the client has already been previously established
    // connection context ensures unique channel id allocation space
    let mut connection_context: ConnectionContext = ConnectionContext::new();

    // ------------------------------------------------------------

    // imagine an incoming request to open a new extended channel
    let open_extended_mining_channel_request = OpenExtendedMiningChannel {
        request_id: 1,
        user_identity: "user1".to_string().try_into().unwrap(),
        nominal_hash_rate: 1000.0,
        min_extranonce_size: 8,
        max_target: [0xff; 32].into(),
    };

    // ------------------------------------------------------------

    // the extranonce prefix is allocated from the app context
    // note that we always use the next_prefix_extended method during allocation, which takes
    // the min_extranonce_size as a parameter
    let extranonce_prefix = app_context
        .extranonce_prefix_factory_extended
        .next_prefix_extended(open_extended_mining_channel_request.min_extranonce_size as usize)
        .unwrap();

    // the channel id is allocated from the connection context
    let channel_id = connection_context.channel_id_factory.next();

    // ------------------------------------------------------------

    // try to create the channel
    let result = ExtendedChannel::new(
        channel_id,
        String::from_utf8(
            open_extended_mining_channel_request
                .user_identity
                .inner_as_ref()
                .to_vec(),
        )
        .unwrap(),
        extranonce_prefix.to_vec(),
        open_extended_mining_channel_request.max_target.into(),
        open_extended_mining_channel_request.nominal_hash_rate,
        true, // version_rolling_allowed
        open_extended_mining_channel_request.min_extranonce_size,
        connection_context.share_batch_size,
        connection_context.expected_share_per_minute,
    );

    // check if channel creation was successful
    let mut extended_channel = match result {
        Ok(extended_channel) => extended_channel,
        Err(e) => {
            let error_code = match e {
                ExtendedChannelError::InvalidNominalHashrate => {
                    "invalid-nominal-hashrate".to_string()
                }
                ExtendedChannelError::RequestedMaxTargetOutOfRange => {
                    "max-target-out-of-range".to_string()
                }
                ExtendedChannelError::RequestedMinExtranonceSizeTooLarge => {
                    "min-extranonce-size-too-large".to_string()
                }
                _ => {
                    unreachable!(
                        "nasty error that shouldn't even be notified to the client: {:?}",
                        e
                    );
                }
            };

            // at this point, we would have to send a OpenMiningChannelError over the wire to the
            // client with the appropriate error_code
            let _open_mining_channel_error = OpenMiningChannelError {
                request_id: open_extended_mining_channel_request.request_id,
                error_code: error_code.try_into().unwrap(),
            };

            return;
        }
    };

    // assuming everything went well, we would have to send the following message to the client
    // it informs it essential information, such as:
    // - channel_id
    // - target (based on the advertised nominal hashrate, and the expected share submission rate)
    // - extranonce_prefix (unique across the entire AppContext)
    // - rollable_extranonce_size (how many bytes the client can roll)
    let _open_extended_mining_channel_success = OpenExtendedMiningChannelSuccess {
        request_id: open_extended_mining_channel_request.request_id,
        channel_id,
        target: extended_channel.get_target().clone().into(),
        extranonce_prefix: extended_channel
            .get_extranonce_prefix()
            .clone()
            .try_into()
            .unwrap(),
        extranonce_size: extended_channel.get_rollable_extranonce_size(),
    }
    .into_static();

    // ------------------------------------------------------------

    // based on the cached future template, we now create a future job

    // we create the coinbase reward outputs
    // based on the template revenue
    let mut coinbase_reward_outputs: Vec<TxOut> = vec![];
    let first_coinbase_reward_output_value = 546;
    let second_coinbase_reward_output_value = app_context
        .cached_future_template
        .coinbase_tx_value_remaining
        - first_coinbase_reward_output_value;
    coinbase_reward_outputs.push(TxOut {
        value: Amount::from_sat(first_coinbase_reward_output_value),
        script_pubkey: app_context.coinbase_reward_script_pubkeys[0].clone(),
    });
    coinbase_reward_outputs.push(TxOut {
        value: Amount::from_sat(second_coinbase_reward_output_value),
        script_pubkey: app_context.coinbase_reward_script_pubkeys[1].clone(),
    });

    extended_channel
        .on_new_template(
            app_context.cached_future_template.clone(),
            coinbase_reward_outputs,
        )
        .unwrap();

    let future_job_id = extended_channel
        .get_future_template_to_job_id()
        .get(&app_context.cached_future_template.template_id)
        .unwrap();
    let future_job = extended_channel
        .get_future_jobs()
        .get(future_job_id)
        .unwrap();

    // this message will be sent to the client over the wire
    // so that the client has a job that will be immediately activated
    let _future_job_message = future_job.get_job_message();

    // ------------------------------------------------------------

    // this message will be sent to the client over the wire
    // to activate the future job
    // giving the client full context on the chain tip so it can start mining
    let _set_new_prev_hash_mp = SetNewPrevHashMp {
        channel_id,
        job_id: *future_job_id,
        prev_hash: app_context.cached_set_new_prev_hash.prev_hash.clone(),
        min_ntime: app_context.cached_set_new_prev_hash.header_timestamp,
        nbits: app_context.cached_set_new_prev_hash.n_bits,
    }
    .into_static();

    // activate the future job on server side
    // making it ready to receive shares
    extended_channel
        .on_set_new_prev_hash(app_context.cached_set_new_prev_hash.clone())
        .unwrap();
    assert!(extended_channel.get_future_jobs().is_empty());

    // ------------------------------------------------------------

    // the channel is now ready to receive shares for the activated job

    // the Template Provider could also submit new templates (future or non-future)
    // which will be converted into jobs

    // see the other examples for more details

    // ------------------------------------------------------------
}

// extranonce prefix allocation space happens within the app context
// we use separate factories for standard and extended channels
// for example, a pool cannot have collisions on extranonce prefixes
//
// the app context should also have cached at least one future template
// and a SetNewPrevHash message associated with this future template
//
// the app context should also provide the script pubkeys for the coinbase reward outputs
struct AppContext {
    pub _extranonce_prefix_factory_standard: ExtendedExtranonce,
    pub extranonce_prefix_factory_extended: ExtendedExtranonce,
    pub cached_future_template: NewTemplate<'static>,
    pub cached_set_new_prev_hash: SetNewPrevHashTdp<'static>,
    pub coinbase_reward_script_pubkeys: Vec<ScriptBuf>,
}

impl AppContext {
    pub fn new() -> Self {
        // ------------------------------------------------------------
        // extranonce_prefix allocation

        // this is what will be written into the coinbase as an identifier of the pool
        let pool_coinbase_tag = "foo".as_bytes();

        // we assume a production pool has multiple servers in its infrastructure
        // so each server needs to have a unique server_id that will be embedded into the
        // extranonce_prefix this ensures that different servers will always generate
        // different extranonce prefixes
        let server_id = 0u32;

        // the pool_coinbase_tag and server_id are concatenated into the
        // additional_coinbase_script_data these are the initial bytes of the
        // extranonce_prefix
        let additional_coinbase_script_data =
            [pool_coinbase_tag, &server_id.to_le_bytes()].concat();

        // mining_sv2::ExtendedExtranonce constructor takes 4 parameters:
        // - range_0: not used at this level, since this is the most upstream level of a mining
        //   stack
        // - range_1: the range of bytes that are allocated on this level
        // - range_2: the range of bytes that are allocated downstream
        // - additional_coinbase_script_data: custom data to be written into the coinbase, added in
        //   the beginning of range_1

        let range_0 = std::ops::Range { start: 0, end: 0 };

        // aside from additional_coinbase_script_data, we also reserve 8 bytes for the actual search
        // space allocation across the extranonce_prefix that are generated on this server
        let range_1_end = additional_coinbase_script_data.len() + 8;
        let range_1 = std::ops::Range {
            start: 0,
            end: range_1_end,
        };
        let range_2 = std::ops::Range {
            start: range_1_end,
            end: MAX_EXTRANONCE_LEN,
        };

        // since this example focuses on extended channels, we don't really use the
        // _extranonce_prefix_factory_standard however this is here to showcase the need to
        // have a separate factory for standard and extended channels currently the APIs of
        // ExtendedExtranonce are not fully optimal, and using the same factory for both
        // would lead to inefficiency on the search space allocation
        // this could change in the future
        let _extranonce_prefix_factory_standard = ExtendedExtranonce::new(
            range_0.clone(),
            range_1.clone(),
            range_2.clone(),
            Some(additional_coinbase_script_data.to_vec()),
        )
        .unwrap();

        // this is the extranonce prefixfactory we actually use for extended channels
        // note that we always use the next_prefix_extended method during allocation, which takes
        // the min_extranonce_size as a parameter
        let extranonce_prefix_factory_extended = ExtendedExtranonce::new(
            range_0,
            range_1,
            range_2,
            Some(additional_coinbase_script_data.to_vec()),
        )
        .unwrap();

        // ------------------------------------------------------------
        // NewTemplate and SetNewPrevHash caching

        // the application context must always cache (at least) one future template and a
        // SetNewPrevHash message associated with it those are used in the process of
        // bootstrapping a new channel

        // here we emulate a future template that was cached
        let template = NewTemplate {
            template_id: 1,
            future_template: true,
            version: 536870912,
            coinbase_tx_version: 2,
            coinbase_prefix: vec![82, 0].try_into().unwrap(),
            coinbase_tx_input_sequence: 4294967295,
            coinbase_tx_value_remaining: 5000000000,
            coinbase_tx_outputs_count: 1,
            coinbase_tx_outputs: vec![
                0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 226, 246, 28, 63, 113, 209,
                222, 253, 63, 169, 153, 223, 163, 105, 83, 117, 92, 105, 6, 137, 121, 153, 98, 180,
                139, 235, 216, 54, 151, 78, 140, 249,
            ]
            .try_into()
            .unwrap(),
            coinbase_tx_locktime: 0,
            merkle_path: vec![].try_into().unwrap(),
        };

        // here we emulate a SetNewPrevHash message that was cached
        let ntime = 1746839905;
        let set_new_prev_hash = SetNewPrevHashTdp {
            template_id: 1,
            prev_hash: [
                200, 53, 253, 129, 214, 31, 43, 84, 179, 58, 58, 76, 128, 213, 24, 53, 38, 144,
                205, 88, 172, 20, 251, 22, 217, 141, 21, 221, 21, 0, 0, 0,
            ]
            .into(),
            header_timestamp: ntime,
            n_bits: 503543726,
            target: [
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                174, 119, 3, 0, 0,
            ]
            .into(),
        };

        // ------------------------------------------------------------
        // coinbase reward script pubkeys

        // the application context must provide the script pubkeys for the coinbase reward outputs
        // these are used to create the coinbase reward outputs for the jobs (assuming Sv2 JD is not
        // being used)
        let coinbase_reward_script_pubkeys = vec![ScriptBuf::new(), ScriptBuf::new()];

        // ------------------------------------------------------------

        // this is the application context that will be used in the example
        Self {
            _extranonce_prefix_factory_standard,
            extranonce_prefix_factory_extended,
            cached_future_template: template,
            cached_set_new_prev_hash: set_new_prev_hash,
            coinbase_reward_script_pubkeys,
        }
    }
}

// channel id namespace happens within the connection context
// for example, if a server has multiple clients, we only need to guarantee that the channel id
// allocation is unique for this client another client could potentially have the same channel ids
//
// the connection context also has a share_batch_size parameter, which is used to establish after
// how many valid shares a SubmitSharesSuccess message is sent to the client it also has an
// expected_share_per_minute parameter, which is used to calculate the initial channel target based
// on the advertised nominal hashrate
//
// both share_batch_size and expected_share_per_minute could also be defined at AppContext level and
// shared across the ConnnectionContext of each client
struct ConnectionContext {
    pub channel_id_factory: IdFactory, /* IdFactory could also be an AtomicU32, or anything that
                                        * guarantees atomic allocation of u32 */
    pub share_batch_size: usize,
    pub expected_share_per_minute: f32,
}

impl ConnectionContext {
    pub fn new() -> Self {
        Self {
            channel_id_factory: IdFactory::new(),
            share_batch_size: 100, // we acknowledge every 100 valid shares
            expected_share_per_minute: 10.0, // we expect 10 shares per minute
        }
    }
}
