use binary_sv2::{self, Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestMsg {
    pub data: u8,
}
