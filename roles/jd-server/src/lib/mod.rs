pub mod error;
pub mod jds_config;
pub mod job_declarator;
pub mod mempool;
pub mod status;

use codec_sv2::{StandardEitherFrame, StandardSv2Frame};
use roles_logic_sv2::parsers::PoolMessages as JdsMessages;

pub type Message = JdsMessages<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;
