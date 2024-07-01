mod client;
mod error;
mod messages;
mod server;

const ADDR: &str = "127.0.0.1:3333";

fn main() {
    // Start the server in a separate thread
    std::thread::spawn(|| {
        server::start_server(ADDR).expect("Server failed");
    });

    // Give the server a moment to start up
    std::thread::sleep(std::time::Duration::from_secs(1));

    // Start the client
    client::start_client(ADDR).expect("Client failed");
}
