#[derive(Debug)]
pub struct Error {}
pub type Result<T> = core::result::Result<T, Error>;

//impl From<core::io::Error> for Error {
//    fn from(_: core::io::Error) -> Self {
//        Error {}
//    }
//}

impl core::fmt::Display for Error {
    fn fmt(&self, _: &mut core::fmt::Formatter<'_>) -> core::result::Result<(), core::fmt::Error> {
        Ok(())
    }
}
