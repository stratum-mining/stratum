use std::path::PathBuf;

#[derive(Debug)]
pub struct Args {
    pub config_path: PathBuf,
}

enum ArgsState {
    Next,
    ExpectPath,
    Done,
}

enum ArgsResult {
    Config(PathBuf),
    None,
    Help(String),
}

impl Args {
    const DEFAULT_CONFIG_PATH: &'static str = "translator-config.toml";

    pub fn from_args() -> Result<Self, String> {
        let cli_args = std::env::args();

        let config_path = cli_args
            .scan(ArgsState::Next, |state, item| {
                match std::mem::replace(state, ArgsState::Done) {
                    ArgsState::Next => match item.as_str() {
                        "-c" | "--config" => {
                            *state = ArgsState::ExpectPath;
                            Some(ArgsResult::None)
                        }
                        "-h" | "--help" => Some(ArgsResult::Help(format!(
                            "Usage: -h/--help, -c/--config <path|default {}>",
                            Self::DEFAULT_CONFIG_PATH
                        ))),
                        _ => {
                            *state = ArgsState::Next;

                            Some(ArgsResult::None)
                        }
                    },
                    ArgsState::ExpectPath => Some(ArgsResult::Config(PathBuf::from(item))),
                    ArgsState::Done => None,
                }
            })
            .last();
        let config_path = match config_path {
            Some(ArgsResult::Config(p)) => p,
            Some(ArgsResult::Help(h)) => return Err(h),
            _ => PathBuf::from(Self::DEFAULT_CONFIG_PATH),
        };
        Ok(Self { config_path })
    }
}
