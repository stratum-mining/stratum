/// This example shows how a Mining Server can bootstrap a standard channel, after
/// receiving an OpenStandardMiningChannel request from a client.
///
/// Standard channels are created upon receiving a channel opening request from a client.
///
/// The Sv2 spec does not define the exact circumstances in which a group channel must be
/// created, or how standard channels should be grouped together. The Sv2 spec only defines
/// that every standard channel must belong to some group channel.
///
/// In SRI apps, we follow the simplest possible criteria:
///
/// - if SetupConnection.REQUIRES_STANDARD_JOBS flag is set, we don't create any group channel,
///   and reply with a OpenStandardMiningChannelSuccess message with `group_channel_id` set to
///   0
/// - if SetupConnection.REQUIRES_STANDARD_JOBS flag is not set, we create one single group
///   channel, and include every standard channel created on that connection under this same
///   group.
///
/// Unless there's good reason to do otherwise, we recommend implementors of Mining Servers to
/// follow this approach.
///
/// This example illustrates this simplified approach.
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
///   process of bootstrapping a new standard channel
/// - it provides the script pubkey(s) for the coinbase reward outputs
///
/// We abstract ConnectionContext as the context of a single connection with a specific client,
/// where multiple channels can be created and managed.
///
/// The most important aspects of ConnectionContext are:
/// - it ensures unique channel id allocation space across all channels
/// - it provides an optional group channel, which is used to group standard channels together
/// - it has a share_batch_size parameter, which is used to establish after how many valid
///   shares a SubmitSharesSuccess message is sent to the client
/// - it has an expected_share_per_minute parameter, which is used to calculate the initial
///   channel target based on the advertised nominal hashrate
///
/// Both share_batch_size and expected_share_per_minute could also be defined at AppContext
/// level and shared across the ConnnectionContext of each client.
///
/// When a new standard channel is created, the following messages need to be sent to the
/// client:
/// - OpenStandardMiningChannelSuccess
/// - NewMiningJob (no min_ntime, making it a future job)
/// - SetNewPrevHash (mining protocol variant) (job_id field argets the future job sent
///   immediately before)
///
/// Even if the connection was created without the REQUIRES_STANDARD_JOBS flag,
/// the first job sent to the client is a standard job. The subsequent jobs are extended jobs
/// addressed to the group channel (although those subsequent jobs are not covered by this
/// example).
///
/// Note: we make no assumptions about:
/// - how the network stack is implemented
/// - how collections of channels are managed
/// - how concurrency safety is handled
use mining_sv2::{
    ExtendedExtranonce, OpenMiningChannelError, OpenStandardMiningChannel,
    OpenStandardMiningChannelSuccess, SetNewPrevHash as SetNewPrevHashMp, MAX_EXTRANONCE_LEN,
};
use roles_logic_sv2::{
    channels::server::{
        error::StandardChannelError, group::GroupChannel, standard::StandardChannel,
    },
    utils::Id as IdFactory,
};
use std::convert::TryInto;
use stratum_common::bitcoin::{Amount, ScriptBuf, TxOut};
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
    let mut connection_context = ConnectionContext::new(app_context.clone(), false);

    // ------------------------------------------------------------

    // imagine an incoming request to open a new standard channel
    let open_standard_mining_channel_request = OpenStandardMiningChannel {
        request_id: 1.into(),
        user_identity: "user1".to_string().try_into().unwrap(),
        nominal_hash_rate: 1000.0,
        max_target: [0xff; 32].into(),
    };

    // ------------------------------------------------------------

    // the extranonce prefix is allocated from the app context
    // note that we always use the next_prefix_standard method during allocation
    let extranonce_prefix = app_context
        .extranonce_prefix_factory_standard
        .next_prefix_standard()
        .unwrap();

    let channel_id = connection_context.channel_id_factory.next();

    let result = StandardChannel::new(
        channel_id,
        String::from_utf8(
            open_standard_mining_channel_request
                .user_identity
                .inner_as_ref()
                .to_vec(),
        )
        .unwrap(),
        extranonce_prefix.clone().to_vec(),
        open_standard_mining_channel_request
            .max_target
            .clone()
            .into(),
        open_standard_mining_channel_request.nominal_hash_rate,
        connection_context.share_batch_size,
        connection_context.expected_share_per_minute,
    );

    // check if channel creation was successful
    let mut standard_channel = match result {
        Ok(standard_channel) => standard_channel,
        Err(e) => {
            let error_code = match e {
                StandardChannelError::InvalidNominalHashrate => {
                    "invalid-nominal-hashrate".to_string()
                }
                StandardChannelError::RequestedMaxTargetOutOfRange => {
                    "max-target-out-of-range".to_string()
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
                request_id: open_standard_mining_channel_request.get_request_id_as_u32(),
                error_code: error_code.try_into().unwrap(),
            };

            return;
        }
    };

    let group_channel_id = if let Some(group_channel) = &connection_context.group_channel {
        group_channel.get_group_channel_id()
    } else {
        0
    };

    // assuming everything went well, we would have to send the following message to the client
    // it informs it essential information, such as:
    // - the assigned channel id
    // - the assigned target (based on the advertised nominal hashrate, and the expected share
    //   submission rate)
    // - the assigned extranonce_prefix (unique across the entire AppContext)
    // - the assigned group channel id
    let _open_standard_mining_channel_success = OpenStandardMiningChannelSuccess {
        request_id: open_standard_mining_channel_request.request_id,
        channel_id,
        target: standard_channel.get_target().clone().into(),
        extranonce_prefix: standard_channel
            .get_extranonce_prefix()
            .clone()
            .try_into()
            .unwrap(),
        group_channel_id,
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

    // set the channel state with the future job
    standard_channel
        .on_new_template(
            app_context.cached_future_template.clone(),
            coinbase_reward_outputs,
        )
        .unwrap();

    let future_job_id = standard_channel
        .get_future_template_to_job_id()
        .get(&app_context.cached_future_template.template_id)
        .unwrap();
    let future_job = standard_channel
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
        job_id: future_job.get_job_id(),
        prev_hash: app_context.cached_set_new_prev_hash.prev_hash.clone(),
        min_ntime: app_context.cached_set_new_prev_hash.header_timestamp,
        nbits: app_context.cached_set_new_prev_hash.n_bits,
    }
    .into_static();

    // activate the future job on the server side
    // making it ready to receive shares
    standard_channel
        .on_set_new_prev_hash(app_context.cached_set_new_prev_hash.clone())
        .unwrap();
    assert!(standard_channel.get_future_jobs().is_empty());

    // ------------------------------------------------------------

    // the channel is now ready to receive shares for the activated job

    // the Template Provider could also send over new templates (future or non-future)
    // which will be converted into extended jobs on the group channel, and from the
    // extended job, converted into the respective standard jobs of each standard channel

    // depending on whether the connection was established with the REQUIRES_STANDARD_JOBS flag,
    // these new jobs will be notified as distinct standard jobs addressed to each standard channel,
    // or as a unique extended job message addressed to the group channel

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
#[derive(Clone)]
struct AppContext {
    pub extranonce_prefix_factory_standard: ExtendedExtranonce,
    pub _extranonce_prefix_factory_extended: ExtendedExtranonce,
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
        // extranonce_prefix
        // this ensures that different servers will always generate
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

        // since this example focuses on standard channels, we don't really use the
        // _extranonce_prefix_factory_extended however this is here to showcase the need to
        // have a separate factory for standard and extended channels currently the APIs of
        // ExtendedExtranonce are not fully optimal, and using the same factory for both
        // would lead to inefficiency on the search space allocation
        // this could change in the future
        let _extranonce_prefix_factory_extended = ExtendedExtranonce::new(
            range_0.clone(),
            range_1.clone(),
            range_2.clone(),
            Some(additional_coinbase_script_data.to_vec()),
        )
        .unwrap();

        // this is the extranonce prefix factory we actually use for standard channels
        // note that we always use the next_prefix_standard method during allocation
        let extranonce_prefix_factory_standard = ExtendedExtranonce::new(
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
            extranonce_prefix_factory_standard,
            _extranonce_prefix_factory_extended,
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
//
// the connection context also has a group channel, which is used to group standard channels
// together
struct ConnectionContext<'a> {
    pub channel_id_factory: IdFactory, /* IdFactory could also be an AtomicU32, or anything that
                                        * guarantees atomic allocation of u32 */
    pub share_batch_size: usize,
    pub expected_share_per_minute: f32,
    pub _app_context: AppContext,
    pub group_channel: Option<GroupChannel<'a>>,
}

impl ConnectionContext<'_> {
    pub fn new(_app_context: AppContext, requires_standard_jobs: bool) -> Self {
        let mut channel_id_factory = IdFactory::new();

        let group_channel = if requires_standard_jobs {
            let group_channel_id = channel_id_factory.next();
            let group_channel = GroupChannel::new(group_channel_id);
            Some(group_channel)
        } else {
            None
        };

        Self {
            channel_id_factory,
            share_batch_size: 100, // we acknowledge every 100 valid shares
            expected_share_per_minute: 10.0, // we expect 10 shares per minute
            group_channel,
            _app_context,
        }
    }
}
