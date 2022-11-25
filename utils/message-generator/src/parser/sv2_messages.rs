use binary_sv2::{Deserialize, Serialize};
use roles_logic_sv2::{
    common_messages_sv2::{SetupConnectionError, SetupConnectionSuccess},
    parsers::AnyMessage,
};
use std::collections::HashMap;

/// Stores any number or combination of a `PoolMessage` (`CommonMessage`, `JobNegotiationMessage`,
/// `MiningMessage`, and/or `TemplateDistributionMessage`) as specified by the `test.json` file.
///
/// Each member field type has the message itself (`message`) and a message identifier (`id`). When
/// an action to send a message is present, the action looks at these messages and finds the right
/// one to send by examining each member field's type's `id`.
///
/// Each message is optional. The code supports all messages being `None`, however no tests will
/// run if this is the case. In this case, the message generator can be used to execute some
/// command specified in the `test.json`, but would be better to use a standalone bash script in
/// this case.
///
/// Q: When would we want multiples of a single message type?
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestMessageParser<'a> {
    /// Stores any number of `CommonMessage`s as specified by the `"common_messages"` key value
    /// pair in `test.json`.
    #[serde(borrow)]
    common_messages: Option<Vec<CommonMessage<'a>>>,
    /// Stores any number of `JobNegotiationMessage`s as specified by the
    /// `"job_negotiation_messages"` key value pair in `test.json`.
    #[serde(borrow)]
    job_negotiation_messages: Option<Vec<JobNegotiationMessage<'a>>>,
    /// Stores any number of `MiningMessage`s as specified by the `"mining_messages"` key value
    /// pair in `test.json`.
    #[serde(borrow)]
    mining_messages: Option<Vec<MiningMessage<'a>>>,
    /// Stores any number of `TemplateDistributionMessage`s as specified by the
    /// `"template_distribution_messages"` key value pair in `test.json`.
    #[serde(borrow)]
    template_distribution_messages: Option<Vec<TemplateDistributionMessage<'a>>>,
}

/// Represents the `PoolMessages` type `CommonMessage` and the message identifier
/// (`"common_messages"`) for the later action to locate and then send this message as specified by
/// the action.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CommonMessage<'a> {
    /// `CommonMessage` message.
    #[serde(borrow)]
    message: CommonMessages<'a>,
    /// `CommonMessage` message identifier as specified in the `test.json` (`"common_messages"`)
    /// for a later action to identify and send.
    id: String,
}

/// Represents the `PoolMessages` type `JobNegotiationMessage` and the message identifier
/// (`"job_negotiation_messages"`) for the later action to locate and then send this message as
/// specified by the action.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct JobNegotiationMessage<'a> {
    /// `JobNegotiation` message.
    #[serde(borrow)]
    message: JobNegotiation<'a>,
    /// `JobNegotiation` message identifier as specified in the `test.json`
    /// (`"job_negotiation_messages"`) for a later action to identify and send.
    id: String,
}

/// Represents the `PoolMessages` type `MiningMessage` and the message identifier
/// (`"mining_messages"`) for the later action to locate and then send this message as specified by
/// the action.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MiningMessage<'a> {
    /// `Mining` message.
    #[serde(borrow)]
    message: Mining<'a>,
    /// `MiningMessage` message identifier as specified in the `test.json` (`"mining_messages"`)
    /// for a later action to identify and send.
    id: String,
}

/// Represents the `PoolMessages` type `TemplateDistributionMessage` and the message identifier
/// (`"template_distribution_messages"`) for the later action to locate and then send this message
/// as specified by the action.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TemplateDistributionMessage<'a> {
    /// `TemplateDistribution` message.
    #[serde(borrow)]
    message: TemplateDistribution<'a>,
    /// `TemplateDistribution` message identifier as specified in the `test.json`
    /// (`"template_distribution_messages"`) for a later action to identify and send.
    id: String,
}

impl<'a> TestMessageParser<'a> {
    /// Converts the `PoolMessages` messages stored in `TestMessageParser` into a hashmap whose key
    /// is the message name as specified in the `test.json` file (`"common_messages"`,
    /// `"mining_messages"`, `"job_negotiation_messages"`, and/or
    /// `"template_distribution_messages"`), and whose value is the message struct. A future action
    /// then identifies the appropriate message to send via the message `id`.
    pub fn into_map(self) -> HashMap<String, AnyMessage<'a>> {
        let mut map = HashMap::new();
        if let Some(common_messages) = self.common_messages {
            for message in common_messages {
                let id = message.id;
                let message = message.message.into();
                map.insert(id, message);
            }
        };
        if let Some(job_negotiation_messages) = self.job_negotiation_messages {
            for message in job_negotiation_messages {
                let id = message.id;
                let message = message.message.into();
                let message = AnyMessage::JobNegotiation(message);
                map.insert(id, message);
            }
        };
        if let Some(mining_messages) = self.mining_messages {
            for message in mining_messages {
                let id = message.id;
                let message = message.message.into();
                let message = AnyMessage::Mining(message);
                map.insert(id, message);
            }
        };
        if let Some(template_distribution_messages) = self.template_distribution_messages {
            for message in template_distribution_messages {
                let id = message.id;
                let message = message.message.into();
                let message = AnyMessage::TemplateDistribution(message);
                map.insert(id, message);
            }
        };
        map
    }

    /// Parses any number or combination of `CommonMessage`, `JobNegotiationMessage`,
    /// `MiningMessage`, and/or `TemplateDistributionMessage` as specified by the `test.json` file
    /// into a `TestMessageParser`.
    pub fn from_str<'b: 'a>(test: &'b str) -> Self {
        serde_json::from_str(test).unwrap()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn it_parse_messages() {
        let data = r#"
            {
                "common_messages": [
                    {
                        "message": {
                            "type": "SetupConnectionSuccess",
                            "flags": 0,
                            "used_version": 2
                        },
                        "id": "setup_connection"
                    }
                ],
                "mining_messages": [
                    {
                        "message": {
                            "type": "CloseChannel",
                            "channel_id": 78,
                            "reason_code": "no reason"
                        },
                        "id": "close_channel"
                    }
                ]
            }
        "#;

        let v: TestMessageParser = dbg!(serde_json::from_str(data).unwrap());
        match v.common_messages.unwrap()[0].message {
            CommonMessages::SetupConnectionSuccess(m) => {
                assert!(m.used_version == 2);
                assert!(m.flags == 0);
            }
            _ => panic!(),
        }
        match &v.mining_messages.unwrap()[0].message {
            Mining::CloseChannel(m) => {
                assert!(m.channel_id == 78);
                let reason_code = m.reason_code.to_vec().clone();
                let reason_code = std::str::from_utf8(&reason_code[..]).unwrap();
                assert!(reason_code == "no reason".to_string());
            }
            _ => panic!(),
        }
    }

    #[test]
    fn it_can_be_converted_into_map() {
        let data = r#"
            {
                "common_messages": [
                    {
                        "message": {
                            "type": "SetupConnectionSuccess",
                            "flags": 0,
                            "used_version": 2
                        },
                        "id": "setup_connection"
                    }
                ],
                "mining_messages": [
                    {
                        "message": {
                            "type": "CloseChannel",
                            "channel_id": 78,
                            "reason_code": "no reason"
                        },
                        "id": "close_channel"
                    }
                ]

            }
        "#;

        let v: TestMessageParser = dbg!(serde_json::from_str(data).unwrap());
        let v = v.into_map();
        match v.get("setup_connection").unwrap() {
            AnyMessage::Common(
                roles_logic_sv2::parsers::CommonMessages::SetupConnectionSuccess(m),
            ) => {
                assert!(m.used_version == 2);
                assert!(m.flags == 0);
            }
            _ => panic!(),
        }
        match v.get("close_channel").unwrap() {
            AnyMessage::Mining(roles_logic_sv2::parsers::Mining::CloseChannel(m)) => {
                assert!(m.channel_id == 78);
                let reason_code = m.reason_code.to_vec().clone();
                let reason_code = std::str::from_utf8(&reason_code[..]).unwrap();
                assert!(reason_code == "no reason".to_string());
            }
            _ => panic!(),
        }
    }
}
use roles_logic_sv2::{
    common_messages_sv2::*,
    job_negotiation_sv2::*,
    mining_sv2::*,
    template_distribution_sv2::{
        CoinbaseOutputDataSize, NewTemplate, RequestTransactionData, RequestTransactionDataError,
        RequestTransactionDataSuccess, SetNewPrevHash as TemplateSetNewPrevHash, SubmitSolution,
    },
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CommonMessages<'a> {
    ChannelEndpointChanged(ChannelEndpointChanged),
    #[serde(borrow)]
    SetupConnection(SetupConnection<'a>),
    #[serde(borrow)]
    SetupConnectionError(SetupConnectionError<'a>),
    SetupConnectionSuccess(SetupConnectionSuccess),
}

impl<'a> From<CommonMessages<'a>> for roles_logic_sv2::parsers::CommonMessages<'a> {
    fn from(v: CommonMessages<'a>) -> Self {
        match v {
            CommonMessages::ChannelEndpointChanged(m) => Self::ChannelEndpointChanged(m),
            CommonMessages::SetupConnection(m) => Self::SetupConnection(m),
            CommonMessages::SetupConnectionError(m) => Self::SetupConnectionError(m),
            CommonMessages::SetupConnectionSuccess(m) => Self::SetupConnectionSuccess(m),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TemplateDistribution<'a> {
    CoinbaseOutputDataSize(CoinbaseOutputDataSize),
    #[serde(borrow)]
    NewTemplate(NewTemplate<'a>),
    RequestTransactionData(RequestTransactionData),
    #[serde(borrow)]
    RequestTransactionDataError(RequestTransactionDataError<'a>),
    #[serde(borrow)]
    RequestTransactionDataSuccess(RequestTransactionDataSuccess<'a>),
    #[serde(borrow)]
    SetNewPrevHash(TemplateSetNewPrevHash<'a>),
    #[serde(borrow)]
    SubmitSolution(SubmitSolution<'a>),
}

impl<'a> From<TemplateDistribution<'a>> for roles_logic_sv2::parsers::TemplateDistribution<'a> {
    fn from(v: TemplateDistribution<'a>) -> Self {
        match v {
            TemplateDistribution::CoinbaseOutputDataSize(m) => Self::CoinbaseOutputDataSize(m),
            TemplateDistribution::NewTemplate(m) => Self::NewTemplate(m),
            TemplateDistribution::RequestTransactionData(m) => Self::RequestTransactionData(m),
            TemplateDistribution::RequestTransactionDataError(m) => {
                Self::RequestTransactionDataError(m)
            }
            TemplateDistribution::RequestTransactionDataSuccess(m) => {
                Self::RequestTransactionDataSuccess(m)
            }
            TemplateDistribution::SetNewPrevHash(m) => Self::SetNewPrevHash(m),
            TemplateDistribution::SubmitSolution(m) => Self::SubmitSolution(m),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum JobNegotiation<'a> {
    #[serde(borrow)]
    AllocateMiningJobToken(AllocateMiningJobToken<'a>),
    #[serde(borrow)]
    AllocateMiningJobTokenSuccess(AllocateMiningJobTokenSuccess<'a>),
    #[serde(borrow)]
    CommitMiningJob(CommitMiningJob<'a>),
    #[serde(borrow)]
    CommitMiningJobSuccess(CommitMiningJobSuccess<'a>),
    #[serde(borrow)]
    CommitMiningJobError(CommitMiningJobError<'a>),
    IdentifyTransactions(IdentifyTransactions),
    #[serde(borrow)]
    IdentifyTransactionsSuccess(IdentifyTransactionsSuccess<'a>),
    #[serde(borrow)]
    ProvideMissingTransactions(ProvideMissingTransactions<'a>),
    #[serde(borrow)]
    ProvideMissingTransactionsSuccess(ProvideMissingTransactionsSuccess<'a>),
}

impl<'a> From<JobNegotiation<'a>> for roles_logic_sv2::parsers::JobNegotiation<'a> {
    fn from(v: JobNegotiation<'a>) -> Self {
        match v {
            JobNegotiation::AllocateMiningJobToken(m) => Self::AllocateMiningJobToken(m),
            JobNegotiation::AllocateMiningJobTokenSuccess(m) => {
                Self::AllocateMiningJobTokenSuccess(m)
            }
            JobNegotiation::CommitMiningJob(m) => Self::CommitMiningJob(m),
            JobNegotiation::CommitMiningJobSuccess(m) => Self::CommitMiningJobSuccess(m),
            JobNegotiation::CommitMiningJobError(m) => Self::CommitMiningJobError(m),
            JobNegotiation::IdentifyTransactions(m) => Self::IdentifyTransactions(m),
            JobNegotiation::IdentifyTransactionsSuccess(m) => Self::IdentifyTransactionsSuccess(m),
            JobNegotiation::ProvideMissingTransactions(m) => Self::ProvideMissingTransactions(m),
            JobNegotiation::ProvideMissingTransactionsSuccess(m) => {
                Self::ProvideMissingTransactionsSuccess(m)
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Mining<'a> {
    #[serde(borrow)]
    CloseChannel(CloseChannel<'a>),
    #[serde(borrow)]
    NewExtendedMiningJob(NewExtendedMiningJob<'a>),
    #[serde(borrow)]
    NewMiningJob(NewMiningJob<'a>),
    #[serde(borrow)]
    OpenExtendedMiningChannel(OpenExtendedMiningChannel<'a>),
    #[serde(borrow)]
    OpenExtendedMiningChannelSuccess(OpenExtendedMiningChannelSuccess<'a>),
    #[serde(borrow)]
    OpenMiningChannelError(OpenMiningChannelError<'a>),
    #[serde(borrow)]
    OpenStandardMiningChannel(OpenStandardMiningChannel<'a>),
    #[serde(borrow)]
    OpenStandardMiningChannelSuccess(OpenStandardMiningChannelSuccess<'a>),
    #[serde(borrow)]
    Reconnect(Reconnect<'a>),
    #[serde(borrow)]
    SetCustomMiningJob(SetCustomMiningJob<'a>),
    #[serde(borrow)]
    SetCustomMiningJobError(SetCustomMiningJobError<'a>),
    #[serde(borrow)]
    SetCustomMiningJobSuccess(SetCustomMiningJobSuccess<'a>),
    #[serde(borrow)]
    SetExtranoncePrefix(SetExtranoncePrefix<'a>),
    #[serde(borrow)]
    SetGroupChannel(SetGroupChannel<'a>),
    #[serde(borrow)]
    SetNewPrevHash(SetNewPrevHash<'a>),
    #[serde(borrow)]
    SetTarget(SetTarget<'a>),
    #[serde(borrow)]
    SubmitSharesError(SubmitSharesError<'a>),
    #[serde(borrow)]
    SubmitSharesExtended(SubmitSharesExtended<'a>),
    SubmitSharesStandard(SubmitSharesStandard),
    SubmitSharesSuccess(SubmitSharesSuccess),
    #[serde(borrow)]
    UpdateChannel(UpdateChannel<'a>),
    #[serde(borrow)]
    UpdateChannelError(UpdateChannelError<'a>),
}

impl<'a> From<Mining<'a>> for roles_logic_sv2::parsers::Mining<'a> {
    fn from(v: Mining<'a>) -> Self {
        match v {
            Mining::CloseChannel(m) => Self::CloseChannel(m),
            Mining::NewExtendedMiningJob(m) => Self::NewExtendedMiningJob(m),
            Mining::NewMiningJob(m) => Self::NewMiningJob(m),
            Mining::OpenExtendedMiningChannel(m) => Self::OpenExtendedMiningChannel(m),
            Mining::OpenExtendedMiningChannelSuccess(m) => {
                Self::OpenExtendedMiningChannelSuccess(m)
            }
            Mining::OpenMiningChannelError(m) => Self::OpenMiningChannelError(m),
            Mining::OpenStandardMiningChannel(m) => Self::OpenStandardMiningChannel(m),
            Mining::OpenStandardMiningChannelSuccess(m) => {
                Self::OpenStandardMiningChannelSuccess(m)
            }
            Mining::Reconnect(m) => Self::Reconnect(m),
            Mining::SetCustomMiningJob(m) => Self::SetCustomMiningJob(m),
            Mining::SetCustomMiningJobError(m) => Self::SetCustomMiningJobError(m),
            Mining::SetCustomMiningJobSuccess(m) => Self::SetCustomMiningJobSuccess(m),
            Mining::SetExtranoncePrefix(m) => Self::SetExtranoncePrefix(m),
            Mining::SetGroupChannel(m) => Self::SetGroupChannel(m),
            Mining::SetNewPrevHash(m) => Self::SetNewPrevHash(m),
            Mining::SetTarget(m) => Self::SetTarget(m),
            Mining::SubmitSharesError(m) => Self::SubmitSharesError(m),
            Mining::SubmitSharesExtended(m) => Self::SubmitSharesExtended(m),
            Mining::SubmitSharesStandard(m) => Self::SubmitSharesStandard(m),
            Mining::SubmitSharesSuccess(m) => Self::SubmitSharesSuccess(m),
            Mining::UpdateChannel(m) => Self::UpdateChannel(m),
            Mining::UpdateChannelError(m) => Self::UpdateChannelError(m),
        }
    }
}
