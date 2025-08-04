pub struct Downstream;
mod message_handler;

impl Downstream {
    pub fn new() -> Self {
        Downstream
    }

    pub async fn start(&self) {}
}
