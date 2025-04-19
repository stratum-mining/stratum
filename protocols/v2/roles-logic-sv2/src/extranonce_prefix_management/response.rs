use mining_sv2::ExtendedExtranonceError;

#[derive(Debug)]
pub enum InnerExtranoncePrefixFactoryResponse {
    Shutdown,
    NextPrefixExtended(Vec<u8>),
    NextPrefixStandard(Vec<u8>),
    FailedToGenerateNextExtranoncePrefixExtended(ExtendedExtranonceError),
    FailedToGenerateNextExtranoncePrefixStandard(ExtendedExtranonceError),
}
