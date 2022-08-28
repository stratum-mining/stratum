//! Simple error handling

use std::{
    error::Error as StdErr,
    fmt::{Debug, Display, Formatter},
};

pub type Result<T> = std::result::Result<T, Error>;

/// Generic error type
pub struct Error {
    inner: Box<dyn StdErr>,
}

#[derive(Debug)]
struct AdHoc(String);

impl Display for AdHoc {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl StdErr for AdHoc {}

#[macro_export]
macro_rules! anyhow {
    ($($arg:tt)*) => ({
        $crate::Error::ad_hoc(format!($($arg)+))
    })
}

impl<E> From<E> for Error
where
    E: StdErr + Send + Sync + 'static,
{
    fn from(e: E) -> Self {
        Self { inner: Box::new(e) }
    }
}

impl Debug for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.inner)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl Error {
    pub fn ad_hoc(description: String) -> Self {
        Self {
            inner: Box::new(AdHoc(description)),
        }
    }
}
