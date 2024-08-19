use clap::{Args, Parser, Subcommand};
use ext_config::{Config, File, FileFormat};

#[derive(Parser)]
#[command(version = "1.0.3")]
#[command(about = "Does awesome things", long_about = None)]
struct Cli {
    #[arg(short, long, action = clap::ArgAction::Count)]
    debug: u8,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Stratum V2 Translator
    Translator(RoleArgs),
}

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
struct RoleArgs {
    #[command(subcommand)]
    command: Option<RoleCommands>,
}

#[derive(Debug, Subcommand)]
enum RoleCommands {
    StartLocal,
    StartHosted,
    StartConfig { path: String },
}

fn parse_file<'a, T: serde::Deserialize<'a>>(config_path: &str) -> Option<T> {
    let settings = Config::builder()
        .add_source(File::new(config_path, FileFormat::Toml))
        .build().ok();
    if let Some(settings) = settings {
        settings.try_deserialize::<T>().ok()
    } else {
        None
    };
    None
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    match &cli.command {
        Some(Commands::Translator (role_args)) => {
            match &role_args.command {
                Some(RoleCommands::StartLocal) => {
                    let path = "../roles/translator/config-examples/tproxy-config-local-jdc-example.toml".to_string();
                    let config: translator_sv2::proxy_config::ProxyConfig = parse_file(&path).unwrap();
                    translator_sv2::TranslatorSv2::new(config).start().await;
                }
                Some(RoleCommands::StartHosted) => {
                    let path = "../roles/translator/config-examples/tproxy-config-hosted-pool-example.toml".to_string();
                    let config: translator_sv2::proxy_config::ProxyConfig = parse_file(&path).unwrap();
                    translator_sv2::TranslatorSv2::new(config).start().await;
                }
                Some(RoleCommands::StartConfig { path }) => {
                    let config: translator_sv2::proxy_config::ProxyConfig = parse_file(path).unwrap();
                    translator_sv2::TranslatorSv2::new(config).start().await;
                }
                None => {
                    println!("Translator No command");
                }
            }
        }
        None => {
            println!("No command");
        }
    }
}
