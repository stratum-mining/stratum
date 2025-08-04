mod message_handler;

pub struct Upstream;

impl Upstream {
    pub fn new() -> Self {
        Upstream
    }

    pub async fn start(&self) {}
}
