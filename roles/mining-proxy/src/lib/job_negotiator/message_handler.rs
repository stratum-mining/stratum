use crate::lib::job_declarator::JobDeclarator;
use roles_logic_sv2::{
    handlers::{job_declaration::ParseServerJobDeclarationMessages, SendTo_},
    parsers::JobDeclaration,
};
pub type SendTo = SendTo_<JobDeclaration<'static>, ()>;
use roles_logic_sv2::errors::Error;

impl ParseServerJobDeclarationMessages for JobDeclarator {
    fn handle_allocate_mining_job_token_sucess(
        &mut self,
        message: roles_logic_sv2::job_declaration_sv2::AllocateMiningJobTokenSuccess,
    ) -> Result<roles_logic_sv2::handlers::job_declaration::SendTo, Error> {
        Ok(SendTo::None(Some(
            JobDeclaration::AllocateMiningJobTokenSuccess(message.into_static()),
        )))
    }

    // We assume that server send success so we are already working on that job, notthing to do.
    fn handle_commit_mining_job_success(
        &mut self,
        _message: roles_logic_sv2::job_declaration_sv2::CommitMiningJobSuccess,
    ) -> Result<roles_logic_sv2::handlers::job_declaration::SendTo, Error> {
        Ok(SendTo::None(None))
    }
}
