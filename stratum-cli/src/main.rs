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
    /// Stratum V2 Job Declarator Client
    JDClient(RoleArgs),
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
    Config::builder()
        .add_source(File::new(config_path, FileFormat::Toml))
        .build().ok().and_then(|s| s.try_deserialize::<T>().ok())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
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
        Some(Commands::JDClient (role_args)) => {
            match &role_args.command {
                Some(RoleCommands::StartLocal) => {
                    let path = "../roles/jd-client/config-examples/jdc-config-local-example.toml".to_string();
                    let config: jd_client::proxy_config::ProxyConfig  = parse_file(&path).unwrap();
                    jd_client::JobDeclaratorClient::new(config).start().await;
                }
                Some(RoleCommands::StartHosted) => {
                    let path = "../roles/jd-client/config-examples/jdc-config-hosted-example.toml".to_string();
                    let config: jd_client::proxy_config::ProxyConfig  = parse_file(&path).unwrap();
                    jd_client::JobDeclaratorClient::new(config).start().await;
                }
                Some(RoleCommands::StartConfig { path }) => {
                    let config: jd_client::proxy_config::ProxyConfig = parse_file(path).unwrap();
                    jd_client::JobDeclaratorClient::new(config).start().await;
                }
                None => {
                    println!("JDClient No command");
                }
            }
        }
        None => {
            println!("No command");
        }
    }
}
