use mining_sv2::ExtendedExtranonceError;

#[derive(Debug, Clone)]
pub enum ExtranoncePrefixFactoryError {
    FailedToCreateInnerFactory(ExtendedExtranonceError),
    MessageSenderError,
    ResponseReceiverError,
    UnexpectedResponse,
    FailedToGenerateNextExtranoncePrefixExtended(ExtendedExtranonceError),
    FailedToGenerateNextExtranoncePrefixStandard(ExtendedExtranonceError),
}
