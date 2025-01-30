mod client;
mod error;
mod messages;
mod server;

const ADDR: &str = "127.0.0.1:3333";
const SERVER_PUBLIC_K: &str = "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72";
const SERVER_PRIVATE_K: &str = "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n";
const SERVER_CERT_VALIDITY: std::time::Duration = std::time::Duration::from_secs(3600);

#[tokio::main]
async fn main() {
    // start the server in a separate thread
    tokio::spawn(async {
        server::start_server(
            ADDR,
            SERVER_PUBLIC_K.to_string(),
            SERVER_PRIVATE_K.to_string(),
            SERVER_CERT_VALIDITY,
        )
        .await
        .expect("Server failed");
    });

    // give the server a moment to start up
    std::thread::sleep(std::time::Duration::from_secs(1));

    // start the client
    // note: it only knows the server's pubkey!
    client::start_client(ADDR, SERVER_PUBLIC_K.to_string())
        .await
        .expect("Client failed");
}
