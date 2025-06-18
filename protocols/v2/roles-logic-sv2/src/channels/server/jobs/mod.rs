pub mod error;
pub mod extended;
pub mod factory;
pub mod standard;

use mining_sv2::SetCustomMiningJob;
use template_distribution_sv2::NewTemplate;

#[derive(Clone, Debug, PartialEq)]
pub enum JobOrigin<'a> {
    NewTemplate(NewTemplate<'a>),
    SetCustomMiningJob(SetCustomMiningJob<'a>),
}
