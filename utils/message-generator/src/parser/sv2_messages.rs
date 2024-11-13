use binary_sv2::{Deserialize, Serialize};
use roles_logic_sv2::{
    common_messages_sv2::{SetupConnectionError, SetupConnectionSuccess},
    parsers::AnyMessage,
};
use std::collections::HashMap;

/// It takes a path and an id. If at `path` there is a file, then it loads it and tries to
/// transform it in `TestmessageParser`. Therefore, with into_map, trasforms the
/// `TestMessageParser` in HashMap (id -> AnyMessage) and tries to take the value that corresponds
/// to id
pub fn message_from_path(path: &[String]) -> AnyMessage<'static> {
    let id = path[1].clone();
    let path = path[0].clone();
    let messages = load_str!(&path);
    let parsed = TestMessageParser::from_str(messages);
    parsed
        .into_map()
        .get(&id)
        .unwrap_or_else(|| panic!("There is no value matching the id {:?}", id))
        .0
        .clone()
}

/// This parses a json object that may or may not (and in this case field is None) have a value
/// with a particular key. While parsing the file below, the mining_message filed is None
//        {
//            "common_messages": [
//                {
//                    "message": {
//                        "type": "SetupConnection",
//                        "protocol": 0,
//                        "min_version": 2,
//                        "max_version": 2,
//                        "flags": 1,
//                        "endpoint_host": "",
//                        "endpoint_port": 0,
//                        "vendor": "",
//                        "hardware_version": "",
//                        "firmware": "",
//                        "device_id": ""
//                    },
//                    "id": "setup_connection_mining_hom"
//                },
//                {
//                    "message": {
//                        "type": "SetupConnectionSuccess",
//                        "flags": 0,
//                        "used_version": 2
//                    },
//                    "id": "setup_connection_success_flag_0"
//                }
//            ]
//        }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestMessageParser<'a> {
    #[serde(borrow)]
    common_messages: Option<Vec<CommonMessage<'a>>>,
    #[serde(borrow)]
    job_declaration_messages: Option<Vec<JobDeclarationMessage<'a>>>,
    #[serde(borrow)]
    mining_messages: Option<Vec<MiningMessage<'a>>>,
    #[serde(borrow)]
    template_distribution_messages: Option<Vec<TemplateDistributionMessage<'a>>>,
}
/// This is not the same CommonMessages as the SRI, but the fiel message is. This structure is
/// needed because we use the id as a key to retrieve the message; this key is not part of the SRI
/// type CommonMessage<'a>
//                      {
//Defines an SRI messag     "message": {
//                              "type": "SetupConnectionSuccess",
//                              "flags": 0,
//                              "used_version": 2
//                          },
//This is contained         "id": "setup_connection_success_flag_0"
//field "id"            }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplaceField {
    pub field_name: String,
    pub keyword: String,
}
impl ReplaceField {
    #[allow(dead_code)]
    fn from_vec_string_string(input: (String, String)) -> ReplaceField {
        ReplaceField {
            field_name: input.0,
            keyword: input.1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CommonMessage<'a> {
    #[serde(borrow)]
    message: CommonMessages<'a>,
    id: String,
    replace_fields: Option<Vec<ReplaceField>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JobDeclarationMessage<'a> {
    #[serde(borrow)]
    message: JobDeclaration<'a>,
    id: String,
    // filed_name, keyword
    replace_fields: Option<Vec<ReplaceField>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MiningMessage<'a> {
    #[serde(borrow)]
    message: Mining<'a>,
    id: String,
    // filed_name, keyword
    replace_fields: Option<Vec<ReplaceField>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TemplateDistributionMessage<'a> {
    #[serde(borrow)]
    message: TemplateDistribution<'a>,
    id: String,
    // filed_name, keyword
    replace_fields: Option<Vec<ReplaceField>>,
}

impl<'a> TestMessageParser<'a> {
    pub fn into_map(self) -> HashMap<String, (AnyMessage<'a>, Vec<ReplaceField>)> {
        let mut map = HashMap::new();
        if let Some(common_messages) = self.common_messages {
            for message in common_messages {
                let id = message.id;
                let replace_fields = match message.replace_fields {
                    Some(replace_fields) => replace_fields,
                    None => vec![],
                };
                let message = message.message.into();
                map.insert(id, (message, replace_fields));
            }
        };
        if let Some(job_declaration_messages) = self.job_declaration_messages {
            for message in job_declaration_messages {
                let id = message.id;
                let replace_fields = match message.replace_fields {
                    Some(replace_fields) => replace_fields,
                    None => vec![],
                };
                let message = message.message.into();
                let message = AnyMessage::JobDeclaration(message);
                map.insert(id, (message, replace_fields));
            }
        };
        if let Some(mining_messages) = self.mining_messages {
            for message in mining_messages {
                let id = message.id;
                let replace_fields = match message.replace_fields {
                    Some(replace_fields) => replace_fields,
                    None => vec![],
                };
                let message = message.message.into();
                let message = AnyMessage::Mining(message);
                map.insert(id, (message, replace_fields));
            }
        };
        if let Some(template_distribution_messages) = self.template_distribution_messages {
            for message in template_distribution_messages {
                let id = message.id;
                let replace_fields = match message.replace_fields {
                    Some(replace_fields) => replace_fields,
                    None => vec![],
                };
                let message = message.message.into();
                let message = AnyMessage::TemplateDistribution(message);
                map.insert(id, (message, replace_fields));
            }
        };
        map
    }

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

        let v: TestMessageParser = serde_json::from_str(data).unwrap();
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

        let v: TestMessageParser = serde_json::from_str(data).unwrap();
        let v = v.into_map();
        match v.get("setup_connection").unwrap().0 {
            AnyMessage::Common(
                roles_logic_sv2::parsers::CommonMessages::SetupConnectionSuccess(m),
            ) => {
                assert!(m.used_version == 2);
                assert!(m.flags == 0);
            }
            _ => panic!(),
        }
        match &v.get("close_channel").unwrap().0 {
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
    job_declaration_sv2::*,
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
pub enum JobDeclaration<'a> {
    #[serde(borrow)]
    AllocateMiningJobTokenSuccess(AllocateMiningJobTokenSuccess<'a>),
    #[serde(borrow)]
    AllocateMiningJobToken(AllocateMiningJobToken<'a>),
    #[serde(borrow)]
    DeclareMiningJob(DeclareMiningJob<'a>),
    #[serde(borrow)]
    DeclareMiningJobSuccess(DeclareMiningJobSuccess<'a>),
    #[serde(borrow)]
    DeclareMiningJobError(DeclareMiningJobError<'a>),
    IdentifyTransactions(IdentifyTransactions),
    #[serde(borrow)]
    IdentifyTransactionsSuccess(IdentifyTransactionsSuccess<'a>),
    #[serde(borrow)]
    ProvideMissingTransactions(ProvideMissingTransactions<'a>),
    #[serde(borrow)]
    ProvideMissingTransactionsSuccess(ProvideMissingTransactionsSuccess<'a>),
    #[serde(borrow)]
    SubmitSolution(SubmitSolutionJd<'a>),
}

impl<'a> From<JobDeclaration<'a>> for roles_logic_sv2::parsers::JobDeclaration<'a> {
    fn from(v: JobDeclaration<'a>) -> Self {
        match v {
            JobDeclaration::AllocateMiningJobTokenSuccess(m) => {
                Self::AllocateMiningJobTokenSuccess(m)
            }
            JobDeclaration::AllocateMiningJobToken(m) => Self::AllocateMiningJobToken(m),
            JobDeclaration::DeclareMiningJobSuccess(m) => Self::DeclareMiningJobSuccess(m),
            JobDeclaration::DeclareMiningJob(m) => Self::DeclareMiningJob(m),
            JobDeclaration::DeclareMiningJobError(m) => Self::DeclareMiningJobError(m),
            JobDeclaration::IdentifyTransactions(m) => Self::IdentifyTransactions(m),
            JobDeclaration::IdentifyTransactionsSuccess(m) => Self::IdentifyTransactionsSuccess(m),
            JobDeclaration::ProvideMissingTransactions(m) => Self::ProvideMissingTransactions(m),
            JobDeclaration::ProvideMissingTransactionsSuccess(m) => {
                Self::ProvideMissingTransactionsSuccess(m)
            }
            JobDeclaration::SubmitSolution(m) => Self::SubmitSolution(m),
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
    SetCustomMiningJobSuccess(SetCustomMiningJobSuccess),
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
