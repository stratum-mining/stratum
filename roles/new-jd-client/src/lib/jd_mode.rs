use std::sync::atomic::{AtomicU8, Ordering};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JdMode {
    CoinbaseOnly = 0,
    TemplateOnly = 1,
    SoloMining = 2,
}

impl From<u8> for JdMode {
    fn from(val: u8) -> Self {
        match val {
            0 => JdMode::CoinbaseOnly,
            1 => JdMode::TemplateOnly,
            2 => JdMode::SoloMining,
            _ => JdMode::SoloMining,
        }
    }
}

impl From<u32> for JdMode {
    fn from(val: u32) -> Self {
        match val {
            0 => JdMode::CoinbaseOnly,
            1 => JdMode::TemplateOnly,
            2 => JdMode::SoloMining,
            _ => JdMode::SoloMining,
        }
    }
}

impl From<JdMode> for u8 {
    fn from(mode: JdMode) -> Self {
        mode as u8
    }
}

pub static JD_MODE: AtomicU8 = AtomicU8::new(JdMode::TemplateOnly as u8);

pub fn set_jd_mode(mode: JdMode) {
    JD_MODE.store(mode as u8, Ordering::SeqCst);
}

pub fn get_jd_mode() -> JdMode {
    JD_MODE.load(Ordering::SeqCst).into()
}
