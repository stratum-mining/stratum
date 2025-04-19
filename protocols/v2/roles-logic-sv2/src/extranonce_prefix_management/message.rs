pub enum InnerExtranoncePrefixFactoryMessage {
    Shutdown,
    NextPrefixExtended(usize),
    NextPrefixStandard,
}
