use roles_logic_sv2::utils::CoinbaseOutput as CoinbaseOutput_;
use std::convert::TryFrom;

/// [`CoinbaseOutput`] is a struct that represents the output of a coinbase transaction.
#[derive(Debug, serde::Deserialize, Clone)]
pub struct CoinbaseOutput {
    output_script_type: String,
    output_script_value: String,
}

impl CoinbaseOutput {
    /// Creates a new [`CoinbaseOutput`].
    pub fn new(output_script_type: String, output_script_value: String) -> Self {
        Self {
            output_script_type,
            output_script_value,
        }
    }
}

impl TryFrom<&CoinbaseOutput> for CoinbaseOutput_ {
    type Error = roles_logic_sv2::Error;

    fn try_from(pool_output: &CoinbaseOutput) -> Result<Self, Self::Error> {
        match pool_output.output_script_type.as_str() {
            "P2PK" | "P2PKH" | "P2WPKH" | "P2SH" | "P2WSH" | "P2TR" => Ok(CoinbaseOutput_ {
                output_script_type: pool_output.clone().output_script_type,
                output_script_value: pool_output.clone().output_script_value,
            }),
            _ => Err(roles_logic_sv2::Error::UnknownOutputScriptType),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_try_from() {
        let possible_output_script_types = vec!["P2PK", "P2PKH", "P2WPKH", "P2SH", "P2WSH", "P2TR"];
        for output_script_type in possible_output_script_types {
            let value = "value";
            let pool_output =
                CoinbaseOutput::new(output_script_type.to_string(), value.to_string());
            let result = CoinbaseOutput_::try_from(&pool_output);
            let result = result.unwrap();
            assert_eq!(result.output_script_type, output_script_type.to_string());
            assert_eq!(result.output_script_value, value.to_string());
        }
    }

    #[test]
    fn test_try_from_unknown_output_script_type() {
        let pool_output = CoinbaseOutput::new("unknown".to_string(), "value".to_string());
        let result = CoinbaseOutput_::try_from(&pool_output);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            roles_logic_sv2::Error::UnknownOutputScriptType
        ));
    }
}
