/// This example shows how a Mining Server can bootstrap a group channel. Please also check the
/// example for bootstrapping a standard channel.
///
/// Differently from extended and standard channels, group channels are not necessarily created
/// upon receiving a channel opening request from a client.
///
/// In fact, the Sv2 spec does not define the exact circumstances in which a group channel must
/// be created, or how standard channels should be grouped together. The Sv2 spec only defines
/// that every standard channel must belong to some group channel.
///
/// This means that, on the context of a connection with a specific client, at least one group
/// channel must be created before any standard channel can be created. However, the strategies
/// and criteria for grouping standard channels are left for the specific implementation to
/// decide.
///
/// In SRI apps, we follow the simplest possible criteria:
///
/// for each connection established with a specific client (as long as the
/// `SetupConnection.REQUIRES_STANDARD_JOBS` flag was not set) create one single group channel,
/// and include every standard channel created on that connection under this same group.
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
/// - it has at least one cached future template and a SetNewPrevHash message associated with
///   this future template, which is used to create and activate the future job during the
///   process of bootstrapping a new group channel
/// - it provides the script pubkey(s) for the coinbase reward outputs that will be used on the
///   jobs
///
/// We abstract ConnectionContext as the context of a single connection with a specific client,
/// where multiple channels can be created and managed.
///
/// The most important aspects of ConnectionContext are:
/// - it ensures unique channel id allocation space across all channels
///
/// Note: we make no assumptions about:
/// - how the network stack is implemented
/// - how collections of channels are managed
/// - how concurrency safety is handled
use roles_logic_sv2::{channels::server::group::GroupChannel, utils::Id as IdFactory};
use std::convert::TryInto;
use stratum_common::bitcoin::{transaction::TxOut, Amount, ScriptBuf};
use template_distribution_sv2::{NewTemplate, SetNewPrevHash as SetNewPrevHashTdp};

fn main() {
    // ------------------------------------------------------------
    // basic context of a channel creation

    // app context ensures:
    // - cached future template and SetNewPrevHash message associated with this future template
    // - script pubkey(s) for the coinbase reward outputs
    let app_context = AppContext::new();

    // ------------------------------------------------------------

    // imagine a new connection was just established with a client
    // and the incoming SetupConnection message did not set the REQUIRES_STANDARD_JOBS flag

    // connection context ensures unique channel id allocation space
    let mut connection_context: ConnectionContext = ConnectionContext::new();

    // ------------------------------------------------------------

    // the channel id is allocated from the connection context
    let channel_id = connection_context.channel_id_factory.next();

    // ------------------------------------------------------------

    // create the group channel
    let mut group_channel = GroupChannel::new(channel_id);

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

    group_channel
        .on_new_template(
            app_context.cached_future_template.clone(),
            coinbase_reward_outputs,
        )
        .unwrap();

    // ------------------------------------------------------------

    // activate the future job on server side
    group_channel
        .on_set_new_prev_hash(app_context.cached_set_new_prev_hash.clone())
        .unwrap();
    assert!(group_channel.get_future_jobs().is_empty());

    // ------------------------------------------------------------

    // as new templates are received on the application context, the group channel is
    // updated with new extended jobs

    // because the connection was established without the REQUIRES_STANDARD_JOBS flag,
    // the extended job from the group channel is sent across the wire, as an optimization
    // against sending one standard job for each standard channel that exists on the connection

    // ------------------------------------------------------------
}

// differently from extended and standard channels, group channels do not have a dedicated
// extranonce_prefix, so this AppContext does not have factories for those
//
// the app context should have cached at least one future template
// and a SetNewPrevHash message associated with this future template
//
// the app context should also provide the script pubkeys for the coinbase reward outputs
struct AppContext {
    pub cached_future_template: NewTemplate<'static>,
    pub cached_set_new_prev_hash: SetNewPrevHashTdp<'static>,
    pub coinbase_reward_script_pubkeys: Vec<ScriptBuf>,
}

impl AppContext {
    pub fn new() -> Self {
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
// group channels share the same channel id namespace of standard channel and extended channels
//
// differently from extended and standard channels, group channels do not receive shares, so
// we don't need to keep track of share_batch_size and expected_share_per_minute like in the other
// channel bootstrapping examples
struct ConnectionContext {
    pub channel_id_factory: IdFactory, /* IdFactory could also be an AtomicU32, or anything that
                                        * guarantees atomic allocation of u32 */
}

impl ConnectionContext {
    pub fn new() -> Self {
        Self {
            channel_id_factory: IdFactory::new(),
        }
    }
}
