use roles_logic_sv2::handlers::job_negotiation::ParseClientJobNegotiationMessages;
use crate::lib::job_negotiator::JobNegotiatorDownstream;
use roles_logic_sv2::job_negotiation_sv2::{
    AllocateMiningJobToken, CommitMiningJob, IdentifyTransactionsSuccess, ProvideMissingTransactionsSuccess, AllocateMiningJobTokenSuccess,
};
use roles_logic_sv2::handlers::SendTo_;
use roles_logic_sv2::parsers::JobNegotiation;
pub type SendTo = SendTo_<JobNegotiation<'static>, ()>;
use roles_logic_sv2::errors::Error;


impl ParseClientJobNegotiationMessages for JobNegotiatorDownstream{

    fn allocate_mining_job_token(
        &mut self,
        message: AllocateMiningJobToken,
    ) -> Result<SendTo, Error>{
        let message_success = AllocateMiningJobTokenSuccess{
            request_id: todo!(),
            mining_job_token: todo!(),
            coinbase_output_max_additional_size: todo!(),
            async_mining_allowed: todo!(),
        };
        let message_enum = JobNegotiation::AllocateMiningJobTokenSuccess(message_success);
        Ok(SendTo::Respond(message_enum))
    }
    fn commit_mining_job(&mut self, message: CommitMiningJob) -> Result<SendTo, Error>{
        todo!()
    }
    fn identify_transactions_success(
        &mut self,
        message: IdentifyTransactionsSuccess,
    ) -> Result<SendTo, Error>{
        todo!()
    }
    fn provide_missing_transactions_success(
        &mut self,
        message: ProvideMissingTransactionsSuccess,
    ) -> Result<SendTo, Error>{
        todo!()
    }
}