/// # Description
/// This macro handles errors inserting error handling logic for a given `Result<T, crate::error::Error<'a>>`
/// it is used by passing in a `Sender<crate::status::Status>` as the first parameter
/// and a `Result<T, crate::error::Error<'a>>` as the second parameter.
///
/// NOTE: can only be used within async functions since status needs to be send over async channel and the macro
/// must be able to reference the user defined function `crate::status::handle_error(T, U);` where U is the output
/// of `e.into()`
///
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
                // handle error
                crate::status::handle_error($sender, e.into()).await;
                panic!("ERROR");
            }
        }
    };
}
