use serde::{Deserialize, Deserializer};

mod coinbase_output;
pub use coinbase_output::{CoinbaseOutput, Error as CoinbaseOutputError};

pub mod logging;

mod toml;
pub use toml::duration_from_toml;

/// Deserialize an object, allowing it to be encoded either directly or as
/// a singleton vector.
///
/// Returns a singleton vector in either case.
///
/// This function was created as part of https://github.com/stratum-mining/stratum/pull/1720
/// as a first step in transitioning the `coinbase_outputs` field of various configuration
/// files away from a vector toward a single object. As part of a larger refactoring, it
/// should be changed to return a single [`CoinbaseOutput`] directly.
pub fn deserialize_vec_exactly_1<'de, D>(d: D) -> Result<Vec<CoinbaseOutput>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error as _;

    // Serde will attempt `Single` first, then `Vec` if that fails.
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Helper {
        Single(CoinbaseOutput),
        Vec(Vec<CoinbaseOutput>),
    }

    match Helper::deserialize(d) {
        Err(_) => {
            // The errors yielded by serde are meaningless to the user and
            // expose the name of the private `Helper` type. Override them
            // with something equally useless but less confusing.
            Err(D::Error::custom(
                "could not parse descriptor string (or old-style list format)",
            ))
        }
        Ok(Helper::Single(ret)) => Ok(vec![ret]),
        Ok(Helper::Vec(ret)) => {
            if ret.len() != 1 {
                return Err(D::Error::invalid_length(
                    ret.len(),
                    &"a list with exactly one coinbase output, or a single descriptor string",
                ));
            }
            Ok(ret)
        }
    }
}
