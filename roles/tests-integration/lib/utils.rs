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

pub mod http {
    pub fn make_get_request(download_url: &str, retries: usize) -> Vec<u8> {
        for attempt in 1..=retries {
            let response = minreq::get(download_url).send();
            match response {
                Ok(res) => {
                    let status_code = res.status_code;
                    if (200..300).contains(&status_code) {
                        return res.as_bytes().to_vec();
                    } else if (500..600).contains(&status_code) {
                        eprintln!(
                            "Attempt {}: URL {} returned a server error code {}",
                            attempt, download_url, status_code
                        );
                    } else {
                        panic!(
                            "URL {} returned unexpected status code {}. Aborting.",
                            download_url, status_code
                        );
                    }
                }
                Err(err) => {
                    eprintln!(
                        "Attempt {}: Failed to fetch URL {}: {:?}",
                        attempt + 1,
                        download_url,
                        err
                    );
                }
            }

            if attempt < retries {
                let delay = 1u64 << (attempt - 1);
                eprintln!("Retrying in {} seconds (exponential backoff)...", delay);
                std::thread::sleep(std::time::Duration::from_secs(delay));
            }
        }
        // If all retries fail, panic with an error message
        panic!(
            "Cannot reach URL {} after {} attempts",
            download_url, retries
        );
    }
}

pub mod tarball {
    use flate2::read::GzDecoder;
    use std::{
        fs::File,
        io::{BufReader, Read},
        path::Path,
    };
    use tar::Archive;

    pub fn read_from_file(path: &str) -> Vec<u8> {
        let file = File::open(path).unwrap_or_else(|_| {
            panic!(
                "Cannot find {:?} specified with env var BITCOIND_TARBALL_FILE",
                path
            )
        });
        let mut reader = BufReader::new(file);
        let mut buffer = Vec::new();
        reader.read_to_end(&mut buffer).unwrap();
        buffer
    }

    pub fn unpack(tarball_bytes: &[u8], destination: &Path) {
        let decoder = GzDecoder::new(tarball_bytes);
        let mut archive = Archive::new(decoder);
        for mut entry in archive.entries().unwrap().flatten() {
            if let Ok(file) = entry.path() {
                if file.ends_with("bitcoind") {
                    entry.unpack_in(destination).unwrap();
                }
            }
        }
    }
}
