#include <cstdarg>
#include <cstdint>
#include <cstdlib>
#include <ostream>
#include <new>

/// Identifier for the extension_type field in the SV2 frame, indicating no
/// extensions.
static const uint16_t EXTENSION_TYPE_NO_EXTENSION = 0;

/// Size of the SV2 frame header in bytes.
static const uintptr_t SV2_FRAME_HEADER_SIZE = 6;

/// Maximum size of an SV2 frame chunk in bytes.
static const uintptr_t SV2_FRAME_CHUNK_SIZE = 65535;

/// Size of the MAC for supported AEAD encryption algorithm (ChaChaPoly).
static const uintptr_t AEAD_MAC_LEN = 16;

/// Size of the encrypted SV2 frame header, including the MAC.
static const uintptr_t ENCRYPTED_SV2_FRAME_HEADER_SIZE = (SV2_FRAME_HEADER_SIZE + AEAD_MAC_LEN);

/// Size of the Noise protocol frame header in bytes.
static const uintptr_t NOISE_FRAME_HEADER_SIZE = 2;

static const uintptr_t NOISE_FRAME_HEADER_LEN_OFFSET = 0;

/// Size in bytes of the encoded elliptic curve point using ElligatorSwift
/// encoding. This encoding produces a 64-byte representation of the
/// X-coordinate of a secp256k1 curve point.
static const uintptr_t ELLSWIFT_ENCODING_SIZE = 64;

static const uintptr_t MAC = 16;

/// Size in bytes of the encrypted ElligatorSwift encoded data, which includes
/// the original ElligatorSwift encoded data and a MAC for integrity
/// verification.
static const uintptr_t ENCRYPTED_ELLSWIFT_ENCODING_SIZE = (ELLSWIFT_ENCODING_SIZE + MAC);

/// Size in bytes of the SIGNATURE_NOISE_MESSAGE, which contains information and
/// a signature for the handshake initiator, formatted according to the Noise
/// Protocol specifications.
static const uintptr_t SIGNATURE_NOISE_MESSAGE_SIZE = 74;

/// Size in bytes of the encrypted signature noise message, which includes the
/// SIGNATURE_NOISE_MESSAGE and a MAC for integrity verification.
static const uintptr_t ENCRYPTED_SIGNATURE_NOISE_MESSAGE_SIZE = (SIGNATURE_NOISE_MESSAGE_SIZE + MAC);

/// Size in bytes of the handshake message expected by the initiator,
/// encompassing:
/// - ElligatorSwift encoded public key
/// - Encrypted ElligatorSwift encoding
/// - Encrypted SIGNATURE_NOISE_MESSAGE
static const uintptr_t INITIATOR_EXPECTED_HANDSHAKE_MESSAGE_SIZE = ((ELLSWIFT_ENCODING_SIZE + ENCRYPTED_ELLSWIFT_ENCODING_SIZE) + ENCRYPTED_SIGNATURE_NOISE_MESSAGE_SIZE);

static const uint8_t SV2_MINING_PROTOCOL_DISCRIMINANT = 0;

static const uint8_t SV2_JOB_DECLARATION_PROTOCOL_DISCRIMINANT = 1;

static const uint8_t SV2_TEMPLATE_DISTR_PROTOCOL_DISCRIMINANT = 2;

static const uint8_t MESSAGE_TYPE_SETUP_CONNECTION = 0;

static const uint8_t MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS = 1;

static const uint8_t MESSAGE_TYPE_SETUP_CONNECTION_ERROR = 2;

static const uint8_t MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED = 3;

static const uint8_t MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL = 16;

static const uint8_t MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS = 17;

static const uint8_t MESSAGE_TYPE_OPEN_MINING_CHANNEL_ERROR = 18;

static const uint8_t MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL = 19;

static const uint8_t MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCES = 20;

static const uint8_t MESSAGE_TYPE_NEW_MINING_JOB = 21;

static const uint8_t MESSAGE_TYPE_UPDATE_CHANNEL = 22;

static const uint8_t MESSAGE_TYPE_UPDATE_CHANNEL_ERROR = 23;

static const uint8_t MESSAGE_TYPE_CLOSE_CHANNEL = 24;

static const uint8_t MESSAGE_TYPE_SET_EXTRANONCE_PREFIX = 25;

static const uint8_t MESSAGE_TYPE_SUBMIT_SHARES_STANDARD = 26;

static const uint8_t MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED = 27;

static const uint8_t MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS = 28;

static const uint8_t MESSAGE_TYPE_SUBMIT_SHARES_ERROR = 29;

static const uint8_t MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB = 31;

static const uint8_t MESSAGE_TYPE_MINING_SET_NEW_PREV_HASH = 32;

static const uint8_t MESSAGE_TYPE_SET_TARGET = 33;

static const uint8_t MESSAGE_TYPE_SET_CUSTOM_MINING_JOB = 34;

static const uint8_t MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS = 35;

static const uint8_t MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR = 36;

static const uint8_t MESSAGE_TYPE_RECONNECT = 37;

static const uint8_t MESSAGE_TYPE_SET_GROUP_CHANNEL = 38;

static const uint8_t MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN = 80;

static const uint8_t MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN_SUCCESS = 81;

static const uint8_t MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS = 85;

static const uint8_t MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS_SUCCESS = 86;

static const uint8_t MESSAGE_TYPE_DECLARE_MINING_JOB = 87;

static const uint8_t MESSAGE_TYPE_DECLARE_MINING_JOB_SUCCESS = 88;

static const uint8_t MESSAGE_TYPE_DECLARE_MINING_JOB_ERROR = 89;

static const uint8_t MESSAGE_TYPE_PUSH_SOLUTION = 96;

static const uint8_t MESSAGE_TYPE_COINBASE_OUTPUT_CONSTRAINTS = 112;

static const uint8_t MESSAGE_TYPE_NEW_TEMPLATE = 113;

static const uint8_t MESSAGE_TYPE_SET_NEW_PREV_HASH = 114;

static const uint8_t MESSAGE_TYPE_REQUEST_TRANSACTION_DATA = 115;

static const uint8_t MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_SUCCESS = 116;

static const uint8_t MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_ERROR = 117;

static const uint8_t MESSAGE_TYPE_SUBMIT_SOLUTION = 118;

static const bool CHANNEL_BIT_SETUP_CONNECTION = false;

static const bool CHANNEL_BIT_SETUP_CONNECTION_SUCCESS = false;

static const bool CHANNEL_BIT_SETUP_CONNECTION_ERROR = false;

static const bool CHANNEL_BIT_CHANNEL_ENDPOINT_CHANGED = true;

static const bool CHANNEL_BIT_COINBASE_OUTPUT_CONSTRAINTS = false;

static const bool CHANNEL_BIT_NEW_TEMPLATE = false;

static const bool CHANNEL_BIT_SET_NEW_PREV_HASH = false;

static const bool CHANNEL_BIT_REQUEST_TRANSACTION_DATA = false;

static const bool CHANNEL_BIT_REQUEST_TRANSACTION_DATA_SUCCESS = false;

static const bool CHANNEL_BIT_REQUEST_TRANSACTION_DATA_ERROR = false;

static const bool CHANNEL_BIT_SUBMIT_SOLUTION = false;

static const bool CHANNEL_BIT_ALLOCATE_MINING_JOB_TOKEN = false;

static const bool CHANNEL_BIT_ALLOCATE_MINING_JOB_TOKEN_SUCCESS = false;

static const bool CHANNEL_BIT_DECLARE_MINING_JOB = false;

static const bool CHANNEL_BIT_DECLARE_MINING_JOB_SUCCESS = false;

static const bool CHANNEL_BIT_DECLARE_MINING_JOB_ERROR = false;

static const bool CHANNEL_BIT_PROVIDE_MISSING_TRANSACTIONS = false;

static const bool CHANNEL_BIT_PROVIDE_MISSING_TRANSACTIONS_SUCCESS = false;

static const bool CHANNEL_BIT_SUBMIT_SOLUTION_JD = true;

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

/// A struct to facilitate transferring a `Vec<u8>` across FFI boundaries.
struct CVec {
  uint8_t *data;
  uintptr_t len;
  uintptr_t capacity;
};

/// A struct to manage a collection of `CVec` objects across FFI boundaries.
struct CVec2 {
  CVec *data;
  uintptr_t len;
  uintptr_t capacity;
};

/// Represents a 24-bit unsigned integer (`U24`), supporting SV2 serialization and deserialization.
/// Only first 3 bytes of a u32 is considered to get the SV2 value, and rest are ignored (in little
/// endian).
struct U24 {
  uint32_t _0;
};

extern "C" {

/// Creates a `CVec` from a buffer that was allocated in C.
///
/// # Safety
/// The caller must ensure that the buffer is valid and that
/// the data length does not exceed the allocated size.
CVec cvec_from_buffer(const uint8_t *data, uintptr_t len);

/// Initializes an empty `CVec2`.
///
/// # Safety
/// The caller is responsible for freeing the `CVec2` when it is no longer needed.
CVec2 init_cvec2();

/// Adds a `CVec` to a `CVec2`.
///
/// # Safety
/// The caller must ensure no duplicate `CVec`s are added, as duplicates may
/// lead to double-free errors when the message is dropped.
void cvec2_push(CVec2 *cvec2, CVec cvec);

/// Exported FFI functions for interoperability with C code for u24
void _c_export_u24(U24 _a);

/// Exported FFI functions for interoperability with C code for CVec
void _c_export_cvec(CVec _a);

/// Exported FFI functions for interoperability with C code for CVec2
void _c_export_cvec2(CVec2 _a);

} // extern "C"
#include <cstdarg>
#include <cstdint>
#include <cstdlib>
#include <ostream>
#include <new>

/// This enum has a list of the different Stratum V2 subprotocols.
enum class Protocol : uint8_t {
  /// Mining protocol.
  MiningProtocol = SV2_MINING_PROTOCOL_DISCRIMINANT,
  /// Job declaration protocol.
  JobDeclarationProtocol = SV2_JOB_DECLARATION_PROTOCOL_DISCRIMINANT,
  /// Template distribution protocol.
  TemplateDistributionProtocol = SV2_TEMPLATE_DISTR_PROTOCOL_DISCRIMINANT,
};

/// Message used by an upstream role for announcing a mining channel endpoint change.
///
/// This message should be sent when a mining channel’s upstream or downstream endpoint changes and
/// that channel had previously exchanged message(s) with `channel_msg` bitset of unknown
/// `extension_type`.
///
/// When a downstream receives such a message, any extension state (including version and extension
/// support) must be reset and renegotiated.
struct ChannelEndpointChanged {
  /// Unique identifier of the channel that has changed its endpoint.
  uint32_t channel_id;
};

/// Message used by an upstream role to accept a connection setup request from a downstream role.
///
/// This message is sent in response to a [`SetupConnection`] message.
struct SetupConnectionSuccess {
  /// Selected version based on the [`SetupConnection::min_version`] and
  /// [`SetupConnection::max_version`] sent by the downstream role.
  ///
  /// This version will be used on the connection for the rest of its life.
  uint16_t used_version;
  /// Flags indicating optional protocol features supported by the upstream.
  ///
  /// The downstream is required to verify this set of flags and act accordingly.
  ///
  /// Each [`SetupConnection::protocol`] field has its own values/flags.
  uint32_t flags;
};

/// C representation of [`SetupConnection`]
struct CSetupConnection {
  /// Protocol to be used for the connection.
  Protocol protocol;
  /// The minimum protocol version supported.
  ///
  /// Currently must be set to 2.
  uint16_t min_version;
  /// The maximum protocol version supported.
  ///
  /// Currently must be set to 2.
  uint16_t max_version;
  /// Flags indicating optional protocol features supported by the downstream.
  ///
  /// Each [`SetupConnection::protocol`] value has it's own flags.
  uint32_t flags;
  /// ASCII representation of the connection hostname or IP address.
  CVec endpoint_host;
  /// Connection port value.
  uint16_t endpoint_port;
  /// Device vendor name.
  CVec vendor;
  /// Device hardware version.
  CVec hardware_version;
  /// Device firmware version.
  CVec firmware;
  /// Device identifier.
  CVec device_id;
};

/// C representation of [`SetupConnectionError`]
struct CSetupConnectionError {
  uint32_t flags;
  CVec error_code;
};

extern "C" {

/// A C-compatible function that exports the [`ChannelEndpointChanged`] struct.
void _c_export_channel_endpoint_changed(ChannelEndpointChanged _a);

/// A C-compatible function that exports the `SetupConnection` struct.
void _c_export_setup_conn_succ(SetupConnectionSuccess _a);

void free_setup_connection(CSetupConnection s);

void free_setup_connection_error(CSetupConnectionError s);

} // extern "C"
#include <cstdarg>
#include <cstdint>
#include <cstdlib>
#include <ostream>
#include <new>

/// Message used by a downstream to indicate the size of the additional bytes they will need in
/// coinbase transaction outputs.
///
/// As the pool is responsible for adding coinbase transaction outputs for payouts and other uses,
/// the Template Provider will need to consider this reserved space when selecting transactions for
/// inclusion in a block(to avoid an invalid, oversized block).  Thus, this message indicates that
/// additional space in the block/coinbase transaction must be reserved for, assuming they will use
/// the entirety of this space.
///
/// The Job Declarator **must** discover the maximum serialized size of the additional outputs which
/// will be added by the pools it intends to use this work. It then **must** communicate the sum of
/// such size to the Template Provider via this message.
///
/// The Template Provider **must not** provide [`NewTemplate`] messages which would represent
/// consensus-invalid blocks once this additional size — along with a maximally-sized (100 byte)
/// coinbase field — is added. Further, the Template Provider **must** consider the maximum
/// additional bytes required in the output count variable-length integer in the coinbase
/// transaction when complying with the size limits.
///
/// [`NewTemplate`]: crate::NewTemplate
struct CoinbaseOutputConstraints {
  /// Additional serialized bytes needed in coinbase transaction outputs.
  uint32_t coinbase_output_max_additional_size;
  /// Additional sigops needed in coinbase transaction outputs.
  uint16_t coinbase_output_max_additional_sigops;
};

/// Message used by a downstream to request data about all transactions in a block template.
///
/// Data includes the full transaction data and any additional data required to block validation.
///
/// Note that the coinbase transaction is excluded from this data.
struct RequestTransactionData {
  /// Identifier of the template that the downstream node is requesting transaction data for.
  ///
  /// This must be identical to previously exchanged [`crate::NewTemplate::template_id`].
  uint64_t template_id;
};

/// C representation of [`NewTemplate`].
struct CNewTemplate {
  uint64_t template_id;
  bool future_template;
  uint32_t version;
  uint32_t coinbase_tx_version;
  CVec coinbase_prefix;
  uint32_t coinbase_tx_input_sequence;
  uint64_t coinbase_tx_value_remaining;
  uint32_t coinbase_tx_outputs_count;
  CVec coinbase_tx_outputs;
  uint32_t coinbase_tx_locktime;
  CVec2 merkle_path;
};

/// C representation of [`RequestTransactionDataSuccess`].
struct CRequestTransactionDataSuccess {
  uint64_t template_id;
  CVec excess_data;
  CVec2 transaction_list;
};

/// C representation of [`RequestTransactionDataError`].
struct CRequestTransactionDataError {
  uint64_t template_id;
  CVec error_code;
};

/// C representation of [`SetNewPrevHash`].
struct CSetNewPrevHash {
  uint64_t template_id;
  CVec prev_hash;
  uint32_t header_timestamp;
  uint32_t n_bits;
  CVec target;
};

/// C representation of [`SubmitSolution`].
struct CSubmitSolution {
  uint64_t template_id;
  uint32_t version;
  uint32_t header_timestamp;
  uint32_t header_nonce;
  CVec coinbase_tx;
};

extern "C" {

/// Exports the [`CoinbaseOutputConstraints`] struct to C.
void _c_export_coinbase_out(CoinbaseOutputConstraints _a);

/// Exports the [`RequestTransactionData`] struct to C.
void _c_export_req_tx_data(RequestTransactionData _a);

/// Drops the [`CNewTemplate`] object.
void free_new_template(CNewTemplate s);

/// Drops the CRequestTransactionDataSuccess object.
void free_request_tx_data_success(CRequestTransactionDataSuccess s);

/// Drops the CRequestTransactionDataError object.
void free_request_tx_data_error(CRequestTransactionDataError s);

/// Drops the CSetNewPrevHash object.
void free_set_new_prev_hash(CSetNewPrevHash s);

/// Drops the CSubmitSolution object.
void free_submit_solution(CSubmitSolution s);

} // extern "C"
#include <cstdarg>
#include <cstdint>
#include <cstdlib>
#include <ostream>
#include <new>

/// C-compatible enumeration of possible errors in the `codec_sv2` module.
///
/// This enum mirrors the [`Error`] enum but is designed to be used in C code through FFI. It
/// represents the same set of errors as [`Error`], making them accessible to C programs.
struct CError {
  enum class Tag {
    /// AEAD (`snow`) error in the Noise protocol.
    AeadError,
    /// Binary Sv2 data format error.
    BinarySv2Error,
    /// Framing Sv2 error.
    FramingError,
    /// Framing Sv2 error.
    FramingSv2Error,
    /// Invalid step for initiator in the Noise protocol.
    InvalidStepForInitiator,
    /// Invalid step for responder in the Noise protocol.
    InvalidStepForResponder,
    /// Missing bytes in the Noise protocol.
    MissingBytes,
    /// Sv2 Noise protocol error.
    NoiseSv2Error,
    /// Noise protocol is not in the expected handshake state.
    NotInHandShakeState,
    /// Unexpected state in the Noise protocol.
    UnexpectedNoiseState,
  };

  struct MissingBytes_Body {
    uintptr_t _0;
  };

  Tag tag;
  union {
    MissingBytes_Body missing_bytes;
  };
};

extern "C" {

/// Force `cbindgen` to create a header for [`CError`].
///
/// It ensures that [`CError`] is included in the generated C header file. This function is not
/// meant to be called and will panic if called. Its only purpose is to make [`CError`] visible to
/// `cbindgen`.
CError export_cerror();

} // extern "C"
#include <cstdarg>
#include <cstdint>
#include <cstdlib>
#include <ostream>
#include <new>

struct DecoderWrapper;

struct EncoderWrapper;

struct CSv2Message {
  enum class Tag {
    CoinbaseOutputConstraints,
    NewTemplate,
    RequestTransactionData,
    RequestTransactionDataError,
    RequestTransactionDataSuccess,
    SetNewPrevHash,
    SubmitSolution,
    ChannelEndpointChanged,
    SetupConnection,
    SetupConnectionError,
    SetupConnectionSuccess,
  };

  struct CoinbaseOutputConstraints_Body {
    CoinbaseOutputConstraints _0;
  };

  struct NewTemplate_Body {
    CNewTemplate _0;
  };

  struct RequestTransactionData_Body {
    RequestTransactionData _0;
  };

  struct RequestTransactionDataError_Body {
    CRequestTransactionDataError _0;
  };

  struct RequestTransactionDataSuccess_Body {
    CRequestTransactionDataSuccess _0;
  };

  struct SetNewPrevHash_Body {
    CSetNewPrevHash _0;
  };

  struct SubmitSolution_Body {
    CSubmitSolution _0;
  };

  struct ChannelEndpointChanged_Body {
    ChannelEndpointChanged _0;
  };

  struct SetupConnection_Body {
    CSetupConnection _0;
  };

  struct SetupConnectionError_Body {
    CSetupConnectionError _0;
  };

  struct SetupConnectionSuccess_Body {
    SetupConnectionSuccess _0;
  };

  Tag tag;
  union {
    CoinbaseOutputConstraints_Body coinbase_output_constraints;
    NewTemplate_Body new_template;
    RequestTransactionData_Body request_transaction_data;
    RequestTransactionDataError_Body request_transaction_data_error;
    RequestTransactionDataSuccess_Body request_transaction_data_success;
    SetNewPrevHash_Body set_new_prev_hash;
    SubmitSolution_Body submit_solution;
    ChannelEndpointChanged_Body channel_endpoint_changed;
    SetupConnection_Body setup_connection;
    SetupConnectionError_Body setup_connection_error;
    SetupConnectionSuccess_Body setup_connection_success;
  };
};

struct Sv2Error {
  enum class Tag {
    BinaryError,
    CodecError,
    EncoderBusy,
    InvalidSv2Frame,
    MissingBytes,
    PayloadTooBig,
    Unknown,
  };

  struct BinaryError_Body {
    CError _0;
  };

  struct CodecError_Body {
    CError _0;
  };

  struct PayloadTooBig_Body {
    CVec _0;
  };

  Tag tag;
  union {
    BinaryError_Body binary_error;
    CodecError_Body codec_error;
    PayloadTooBig_Body payload_too_big;
  };
};

template<typename T, typename E>
struct CResult {
  enum class Tag {
    Ok,
    Err,
  };

  struct Ok_Body {
    T _0;
  };

  struct Err_Body {
    E _0;
  };

  Tag tag;
  union {
    Ok_Body ok;
    Err_Body err;
  };
};

extern "C" {

void drop_sv2_message(CSv2Message s);

/// This function does nothing unless there is some heap allocated data owned by the C side that
/// needs to be dropped (specifically a `CVec`). In this case, `free_vec` is used in order to drop
/// that memory.
void drop_sv2_error(Sv2Error s);

bool is_ok(const CResult<CSv2Message, Sv2Error> *cresult);

EncoderWrapper *new_encoder();

void flush_encoder(EncoderWrapper *encoder);

void free_decoder(DecoderWrapper *decoder);

/// # Safety
CResult<CVec, Sv2Error> encode(CSv2Message *message, EncoderWrapper *encoder);

DecoderWrapper *new_decoder();

CVec get_writable(DecoderWrapper *decoder);

CResult<CSv2Message, Sv2Error> next_frame(DecoderWrapper *decoder);

} // extern "C"
