#[derive(Debug)]
pub enum Error {
    MissingBytes(usize),
    Todo,
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<std::io::Error> for Error {
    fn from(_: std::io::Error) -> Self {
        todo!()
    }
}

impl From<()> for Error {
    fn from(_: ()) -> Self {
        Error::Todo
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        Ok(())
    }
}
