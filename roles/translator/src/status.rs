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

/// # Description
/// This macro handles errors inserting error handling logic for a given `Result<T, crate::error::Error<'a>>`
/// it is used by passing in a `Sender<crate::status::Status>` as the first parameter
/// and a `Result<T, crate::error::Error<'a>>` as the second parameter.
/// NOTE: can only be used within async functions since status needs to be send over async channel
/// # Example
/// ```
/// let (tx_status: Sender<Status>, rx_status) = async_channel::unbounded();
/// let variable = handle_result!(
///     tx_status,
///     type_.try_into()
/// );
/// ```
#[macro_export]
macro_rules! handle_result {
    ($sender:expr, $res:expr) => {
        match $res {
            Ok(val) => val,
            Err(e) => {
                tracing::error!("Error: {:?}", e);
                // send status
                $sender
                    .send($crate::status::Status {
                        state: $crate::status::State::Shutdown(e.into()),
                    })
                    .await
                    .unwrap();
                // implement error specific handling here instead of panicking
                panic!("Error");
            }
        }
    };
}
