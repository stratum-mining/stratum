//! Global configuration for Job Declarator (JD) operating mode.
//!
//! This module defines different operating modes for the Job Declarator
//! and provides atomic accessors for setting and retrieving the current mode.
//!
//! Modes are stored in a global [`AtomicU8`] to allow safe concurrent access
//! across threads.
use std::sync::atomic::{AtomicU8, Ordering};

/// Operating modes for the Job Declarator.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JdMode {
    /// Runs in Coinbase only mode.
    CoinbaseOnly = 0,
    /// Runs in Full template mode,
    FullTemplate = 1,
    /// Runs in solo mining mode,
    SoloMining = 2,
}

impl From<u8> for JdMode {
    fn from(val: u8) -> Self {
        match val {
            0 => JdMode::CoinbaseOnly,
            1 => JdMode::FullTemplate,
            2 => JdMode::SoloMining,
            _ => JdMode::SoloMining,
        }
    }
}

impl From<u32> for JdMode {
    fn from(val: u32) -> Self {
        match val {
            0 => JdMode::CoinbaseOnly,
            1 => JdMode::FullTemplate,
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

/// Global atomic variable storing the current JD mode.
pub static JD_MODE: AtomicU8 = AtomicU8::new(JdMode::SoloMining as u8);

/// Updates the global JD mode.
pub fn set_jd_mode(mode: JdMode) {
    JD_MODE.store(mode as u8, Ordering::SeqCst);
}

/// Returns the current global JD mode.
pub fn get_jd_mode() -> JdMode {
    JD_MODE.load(Ordering::SeqCst).into()
}
