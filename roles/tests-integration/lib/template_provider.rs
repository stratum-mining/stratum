use corepc_node::{Conf, Node};
use flate2::read::GzDecoder;
use std::{
    env,
    fs::{create_dir_all, File},
    io::{BufReader, Read},
    path::{Path, PathBuf},
};
use tar::Archive;

const VERSION_TP: &str = "0.1.13";

fn download_bitcoind_tarball(download_url: &str) -> Vec<u8> {
    let response = minreq::get(download_url)
        .send()
        .unwrap_or_else(|_| panic!("Cannot reach URL: {}", download_url));
    assert_eq!(
        response.status_code, 200,
        "URL {} didn't return 200",
        download_url
    );
    response.as_bytes().to_vec()
}

fn read_tarball_from_file(path: &str) -> Vec<u8> {
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

fn unpack_tarball(tarball_bytes: &[u8], destination: &Path) {
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

fn get_bitcoind_filename(os: &str, arch: &str) -> String {
    match (os, arch) {
        ("macos", "aarch64") => format!("bitcoin-sv2-tp-{}-arm64-apple-darwin.tar.gz", VERSION_TP),
        ("macos", "x86_64") => format!("bitcoin-sv2-tp-{}-x86_64-apple-darwin.tar.gz", VERSION_TP),
        ("linux", "x86_64") => format!("bitcoin-sv2-tp-{}-x86_64-linux-gnu.tar.gz", VERSION_TP),
        ("linux", "aarch64") => format!("bitcoin-sv2-tp-{}-aarch64-linux-gnu.tar.gz", VERSION_TP),
        _ => format!(
            "bitcoin-sv2-tp-{}-x86_64-apple-darwin-unsigned.zip",
            VERSION_TP
        ),
    }
}

#[derive(Debug)]
pub struct TemplateProvider {
    bitcoind: Node,
}

impl TemplateProvider {
    pub fn start(port: u16, sv2_interval: u32) -> Self {
        let current_dir: PathBuf = std::env::current_dir().expect("failed to read current dir");
        let tp_dir = current_dir.join("template-provider");
        let mut conf = Conf::default();
        let staticdir = format!(".bitcoin-{}", port);
        conf.staticdir = Some(tp_dir.join(staticdir));
        let port_arg = format!("-sv2port={}", port);
        let sv2_interval_arg = format!("-sv2interval={}", sv2_interval);
        conf.args.extend(vec![
            "-txindex=1",
            "-sv2",
            &port_arg,
            "-debug=rpc",
            "-debug=sv2",
            &sv2_interval_arg,
            "-sv2feedelta=0",
            "-loglevel=sv2:trace",
            "-logtimemicros=1",
        ]);

        let os = env::consts::OS;
        let arch = env::consts::ARCH;
        let download_filename = get_bitcoind_filename(os, arch);
        let bitcoin_exe_home = tp_dir
            .join(format!("bitcoin-sv2-tp-{}", VERSION_TP))
            .join("bin");

        if !bitcoin_exe_home.exists() {
            let tarball_bytes = match env::var("BITCOIND_TARBALL_FILE") {
                Ok(path) => read_tarball_from_file(&path),
                Err(_) => {
                    let download_endpoint =
                        env::var("BITCOIND_DOWNLOAD_ENDPOINT").unwrap_or_else(|_| {
                            "https://github.com/Sjors/bitcoin/releases/download".to_owned()
                        });
                    let url = format!(
                        "{}/sv2-tp-{}/{}",
                        download_endpoint, VERSION_TP, download_filename
                    );
                    download_bitcoind_tarball(&url)
                }
            };

            if let Some(parent) = bitcoin_exe_home.parent() {
                create_dir_all(parent).unwrap();
            }

            unpack_tarball(&tarball_bytes, &tp_dir);

            if os == "macos" {
                let bitcoind_binary = bitcoin_exe_home.join("bitcoind");
                std::process::Command::new("codesign")
                    .arg("--sign")
                    .arg("-")
                    .arg(&bitcoind_binary)
                    .output()
                    .expect("Failed to sign bitcoind binary");
            }
        }

        env::set_var("BITCOIND_EXE", bitcoin_exe_home.join("bitcoind"));
        let exe_path = corepc_node::exe_path().expect("Failed to get bitcoind path");

        let bitcoind = Node::with_conf(exe_path, &conf).expect("Failed to create Node");

        TemplateProvider { bitcoind }
    }

    pub fn generate_blocks(&self, n: u64) {
        let mining_address = self
            .bitcoind
            .client
            .new_address()
            .expect("Failed to get mining address");
        self.bitcoind
            .client
            .generate_to_address(n as usize, &mining_address)
            .expect("Failed to generate blocks");
    }
}
