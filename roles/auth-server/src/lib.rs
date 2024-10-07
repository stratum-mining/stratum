use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use log::{info, debug, error};

mod config;
use config::Config;

#[derive(Deserialize)]
struct AuthorizeRequest {
    id: u64,
    method: String,
    params: Vec<String>,
}

#[derive(Serialize)]
struct AuthorizeResponse {
    id: u64,
    result: bool,
    error: Option<Value>,
}

async fn handle_client(mut socket: tokio::net::TcpStream) {
    let mut buffer = [0; 1024];

    loop {
        match socket.read(&mut buffer).await {
            Ok(0) => break,
            Ok(n) => {
                let received = String::from_utf8_lossy(&buffer[..n]);
                debug!("Received: {}", received);

                if let Ok(request) = serde_json::from_str::<AuthorizeRequest>(&received) {
                    if request.method == "mining.authorize" {
                        let response = AuthorizeResponse {
                            id: request.id,
                            result: true,
                            error: None,
                        };

                        info!("Authorizing user: {}", request.params[0]);

                        let response_json = serde_json::to_string(&response).unwrap();
                        if let Err(e) = socket.write_all(response_json.as_bytes()).await {
                            error!("Failed to send response: {}", e);
                            break;
                        }
                    }
                } else {
                    error!("Failed to parse request");
                }
            }
            Err(e) => {
                error!("Failed to read from socket: {}", e);
                break;
            }
        }
    }
}

pub async fn run_server() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("debug"));

    let config = Config::load().expect("Failed to load configuration");
    let addr = format!("{}:{}", config.host, config.port);

    let listener = TcpListener::bind(&addr).await?;
    info!("Auth server listening on {}", addr);

    loop {
        let (socket, _) = listener.accept().await?;
        tokio::spawn(async move {
            handle_client(socket).await;
        });
    }
}