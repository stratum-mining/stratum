use crate::error::Error;

#[derive(Debug)]
pub enum State<'a> {
    Shutdown(Error<'a>),
    Healthy(String),
}

#[derive(Debug)]
pub struct Status<'a> {
    pub state: State<'a>,
}
