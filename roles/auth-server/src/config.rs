use serde::Deserialize;
use config::{Config as ConfigSource, ConfigError, File, Environment};

#[derive(Deserialize)]
pub struct Config {
    pub host: String,
    pub port: u16,
}

impl Config {
    pub fn load() -> Result<Self, ConfigError> {
        let builder = ConfigSource::builder()
            .add_source(File::with_name("auth_config").required(false))
            .add_source(Environment::with_prefix("AUTH"));

        builder.build()?.try_deserialize()
    }
}