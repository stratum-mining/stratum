/// Generator of unique IDs for channels and groups.
///
/// It keeps an internal counter, which is incremented every time a new unique id is requested.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct IdFactory {
    state: u32,
}

impl IdFactory {
    /// Creates a new [`Id`] instance initialized to `0`.
    pub fn new() -> Self {
        Self { state: 0 }
    }

    /// Increments then returns the internal state on a new ID.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u32 {
        self.state += 1;
        self.state
    }
}

impl Default for IdFactory {
    fn default() -> Self {
        Self::new()
    }
}
