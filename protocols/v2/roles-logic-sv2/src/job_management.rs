use crate::{template_distribution_sv2::NewTemplate, utils::Id};
use mining_sv2::{NewExtendedMiningJob, NewMiningJob, SetCustomMiningJob};

/// A factory for creating jobs.
///
/// Not thread safe, we expect concurrency protection to be managed by the caller.
#[derive(Debug)]
pub struct JobFactory {
    future_template: Option<NewTemplate<'static>>,
    active_template: Option<NewTemplate<'static>>,
    job_id_factory: Id,
    extranonce_len: u8,
}

impl JobFactory {
    pub fn new(extranonce_len: u8) -> Self {
        Self {
            future_template: None,
            active_template: None,
            job_id_factory: Id::new(),
            extranonce_len,
        }
    }

    pub fn set_current_template(&mut self, template: NewTemplate<'static>) {
        self.active_template = Some(template);
    }

    pub fn set_future_template(&mut self, template: NewTemplate<'static>) {
        self.future_template = Some(template);
    }

    pub fn standard_job_from_current_template(
        &mut self,
    ) -> Result<NewMiningJob<'static>, JobFactoryError> {
        let job_id = self.job_id_factory.next();
        todo!()
    }

    pub fn extended_job_from_current_template(
        &mut self,
    ) -> Result<NewExtendedMiningJob<'static>, JobFactoryError> {
        let job_id = self.job_id_factory.next();
        todo!()
    }

    pub fn standard_job_from_future_template(
        &mut self,
    ) -> Result<NewMiningJob<'static>, JobFactoryError> {
        let job_id = self.job_id_factory.next();
        todo!()
    }

    pub fn extended_job_from_future_template(
        &mut self,
    ) -> Result<NewExtendedMiningJob<'static>, JobFactoryError> {
        let job_id = self.job_id_factory.next();
        todo!()
    }
}

pub enum JobFactoryError {}
