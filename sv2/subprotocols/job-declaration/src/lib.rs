//! # Job Declaration Protocol
//!
//! `job_declaration_sv2` is a Rust crate that implements a set of messages defined in the Job
//! Declaration Protocol of Stratum V2.  This protocol runs between the Job Declarator Server
//! (JDS) and Job Declarator Client (JDC).
//!
//! ## Build Options
//! This crate can be built with the following features:
//! - `std`: Enables support for standard library features.
//!
//! For further information about the messages, please refer to [Stratum V2 documentation - Job
//! Declaration](https://stratumprotocol.org/specification/06-Job-Declaration-Protocol/).

#![no_std]

extern crate alloc;
mod allocate_mining_job_token;
mod declare_mining_job;
mod provide_missing_transactions;
mod push_solution;

pub use allocate_mining_job_token::{AllocateMiningJobToken, AllocateMiningJobTokenSuccess};
pub use declare_mining_job::{DeclareMiningJob, DeclareMiningJobError, DeclareMiningJobSuccess};
pub use provide_missing_transactions::{
    ProvideMissingTransactions, ProvideMissingTransactionsSuccess,
};
pub use push_solution::PushSolution;

// Job Declaration Protocol message types.
pub const MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN: u8 = 0x50;
pub const MESSAGE_TYPE_ALLOCATE_MINING_JOB_TOKEN_SUCCESS: u8 = 0x51;
pub const MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS: u8 = 0x55;
pub const MESSAGE_TYPE_PROVIDE_MISSING_TRANSACTIONS_SUCCESS: u8 = 0x56;
pub const MESSAGE_TYPE_DECLARE_MINING_JOB: u8 = 0x57;
pub const MESSAGE_TYPE_DECLARE_MINING_JOB_SUCCESS: u8 = 0x58;
pub const MESSAGE_TYPE_DECLARE_MINING_JOB_ERROR: u8 = 0x59;
pub const MESSAGE_TYPE_PUSH_SOLUTION: u8 = 0x60;

// In the Job Declaration protocol, the `channel_msg` bit is always unset,
// except for `SUBMIT_SOLUTION_JD`, which requires a specific channel reference.
pub const CHANNEL_BIT_ALLOCATE_MINING_JOB_TOKEN: bool = false;
pub const CHANNEL_BIT_ALLOCATE_MINING_JOB_TOKEN_SUCCESS: bool = false;
pub const CHANNEL_BIT_DECLARE_MINING_JOB: bool = false;
pub const CHANNEL_BIT_DECLARE_MINING_JOB_SUCCESS: bool = false;
pub const CHANNEL_BIT_DECLARE_MINING_JOB_ERROR: bool = false;
pub const CHANNEL_BIT_PROVIDE_MISSING_TRANSACTIONS: bool = false;
pub const CHANNEL_BIT_PROVIDE_MISSING_TRANSACTIONS_SUCCESS: bool = false;
pub const CHANNEL_BIT_SUBMIT_SOLUTION_JD: bool = true;
