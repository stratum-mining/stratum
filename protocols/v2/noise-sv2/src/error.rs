#[derive(Debug)]
pub struct Error {}
pub type Result<T> = std::result::Result<T, Error>;

impl From<std::io::Error> for Error {
    fn from(_: std::io::Error) -> Self {
        Error {}
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        Ok(())
    }
}
