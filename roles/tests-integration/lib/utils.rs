use once_cell::sync::Lazy;
use std::{
    collections::HashSet,
    net::{SocketAddr, TcpListener},
    sync::Mutex,
};

// prevents get_available_port from ever returning the same port twice
static UNIQUE_PORTS: Lazy<Mutex<HashSet<u16>>> = Lazy::new(|| Mutex::new(HashSet::new()));

pub fn get_available_address() -> SocketAddr {
    let port = get_available_port();
    SocketAddr::from(([127, 0, 0, 1], port))
}

fn get_available_port() -> u16 {
    let mut unique_ports = UNIQUE_PORTS.lock().unwrap();

    loop {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        if !unique_ports.contains(&port) {
            unique_ports.insert(port);
            return port;
        }
    }
}
