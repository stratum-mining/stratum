pub mod error;
pub mod job_declarator;
pub mod mempool;
pub mod status;
pub mod jds_config;

use roles_logic_sv2::parsers::PoolMessages as JdsMessages;
use codec_sv2::{StandardEitherFrame, StandardSv2Frame};

pub type Message = JdsMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;