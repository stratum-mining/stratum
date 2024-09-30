use async_channel::Sender;

#[derive(Debug, Clone)]
pub(crate) struct NewWorkNotifier {
    pub(crate) should_send: bool,
    pub(crate) sender: Sender<()>,
}
