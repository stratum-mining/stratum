use tokio::sync::Mutex;
use tokio::net::TcpStream;
use tokio::io::{AsyncWriteExt, AsyncReadExt};
use serde_json::{json, Value};
use log::{info, debug, error};

#[derive(Debug)]
pub struct AuthService {
    auth_server_addr: String,
}

impl AuthService {
    pub async fn authorize(&self) -> bool {
        debug!("Starting authorization process");

        let mut stream = match TcpStream::connect(&self.auth_server_addr).await {
            Ok(stream) => {
                debug!("Connected to auth server at {}", self.auth_server_addr);
                stream
            },
            Err(e) => {
                error!("Failed to connect to auth server: {:?}", e);
                return false;
            }
        };

        let auth_request = json!({
            "id": 1,
            "method": "mining.authorize",
            "params": ["username.worker", "password"]
        });

        let request_str = auth_request.to_string() + "\n";
        debug!("Sending authorization request: {}", request_str);

        if let Err(e) = stream.write_all(request_str.as_bytes()).await {
            error!("Failed to send request: {:?}", e);
            return false;
        }

        debug!("Request sent, waiting for response");

        let mut buffer = [0; 1024];
        match stream.read(&mut buffer).await {
            Ok(n) => {
                if n == 0 {
                    error!("Server closed the connection");
                    return false;
                }
                let response = String::from_utf8_lossy(&buffer[..n]);
                debug!("Received response ({} bytes): {}", n, response.trim());

                match serde_json::from_str::<Value>(&response) {
                    Ok(json_response) => {
                        if let Some(result) = json_response.get("result") {
                            let authorized = result.as_bool().unwrap_or(false);
                            info!("Authorization result: {}", authorized);
                            authorized
                        } else {
                            error!("Invalid response format from auth server");
                            false
                        }
                    }
                    Err(e) => {
                        error!("Failed to parse response: {:?}", e);
                        false
                    }
                }
            }
            Err(e) => {
                error!("Failed to read response: {:?}", e);
                false
            }
        }
    }
}

pub type SharedAuthService = Mutex<AuthService>;

pub fn new_auth_service() -> SharedAuthService {
    Mutex::new(AuthService {
        auth_server_addr: "localhost:3334".to_string(),
    })
}