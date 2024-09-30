pub(crate) enum NextShareOutcome {
    ValidShare,
    InvalidShare,
}

impl NextShareOutcome {
    pub(crate) fn is_valid(&self) -> bool {
        matches!(self, NextShareOutcome::ValidShare)
    }
}
