//! # Job Declaration Protocol
//!
//! `job_declaration_sv2` is a Rust crate that implements a set of messages defined in the Job
//! Declaration Protocol of Stratum V2.  This protocol runs between the Job Declarator Server
//! (JDS) and Job Declarator Client (JDC).
//!
//! ## Build Options
//! This crate can be built with the following features:
//! - `std`: Enables support for standard library features.
//! - `with_serde`: Enables support for serialization and de-serialization using Serde.
//!
//! Note that `with_serde` feature flag is only used for the Message Generator, and deprecated for
//! any other kind of usage. It will likely be fully deprecated in the future.
//!
//! For further information about the messages, please refer to [Stratum V2 documentation - Job
//! Declaration](https://stratumprotocol.org/specification/06-Job-Declaration-Protocol/).

#![no_std]

extern crate alloc;
mod allocate_mining_job_token;
mod declare_mining_job;
mod identify_transactions;
mod provide_missing_transactions;
mod submit_solution;

pub use allocate_mining_job_token::{AllocateMiningJobToken, AllocateMiningJobTokenSuccess};
pub use declare_mining_job::{DeclareMiningJob, DeclareMiningJobError, DeclareMiningJobSuccess};
pub use identify_transactions::{IdentifyTransactions, IdentifyTransactionsSuccess};
pub use provide_missing_transactions::{
    ProvideMissingTransactions, ProvideMissingTransactionsSuccess,
};
pub use submit_solution::SubmitSolutionJd;
