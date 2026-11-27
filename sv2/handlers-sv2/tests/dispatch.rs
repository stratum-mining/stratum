//! Every handler trait hands each message of its protocol to the handler of that message, with
//! the TLV fields of the frame, or rejects it as unexpected when its side never receives it.

use framing_sv2::header::Header;
use handlers_sv2::*;
use parsers_sv2::{AnyMessage, ParserError, Tlv};
use std::{
    future::Future,
    pin::pin,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
};

const CHANNEL_MSG: u16 = 0x8000;
const TLV_EXTENSION: u16 = 0x0002;

#[derive(Debug, PartialEq)]
enum Error {
    Unexpected(u16, u8),
    Parse(String),
}

impl HandlerErrorType for Error {
    fn unexpected_message(extension_type: u16, message_type: u8) -> Self {
        Error::Unexpected(extension_type, message_type)
    }
    fn parse_error(error: ParserError) -> Self {
        Error::Parse(format!("{error:?}"))
    }
}

/// The message type a handler was called for, and the TLV fields it was given.
type Handled = (u8, Option<Vec<Tlv>>);

struct Recorder {
    negotiated_extensions: Vec<u16>,
    handled: Option<Handled>,
}

impl Recorder {
    fn record(&mut self, message_type: u8, tlv_fields: Option<&[Tlv]>) -> Result<(), Error> {
        self.handled = Some((message_type, tlv_fields.map(<[Tlv]>::to_vec)));
        Ok(())
    }
}

// The handlers never wait, so their futures are ready on the first poll.
fn block_on<F: Future>(future: F) -> F::Output {
    struct NoopWaker;
    impl Wake for NoopWaker {
        fn wake(self: Arc<Self>) {}
    }
    let waker = Waker::from(Arc::new(NoopWaker));
    match pin!(future).poll(&mut Context::from_waker(&waker)) {
        Poll::Ready(output) => output,
        Poll::Pending => unreachable!(),
    }
}

fn header(extension_type: u16, message_type: u8, payload_length: usize) -> Header {
    let ext = extension_type.to_le_bytes();
    let len = (payload_length as u32).to_le_bytes();
    Header::from_bytes(&[ext[0], ext[1], message_type, len[0], len[1], len[2]]).unwrap()
}

/// Sends every message of a protocol through the sync and async flavor of a handler trait, once
/// without TLV fields and once with a field of a negotiated extension.
///
/// `messages` lists each message as its type and channel bit, and `handled` the message types the
/// trait has a handler for.
fn check_dispatch(
    extension_type: u16,
    messages: &[(u8, bool)],
    handled: &[u8],
    dispatch: impl Fn(&[u16], Header, &[u8]) -> [Result<Handled, Error>; 2],
) {
    let tlv = Tlv::new(TLV_EXTENSION, 1, vec![7]).unwrap();
    for &(message_type, channel_bit) in messages {
        let frame_extension_type = extension_type | if channel_bit { CHANNEL_MSG } else { 0 };
        // Every message decodes from zeroes, so the shortest run of them that decodes is exactly
        // one message.
        let message = (0..512)
            .map(|length| vec![0; length])
            .find(|payload| {
                let header = header(frame_extension_type, message_type, payload.len());
                AnyMessage::try_from((header, &mut payload.clone()[..])).is_ok()
            })
            .unwrap();
        let message_with_tlv = [message.clone(), tlv.encode().unwrap()].concat();

        for (negotiated_extensions, payload, tlv_fields) in [
            (&[][..], &message, None),
            (
                &[TLV_EXTENSION][..],
                &message_with_tlv,
                Some(vec![tlv.clone()]),
            ),
        ] {
            let expected = if handled.contains(&message_type) {
                Ok((message_type, tlv_fields))
            } else {
                Err(Error::Unexpected(extension_type, message_type))
            };
            let header = header(frame_extension_type, message_type, payload.len());
            for outcome in dispatch(negotiated_extensions, header, payload) {
                assert_eq!(outcome, expected);
            }

            // A header that disagrees with the message never reaches a handler.
            let mismatches = [
                // wrong channel bit
                crate::header(
                    frame_extension_type ^ CHANNEL_MSG,
                    message_type,
                    payload.len(),
                ),
                // wrong payload length
                crate::header(frame_extension_type, message_type, payload.len() + 1),
                // another extension namespace
                crate::header(frame_extension_type ^ 0x0004, message_type, payload.len()),
            ];
            for header in mismatches {
                for outcome in dispatch(negotiated_extensions, header, payload) {
                    assert!(matches!(outcome, Err(Error::Parse(_))), "{outcome:?}");
                }
            }
        }
    }
}

/// Implements the sync and async flavor of a handler trait for [`Recorder`], and defines a test
/// that checks how both dispatch the messages of the enclosing module's protocol.
macro_rules! dispatch_test {
    (
        $test:ident, $sync:ident, $async:ident, $negotiated_extensions:ident, $handle_frame:ident,
        $($handler:ident($message:ty) => $message_type:expr,)*
    ) => {
        impl $sync for Recorder {
            type Error = Error;
            fn $negotiated_extensions(&self, _: Option<usize>) -> Result<Vec<u16>, Error> {
                Ok(self.negotiated_extensions.clone())
            }
            $(fn $handler(
                &mut self,
                _: Option<usize>,
                _: $message,
                tlv_fields: Option<&[Tlv]>,
            ) -> Result<(), Error> {
                self.record($message_type, tlv_fields)
            })*
        }

        impl $async for Recorder {
            type Error = Error;
            fn $negotiated_extensions(&self, _: Option<usize>) -> Result<Vec<u16>, Error> {
                Ok(self.negotiated_extensions.clone())
            }
            $(async fn $handler(
                &mut self,
                _: Option<usize>,
                _: $message,
                tlv_fields: Option<&[Tlv]>,
            ) -> Result<(), Error> {
                self.record($message_type, tlv_fields)
            })*
        }

        #[test]
        fn $test() {
            check_dispatch(
                EXTENSION_TYPE,
                MESSAGES,
                &[$($message_type),*],
                |negotiated_extensions, header, payload| {
                    let recorder = || Recorder {
                        negotiated_extensions: negotiated_extensions.to_vec(),
                        handled: None,
                    };
                    let (mut sync, mut asynchronous) = (recorder(), recorder());
                    [
                        <Recorder as $sync>::$handle_frame(
                            &mut sync,
                            None,
                            header,
                            &mut payload.to_vec(),
                        )
                        .map(|()| sync.handled.unwrap()),
                        block_on(<Recorder as $async>::$handle_frame(
                            &mut asynchronous,
                            None,
                            header,
                            &mut payload.to_vec(),
                        ))
                        .map(|()| asynchronous.handled.unwrap()),
                    ]
                },
            );
        }
    };
}

mod common {
    use super::*;
    use common_messages_sv2::*;

    const EXTENSION_TYPE: u16 = 0;
    const MESSAGES: &[(u8, bool)] = &[
        (MESSAGE_TYPE_SETUP_CONNECTION, CHANNEL_BIT_SETUP_CONNECTION),
        (
            MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
            CHANNEL_BIT_SETUP_CONNECTION_SUCCESS,
        ),
        (
            MESSAGE_TYPE_SETUP_CONNECTION_ERROR,
            CHANNEL_BIT_SETUP_CONNECTION_ERROR,
        ),
        (
            MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED,
            CHANNEL_BIT_CHANNEL_ENDPOINT_CHANGED,
        ),
        (MESSAGE_TYPE_RECONNECT, CHANNEL_BIT_RECONNECT),
    ];

    dispatch_test!(
        from_server,
        HandleCommonMessagesFromServerSync,
        HandleCommonMessagesFromServerAsync,
        get_negotiated_extensions_with_server,
        handle_common_message_frame_from_server,
        handle_setup_connection_success(SetupConnectionSuccess) => MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
        handle_setup_connection_error(SetupConnectionError<'_>) => MESSAGE_TYPE_SETUP_CONNECTION_ERROR,
        handle_channel_endpoint_changed(ChannelEndpointChanged) => MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED,
        handle_reconnect(Reconnect<'_>) => MESSAGE_TYPE_RECONNECT,
    );
    dispatch_test!(
        from_server_owned,
        HandleCommonMessagesFromServerOwnedSync,
        HandleCommonMessagesFromServerOwnedAsync,
        get_negotiated_extensions_with_server,
        handle_common_message_frame_from_server,
        handle_setup_connection_success(SetupConnectionSuccessOwned) => MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
        handle_setup_connection_error(SetupConnectionErrorOwned) => MESSAGE_TYPE_SETUP_CONNECTION_ERROR,
        handle_channel_endpoint_changed(ChannelEndpointChangedOwned) => MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED,
        handle_reconnect(ReconnectOwned) => MESSAGE_TYPE_RECONNECT,
    );
    dispatch_test!(
        from_client,
        HandleCommonMessagesFromClientSync,
        HandleCommonMessagesFromClientAsync,
        get_negotiated_extensions_with_client,
        handle_common_message_frame_from_client,
        handle_setup_connection(SetupConnection<'_>) => MESSAGE_TYPE_SETUP_CONNECTION,
    );
    dispatch_test!(
        from_client_owned,
        HandleCommonMessagesFromClientOwnedSync,
        HandleCommonMessagesFromClientOwnedAsync,
        get_negotiated_extensions_with_client,
        handle_common_message_frame_from_client,
        handle_setup_connection(SetupConnectionOwned) => MESSAGE_TYPE_SETUP_CONNECTION,
    );
}

mod mining {
    use super::*;
    use mining_sv2::*;

    const EXTENSION_TYPE: u16 = 0;
    const MESSAGES: &[(u8, bool)] = &[
        (
            MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL,
            CHANNEL_BIT_OPEN_STANDARD_MINING_CHANNEL,
        ),
        (
            MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS,
            CHANNEL_BIT_OPEN_STANDARD_MINING_CHANNEL_SUCCESS,
        ),
        (
            MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL,
            CHANNEL_BIT_OPEN_EXTENDED_MINING_CHANNEL,
        ),
        (
            MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCESS,
            CHANNEL_BIT_OPEN_EXTENDED_MINING_CHANNEL_SUCCESS,
        ),
        (
            MESSAGE_TYPE_OPEN_MINING_CHANNEL_ERROR,
            CHANNEL_BIT_OPEN_MINING_CHANNEL_ERROR,
        ),
        (MESSAGE_TYPE_UPDATE_CHANNEL, CHANNEL_BIT_UPDATE_CHANNEL),
        (
            MESSAGE_TYPE_UPDATE_CHANNEL_ERROR,
            CHANNEL_BIT_UPDATE_CHANNEL_ERROR,
        ),
        (MESSAGE_TYPE_CLOSE_CHANNEL, CHANNEL_BIT_CLOSE_CHANNEL),
        (
            MESSAGE_TYPE_SET_EXTRANONCE_PREFIX,
            CHANNEL_BIT_SET_EXTRANONCE_PREFIX,
        ),
        (
            MESSAGE_TYPE_SUBMIT_SHARES_STANDARD,
            CHANNEL_BIT_SUBMIT_SHARES_STANDARD,
        ),
        (
            MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
            CHANNEL_BIT_SUBMIT_SHARES_EXTENDED,
        ),
        (
            MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS,
            CHANNEL_BIT_SUBMIT_SHARES_SUCCESS,
        ),
        (
            MESSAGE_TYPE_SUBMIT_SHARES_ERROR,
            CHANNEL_BIT_SUBMIT_SHARES_ERROR,
        ),
        (MESSAGE_TYPE_NEW_MINING_JOB, CHANNEL_BIT_NEW_MINING_JOB),
        (
            MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB,
            CHANNEL_BIT_NEW_EXTENDED_MINING_JOB,
        ),
        (
            MESSAGE_TYPE_MINING_SET_NEW_PREV_HASH,
            CHANNEL_BIT_MINING_SET_NEW_PREV_HASH,
        ),
        (
            MESSAGE_TYPE_SET_CUSTOM_MINING_JOB,
            CHANNEL_BIT_SET_CUSTOM_MINING_JOB,
        ),
        (
            MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS,
            CHANNEL_BIT_SET_CUSTOM_MINING_JOB_SUCCESS,
        ),
        (
            MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR,
            CHANNEL_BIT_SET_CUSTOM_MINING_JOB_ERROR,
        ),
        (MESSAGE_TYPE_SET_TARGET, CHANNEL_BIT_SET_TARGET),
        (
            MESSAGE_TYPE_SET_GROUP_CHANNEL,
            CHANNEL_BIT_SET_GROUP_CHANNEL,
        ),
    ];

    dispatch_test!(
        from_server,
        HandleMiningMessagesFromServerSync,
        HandleMiningMessagesFromServerAsync,
        get_negotiated_extensions_with_server,
        handle_mining_message_frame_from_server,
        handle_open_standard_mining_channel_success(OpenStandardMiningChannelSuccess<'_>) => MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS,
        handle_open_extended_mining_channel_success(OpenExtendedMiningChannelSuccess<'_>) => MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCESS,
        handle_open_mining_channel_error(OpenMiningChannelError<'_>) => MESSAGE_TYPE_OPEN_MINING_CHANNEL_ERROR,
        handle_update_channel_error(UpdateChannelError<'_>) => MESSAGE_TYPE_UPDATE_CHANNEL_ERROR,
        handle_close_channel(CloseChannel<'_>) => MESSAGE_TYPE_CLOSE_CHANNEL,
        handle_set_extranonce_prefix(SetExtranoncePrefix<'_>) => MESSAGE_TYPE_SET_EXTRANONCE_PREFIX,
        handle_submit_shares_success(SubmitSharesSuccess) => MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS,
        handle_submit_shares_error(SubmitSharesError<'_>) => MESSAGE_TYPE_SUBMIT_SHARES_ERROR,
        handle_new_mining_job(NewMiningJob<'_>) => MESSAGE_TYPE_NEW_MINING_JOB,
        handle_new_extended_mining_job(NewExtendedMiningJob<'_>) => MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB,
        handle_set_new_prev_hash(SetNewPrevHash<'_>) => MESSAGE_TYPE_MINING_SET_NEW_PREV_HASH,
        handle_set_custom_mining_job_success(SetCustomMiningJobSuccess) => MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS,
        handle_set_custom_mining_job_error(SetCustomMiningJobError<'_>) => MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR,
        handle_set_target(SetTarget<'_>) => MESSAGE_TYPE_SET_TARGET,
        handle_set_group_channel(SetGroupChannel<'_>) => MESSAGE_TYPE_SET_GROUP_CHANNEL,
    );
    dispatch_test!(
        from_server_owned,
        HandleMiningMessagesFromServerOwnedSync,
        HandleMiningMessagesFromServerOwnedAsync,
        get_negotiated_extensions_with_server,
        handle_mining_message_frame_from_server,
        handle_open_standard_mining_channel_success(OpenStandardMiningChannelSuccessOwned) => MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS,
        handle_open_extended_mining_channel_success(OpenExtendedMiningChannelSuccessOwned) => MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCESS,
        handle_open_mining_channel_error(OpenMiningChannelErrorOwned) => MESSAGE_TYPE_OPEN_MINING_CHANNEL_ERROR,
        handle_update_channel_error(UpdateChannelErrorOwned) => MESSAGE_TYPE_UPDATE_CHANNEL_ERROR,
        handle_close_channel(CloseChannelOwned) => MESSAGE_TYPE_CLOSE_CHANNEL,
        handle_set_extranonce_prefix(SetExtranoncePrefixOwned) => MESSAGE_TYPE_SET_EXTRANONCE_PREFIX,
        handle_submit_shares_success(SubmitSharesSuccessOwned) => MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS,
        handle_submit_shares_error(SubmitSharesErrorOwned) => MESSAGE_TYPE_SUBMIT_SHARES_ERROR,
        handle_new_mining_job(NewMiningJobOwned) => MESSAGE_TYPE_NEW_MINING_JOB,
        handle_new_extended_mining_job(NewExtendedMiningJobOwned) => MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB,
        handle_set_new_prev_hash(SetNewPrevHashOwned) => MESSAGE_TYPE_MINING_SET_NEW_PREV_HASH,
        handle_set_custom_mining_job_success(SetCustomMiningJobSuccessOwned) => MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS,
        handle_set_custom_mining_job_error(SetCustomMiningJobErrorOwned) => MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR,
        handle_set_target(SetTargetOwned) => MESSAGE_TYPE_SET_TARGET,
        handle_set_group_channel(SetGroupChannelOwned) => MESSAGE_TYPE_SET_GROUP_CHANNEL,
    );
    dispatch_test!(
        from_client,
        HandleMiningMessagesFromClientSync,
        HandleMiningMessagesFromClientAsync,
        get_negotiated_extensions_with_client,
        handle_mining_message_frame_from_client,
        handle_close_channel(CloseChannel<'_>) => MESSAGE_TYPE_CLOSE_CHANNEL,
        handle_open_standard_mining_channel(OpenStandardMiningChannel<'_>) => MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL,
        handle_open_extended_mining_channel(OpenExtendedMiningChannel<'_>) => MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL,
        handle_update_channel(UpdateChannel<'_>) => MESSAGE_TYPE_UPDATE_CHANNEL,
        handle_submit_shares_standard(SubmitSharesStandard) => MESSAGE_TYPE_SUBMIT_SHARES_STANDARD,
        handle_submit_shares_extended(SubmitSharesExtended<'_>) => MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
        handle_set_custom_mining_job(SetCustomMiningJob<'_>) => MESSAGE_TYPE_SET_CUSTOM_MINING_JOB,
    );
    dispatch_test!(
        from_client_owned,
        HandleMiningMessagesFromClientOwnedSync,
        HandleMiningMessagesFromClientOwnedAsync,
        get_negotiated_extensions_with_client,
        handle_mining_message_frame_from_client,
        handle_close_channel(CloseChannelOwned) => MESSAGE_TYPE_CLOSE_CHANNEL,
        handle_open_standard_mining_channel(OpenStandardMiningChannelOwned) => MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL,
        handle_open_extended_mining_channel(OpenExtendedMiningChannelOwned) => MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL,
        handle_update_channel(UpdateChannelOwned) => MESSAGE_TYPE_UPDATE_CHANNEL,
        handle_submit_shares_standard(SubmitSharesStandardOwned) => MESSAGE_TYPE_SUBMIT_SHARES_STANDARD,
        handle_submit_shares_extended(SubmitSharesExtendedOwned) => MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
        handle_set_custom_mining_job(SetCustomMiningJobOwned) => MESSAGE_TYPE_SET_CUSTOM_MINING_JOB,
    );
}

mod job_declaration {
    use super::*;
    use job_declaration_sv2::*;

    const EXTENSION_TYPE: u16 = 0;
    const MESSAGES: &[(u8, bool)] = &[
        (
            MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN,
            CHANNEL_BIT_ALLOCATE_MINING_JOB_TOKEN,
        ),
        (
            MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN_SUCCESS,
            CHANNEL_BIT_ALLOCATE_MINING_JOB_TOKEN_SUCCESS,
        ),
        (
            MESSAGE_TYPE_DECLARE_MINING_JOB,
            CHANNEL_BIT_DECLARE_MINING_JOB,
        ),
        (
            MESSAGE_TYPE_DECLARE_MINING_JOB_SUCCESS,
            CHANNEL_BIT_DECLARE_MINING_JOB_SUCCESS,
        ),
        (
            MESSAGE_TYPE_DECLARE_MINING_JOB_ERROR,
            CHANNEL_BIT_DECLARE_MINING_JOB_ERROR,
        ),
        (
            MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS,
            CHANNEL_BIT_PROVIDE_MISSING_TRANSACTIONS,
        ),
        (
            MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS_SUCCESS,
            CHANNEL_BIT_PROVIDE_MISSING_TRANSACTIONS_SUCCESS,
        ),
        (MESSAGE_TYPE_PUSH_SOLUTION, CHANNEL_BIT_PUSH_SOLUTION),
    ];

    dispatch_test!(
        from_server,
        HandleJobDeclarationMessagesFromServerSync,
        HandleJobDeclarationMessagesFromServerAsync,
        get_negotiated_extensions_with_server,
        handle_job_declaration_message_frame_from_server,
        handle_allocate_mining_job_token_success(AllocateMiningJobTokenSuccess<'_>) => MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN_SUCCESS,
        handle_declare_mining_job_success(DeclareMiningJobSuccess<'_>) => MESSAGE_TYPE_DECLARE_MINING_JOB_SUCCESS,
        handle_declare_mining_job_error(DeclareMiningJobError<'_>) => MESSAGE_TYPE_DECLARE_MINING_JOB_ERROR,
        handle_provide_missing_transactions(ProvideMissingTransactions<'_>) => MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS,
    );
    dispatch_test!(
        from_server_owned,
        HandleJobDeclarationMessagesFromServerOwnedSync,
        HandleJobDeclarationMessagesFromServerOwnedAsync,
        get_negotiated_extensions_with_server,
        handle_job_declaration_message_frame_from_server,
        handle_allocate_mining_job_token_success(AllocateMiningJobTokenSuccessOwned) => MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN_SUCCESS,
        handle_declare_mining_job_success(DeclareMiningJobSuccessOwned) => MESSAGE_TYPE_DECLARE_MINING_JOB_SUCCESS,
        handle_declare_mining_job_error(DeclareMiningJobErrorOwned) => MESSAGE_TYPE_DECLARE_MINING_JOB_ERROR,
        handle_provide_missing_transactions(ProvideMissingTransactionsOwned) => MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS,
    );
    dispatch_test!(
        from_client,
        HandleJobDeclarationMessagesFromClientSync,
        HandleJobDeclarationMessagesFromClientAsync,
        get_negotiated_extensions_with_client,
        handle_job_declaration_message_frame_from_client,
        handle_allocate_mining_job_token(AllocateMiningJobToken<'_>) => MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN,
        handle_declare_mining_job(DeclareMiningJob<'_>) => MESSAGE_TYPE_DECLARE_MINING_JOB,
        handle_provide_missing_transactions_success(ProvideMissingTransactionsSuccess<'_>) => MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS_SUCCESS,
        handle_push_solution(PushSolution<'_>) => MESSAGE_TYPE_PUSH_SOLUTION,
    );
    dispatch_test!(
        from_client_owned,
        HandleJobDeclarationMessagesFromClientOwnedSync,
        HandleJobDeclarationMessagesFromClientOwnedAsync,
        get_negotiated_extensions_with_client,
        handle_job_declaration_message_frame_from_client,
        handle_allocate_mining_job_token(AllocateMiningJobTokenOwned) => MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN,
        handle_declare_mining_job(DeclareMiningJobOwned) => MESSAGE_TYPE_DECLARE_MINING_JOB,
        handle_provide_missing_transactions_success(ProvideMissingTransactionsSuccessOwned) => MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS_SUCCESS,
        handle_push_solution(PushSolutionOwned) => MESSAGE_TYPE_PUSH_SOLUTION,
    );
}

mod template_distribution {
    use super::*;
    use template_distribution_sv2::*;

    const EXTENSION_TYPE: u16 = 0;
    const MESSAGES: &[(u8, bool)] = &[
        (
            MESSAGE_TYPE_COINBASE_OUTPUT_CONSTRAINTS,
            CHANNEL_BIT_COINBASE_OUTPUT_CONSTRAINTS,
        ),
        (MESSAGE_TYPE_NEW_TEMPLATE, CHANNEL_BIT_NEW_TEMPLATE),
        (
            MESSAGE_TYPE_SET_NEW_PREV_HASH,
            CHANNEL_BIT_SET_NEW_PREV_HASH,
        ),
        (
            MESSAGE_TYPE_REQUEST_TRANSACTION_DATA,
            CHANNEL_BIT_REQUEST_TRANSACTION_DATA,
        ),
        (
            MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_SUCCESS,
            CHANNEL_BIT_REQUEST_TRANSACTION_DATA_SUCCESS,
        ),
        (
            MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_ERROR,
            CHANNEL_BIT_REQUEST_TRANSACTION_DATA_ERROR,
        ),
        (MESSAGE_TYPE_SUBMIT_SOLUTION, CHANNEL_BIT_SUBMIT_SOLUTION),
    ];

    dispatch_test!(
        from_server,
        HandleTemplateDistributionMessagesFromServerSync,
        HandleTemplateDistributionMessagesFromServerAsync,
        get_negotiated_extensions_with_server,
        handle_template_distribution_message_frame_from_server,
        handle_new_template(NewTemplate<'_>) => MESSAGE_TYPE_NEW_TEMPLATE,
        handle_set_new_prev_hash(SetNewPrevHash<'_>) => MESSAGE_TYPE_SET_NEW_PREV_HASH,
        handle_request_tx_data_success(RequestTransactionDataSuccess<'_>) => MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_SUCCESS,
        handle_request_tx_data_error(RequestTransactionDataError<'_>) => MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_ERROR,
    );
    dispatch_test!(
        from_server_owned,
        HandleTemplateDistributionMessagesFromServerOwnedSync,
        HandleTemplateDistributionMessagesFromServerOwnedAsync,
        get_negotiated_extensions_with_server,
        handle_template_distribution_message_frame_from_server,
        handle_new_template(NewTemplateOwned) => MESSAGE_TYPE_NEW_TEMPLATE,
        handle_set_new_prev_hash(SetNewPrevHashOwned) => MESSAGE_TYPE_SET_NEW_PREV_HASH,
        handle_request_tx_data_success(RequestTransactionDataSuccessOwned) => MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_SUCCESS,
        handle_request_tx_data_error(RequestTransactionDataErrorOwned) => MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_ERROR,
    );
    dispatch_test!(
        from_client,
        HandleTemplateDistributionMessagesFromClientSync,
        HandleTemplateDistributionMessagesFromClientAsync,
        get_negotiated_extensions_with_client,
        handle_template_distribution_message_frame_from_client,
        handle_coinbase_output_constraints(CoinbaseOutputConstraints) => MESSAGE_TYPE_COINBASE_OUTPUT_CONSTRAINTS,
        handle_request_tx_data(RequestTransactionData) => MESSAGE_TYPE_REQUEST_TRANSACTION_DATA,
        handle_submit_solution(SubmitSolution<'_>) => MESSAGE_TYPE_SUBMIT_SOLUTION,
    );
    dispatch_test!(
        from_client_owned,
        HandleTemplateDistributionMessagesFromClientOwnedSync,
        HandleTemplateDistributionMessagesFromClientOwnedAsync,
        get_negotiated_extensions_with_client,
        handle_template_distribution_message_frame_from_client,
        handle_coinbase_output_constraints(CoinbaseOutputConstraintsOwned) => MESSAGE_TYPE_COINBASE_OUTPUT_CONSTRAINTS,
        handle_request_tx_data(RequestTransactionDataOwned) => MESSAGE_TYPE_REQUEST_TRANSACTION_DATA,
        handle_submit_solution(SubmitSolutionOwned) => MESSAGE_TYPE_SUBMIT_SOLUTION,
    );
}

mod extensions {
    use super::*;
    use extensions_sv2::*;

    const EXTENSION_TYPE: u16 = EXTENSION_TYPE_EXTENSIONS_NEGOTIATION;
    const MESSAGES: &[(u8, bool)] = &[
        (
            MESSAGE_TYPE_REQUEST_EXTENSIONS,
            CHANNEL_BIT_REQUEST_EXTENSIONS,
        ),
        (
            MESSAGE_TYPE_REQUEST_EXTENSIONS_SUCCESS,
            CHANNEL_BIT_REQUEST_EXTENSIONS_SUCCESS,
        ),
        (
            MESSAGE_TYPE_REQUEST_EXTENSIONS_ERROR,
            CHANNEL_BIT_REQUEST_EXTENSIONS_ERROR,
        ),
    ];

    dispatch_test!(
        from_server,
        HandleExtensionsFromServerSync,
        HandleExtensionsFromServerAsync,
        get_negotiated_extensions_with_server,
        handle_extensions_message_frame_from_server,
        handle_request_extensions_success(RequestExtensionsSuccess<'_>) => MESSAGE_TYPE_REQUEST_EXTENSIONS_SUCCESS,
        handle_request_extensions_error(RequestExtensionsError<'_>) => MESSAGE_TYPE_REQUEST_EXTENSIONS_ERROR,
    );
    dispatch_test!(
        from_server_owned,
        HandleExtensionsFromServerOwnedSync,
        HandleExtensionsFromServerOwnedAsync,
        get_negotiated_extensions_with_server,
        handle_extensions_message_frame_from_server,
        handle_request_extensions_success(RequestExtensionsSuccessOwned) => MESSAGE_TYPE_REQUEST_EXTENSIONS_SUCCESS,
        handle_request_extensions_error(RequestExtensionsErrorOwned) => MESSAGE_TYPE_REQUEST_EXTENSIONS_ERROR,
    );
    dispatch_test!(
        from_client,
        HandleExtensionsFromClientSync,
        HandleExtensionsFromClientAsync,
        get_negotiated_extensions_with_client,
        handle_extensions_message_frame_from_client,
        handle_request_extensions(RequestExtensions<'_>) => MESSAGE_TYPE_REQUEST_EXTENSIONS,
    );
    dispatch_test!(
        from_client_owned,
        HandleExtensionsFromClientOwnedSync,
        HandleExtensionsFromClientOwnedAsync,
        get_negotiated_extensions_with_client,
        handle_extensions_message_frame_from_client,
        handle_request_extensions(RequestExtensionsOwned) => MESSAGE_TYPE_REQUEST_EXTENSIONS,
    );
}
