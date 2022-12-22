#include <cstdarg>
#include <cstdint>
#include <cstdlib>
#include <ostream>
#include <new>

static const uint16_t EXTENSION_TYPE_NO_EXTENSION = 0;

static const uintptr_t SV2_FRAME_HEADER_SIZE = 6;

static const uintptr_t SV2_FRAME_HEADER_LEN_OFFSET = 3;

static const uintptr_t SV2_FRAME_HEADER_LEN_END = 3;

static const uintptr_t NOISE_FRAME_HEADER_SIZE = 2;

static const uintptr_t NOISE_FRAME_HEADER_LEN_OFFSET = 0;

static const uintptr_t NOISE_FRAME_HEADER_LEN_END = 2;

static const uintptr_t SNOW_PSKLEN = 32;

static const uintptr_t SNOW_TAGLEN = 16;

static const uint8_t SV2_MINING_PROTOCOL_DISCRIMINANT = 0;

static const uint8_t SV2_JOB_NEG_PROTOCOL_DISCRIMINANT = 1;

static const uint8_t SV2_TEMPLATE_DISTR_PROTOCOL_DISCRIMINANT = 2;

static const uint8_t SV2_JOB_DISTR_PROTOCOL_DISCRIMINANT = 3;

static const uint8_t MESSAGE_TYPE_SETUP_CONNECTION = 0;

static const uint8_t MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS = 1;

static const uint8_t MESSAGE_TYPE_SETUP_CONNECTION_ERROR = 2;

static const uint8_t MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED = 3;

static const uint8_t MESSAGE_TYPE_COINBASE_OUTPUT_DATA_SIZE = 112;

static const uint8_t MESSAGE_TYPE_NEW_TEMPLATE = 113;

static const uint8_t MESSAGE_TYPE_SET_NEW_PREV_HASH = 114;

static const uint8_t MESSAGE_TYPE_REQUEST_TRANSACTION_DATA = 115;

static const uint8_t MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_SUCCESS = 116;

static const uint8_t MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_ERROR = 117;

static const uint8_t MESSAGE_TYPE_SUBMIT_SOLUTION = 118;

static const uint8_t MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN = 80;

static const uint8_t MESSAGE_TYPE_ALLOCATE_MINING_JOB_SUCCESS = 81;

static const uint8_t MESSAGE_TYPE_IDENTIFY_TRANSACTIONS = 83;

static const uint8_t MESSAGE_TYPE_IDENTIFY_TRANSACTIONS_SUCCESS = 84;

static const uint8_t MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTION = 85;

static const uint8_t MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTION_SUCCESS = 86;

static const uint8_t MESSAGE_TYPE_COMMIT_MINING_JOB = 87;

static const uint8_t MESSAGE_TYPE_COMMIT_MINING_JOB_SUCCESS = 88;

static const uint8_t MESSAGE_TYPE_COMMIT_MINING_JOB_ERROR = 89;

static const uint8_t MESSAGE_TYPE_CLOSE_CHANNEL = 24;

static const uint8_t MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB = 31;

static const uint8_t MESSAGE_TYPE_NEW_MINING_JOB = 30;

static const uint8_t MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL = 19;

static const uint8_t MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCES = 20;

static const uint8_t MESSAGE_TYPE_OPEN_MINING_CHANNEL_ERROR = 18;

static const uint8_t MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL = 16;

static const uint8_t MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS = 17;

static const uint8_t MESSAGE_TYPE_RECONNECT = 37;

static const uint8_t MESSAGE_TYPE_SET_CUSTOM_MINING_JOB = 34;

static const uint8_t MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR = 36;

static const uint8_t MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS = 35;

static const uint8_t MESSAGE_TYPE_SET_EXTRANONCE_PREFIX = 25;

static const uint8_t MESSAGE_TYPE_SET_GROUP_CHANNEL = 38;

static const uint8_t MESSAGE_TYPE_MINING_SET_NEW_PREV_HASH = 32;

static const uint8_t MESSAGE_TYPE_SET_TARGET = 33;

static const uint8_t MESSAGE_TYPE_SUBMIT_SHARES_ERROR = 29;

static const uint8_t MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED = 27;

static const uint8_t MESSAGE_TYPE_SUBMIT_SHARES_STANDARD = 26;

static const uint8_t MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS = 28;

static const uint8_t MESSAGE_TYPE_UPDATE_CHANNEL = 22;

static const uint8_t MESSAGE_TYPE_UPDATE_CHANNEL_ERROR = 23;

static const bool CHANNEL_BIT_SETUP_CONNECTION = false;

static const bool CHANNEL_BIT_SETUP_CONNECTION_SUCCESS = false;

static const bool CHANNEL_BIT_SETUP_CONNECTION_ERROR = false;

static const bool CHANNEL_BIT_CHANNEL_ENDPOINT_CHANGED = true;

static const bool CHANNEL_BIT_COINBASE_OUTPUT_DATA_SIZE = false;

static const bool CHANNEL_BIT_NEW_TEMPLATE = false;

static const bool CHANNEL_BIT_SET_NEW_PREV_HASH = false;

static const bool CHANNEL_BIT_REQUEST_TRANSACTION_DATA = false;

static const bool CHANNEL_BIT_REQUEST_TRANSACTION_DATA_SUCCESS = false;

static const bool CHANNEL_BIT_REQUEST_TRANSACTION_DATA_ERROR = false;

static const bool CHANNEL_BIT_SUBMIT_SOLUTION = false;

static const bool CHANNEL_BIT_ALLOCATE_MINING_JOB_TOKEN = false;

static const bool CHANNEL_BIT_ALLOCATE_MINING_JOB_SUCCESS = false;

static const bool CHANNEL_BIT_ALLOCATE_MINING_JOB_ERROR = false;

static const bool CHANNEL_BIT_IDENTIFY_TRANSACTIONS = false;

static const bool CHANNEL_BIT_IDENTIFY_TRANSACTIONS_SUCCESS = false;

static const bool CHANNEL_BIT_PROVIDE_MISSING_TRANSACTION = false;

static const bool CHANNEL_BIT_PROVIDE_MISSING_TRANSACTION_SUCCESS = false;

static const bool CHANNEL_BIT_COMMIT_MINING_JOB = false;

static const bool CHANNEL_BIT_COMMIT_MINING_JOB_SUCCESS = false;

static const bool CHANNEL_BIT_COMMIT_MINING_JOB_ERROR = false;

static const bool CHANNEL_BIT_CLOSE_CHANNEL = true;

static const bool CHANNEL_BIT_NEW_EXTENDED_MINING_JOB = true;

static const bool CHANNEL_BIT_NEW_MINING_JOB = true;

static const bool CHANNEL_BIT_OPEN_EXTENDED_MINING_CHANNEL = false;

static const bool CHANNEL_BIT_OPEN_EXTENDED_MINING_CHANNEL_SUCCES = false;

static const bool CHANNEL_BIT_OPEN_MINING_CHANNEL_ERROR = false;

static const bool CHANNEL_BIT_OPEN_STANDARD_MINING_CHANNEL = false;

static const bool CHANNEL_BIT_OPEN_STANDARD_MINING_CHANNEL_SUCCESS = false;

static const bool CHANNEL_BIT_RECONNECT = false;

static const bool CHANNEL_BIT_SET_CUSTOM_MINING_JOB = false;

static const bool CHANNEL_BIT_SET_CUSTOM_MINING_JOB_ERROR = false;

static const bool CHANNEL_BIT_SET_CUSTOM_MINING_JOB_SUCCESS = false;

static const bool CHANNEL_BIT_SET_EXTRANONCE_PREFIX = true;

static const bool CHANNEL_BIT_SET_GROUP_CHANNEL = false;

static const bool CHANNEL_BIT_MINING_SET_NEW_PREV_HASH = true;

static const bool CHANNEL_BIT_SET_TARGET = true;

static const bool CHANNEL_BIT_SUBMIT_SHARES_ERROR = true;

static const bool CHANNEL_BIT_SUBMIT_SHARES_EXTENDED = true;

static const bool CHANNEL_BIT_SUBMIT_SHARES_STANDARD = true;

static const bool CHANNEL_BIT_SUBMIT_SHARES_SUCCESS = true;

static const bool CHANNEL_BIT_UPDATE_CHANNEL = true;

static const bool CHANNEL_BIT_UPDATE_CHANNEL_ERROR = true;
#include <cstdarg>
#include <cstdint>
#include <cstdlib>
#include <ostream>
#include <new>

struct CVec {
  uint8_t *data;
  uintptr_t len;
  uintptr_t capacity;
};

struct CVec2 {
  CVec *data;
  uintptr_t len;
  uintptr_t capacity;
};

struct U24 {
  uint32_t _0;
};

extern "C" {

/// Given a C allocated buffer return a rust allocated CVec
///
/// # Safety
///
CVec cvec_from_buffer(const uint8_t *data, uintptr_t len);

/// # Safety
///
CVec2 init_cvec2();

/// The caller is reponsible for NOT adding duplicate cvecs to the cvec2 structure,
/// as this can lead to double free errors when the message is dropped.
/// # Safety
///
void cvec2_push(CVec2 *cvec2, CVec cvec);

void _c_export_u24(U24 _a);

void _c_export_cvec(CVec _a);

void _c_export_cvec2(CVec2 _a);

} // extern "C"
#include <cstdarg>
#include <cstdint>
#include <cstdlib>
#include <ostream>
#include <new>

/// MiningProtocol = [`SV2_MINING_PROTOCOL_DISCRIMINANT`],
/// JobNegotiationProtocol = [`SV2_JOB_NEG_PROTOCOL_DISCRIMINANT`],
/// TemplateDistributionProtocol = [`SV2_TEMPLATE_DISTR_PROTOCOL_DISCRIMINANT`],
/// JobDistributionProtocol = [`SV2_JOB_DISTR_PROTOCOL_DISCRIMINANT`],
enum class Protocol : uint8_t {
  MiningProtocol = SV2_MINING_PROTOCOL_DISCRIMINANT,
  JobNegotiationProtocol = SV2_JOB_NEG_PROTOCOL_DISCRIMINANT,
  TemplateDistributionProtocol = SV2_TEMPLATE_DISTR_PROTOCOL_DISCRIMINANT,
  JobDistributionProtocol = SV2_JOB_DISTR_PROTOCOL_DISCRIMINANT,
};

/// ## ChannelEndpointChanged (Server -> Client)
/// When a channel’s upstream or downstream endpoint changes and that channel had previously
/// sent messages with [channel_msg] bitset of unknown extension_type, the intermediate proxy
/// MUST send a [`ChannelEndpointChanged`] message. Upon receipt thereof, any extension state
/// (including version negotiation and the presence of support for a given extension) MUST be
/// reset and version/presence negotiation must begin again.
///
struct ChannelEndpointChanged {
  /// The channel which has changed endpoint.
  uint32_t channel_id;
};

/// ## SetupConnection.Success (Server -> Client)
/// Response to [`SetupConnection`] message if the server accepts the connection. The client is
/// required to verify the set of feature flags that the server supports and act accordingly.
struct SetupConnectionSuccess {
  /// Selected version proposed by the connecting node that the upstream
  /// node supports. This version will be used on the connection for the rest
  /// of its life.
  uint16_t used_version;
  /// Flags indicating optional protocol features the server supports. Each
  /// protocol from [`Protocol`] field has its own values/flags.
  uint32_t flags;
};

struct CSetupConnection {
  Protocol protocol;
  uint16_t min_version;
  uint16_t max_version;
  uint32_t flags;
  CVec endpoint_host;
  uint16_t endpoint_port;
  CVec vendor;
  CVec hardware_version;
  CVec firmware;
  CVec device_id;
};

struct CSetupConnectionError {
  uint32_t flags;
  CVec error_code;
};

extern "C" {

void _c_export_channel_endpoint_changed(ChannelEndpointChanged _a);

void _c_export_setup_conn_succ(SetupConnectionSuccess _a);

void free_setup_connection(CSetupConnection s);

void free_setup_connection_error(CSetupConnectionError s);

} // extern "C"
