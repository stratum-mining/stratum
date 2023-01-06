use crate::error::{self, Error};

#[derive(Debug)]
pub enum State<'a> {
    Shutdown(Error<'a>),
    Healthy(String),
}

#[derive(Debug)]
pub struct Status<'a> {
    pub state: State<'a>,
}

// this is called by `error_handling::handle_result!`
pub async fn handle_error(
    sender: async_channel::Sender<Status<'static>>,
    e: error::Error<'static>,
) {
    match sender
        .send(Status {
            state: State::Shutdown(e),
        })
        .await
    {
        Ok(_) => {
            // implement error specific handling here instead of panicking
            panic!("Error")
        }
        Err(_e) => {
            // implement error specific handling here instead of panicking
            panic!("Error")
        }
    }
}
