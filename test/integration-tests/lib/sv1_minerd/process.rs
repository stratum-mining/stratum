use std::{
    fs,
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    process::{Child as TokioChild, Command as TokioCommand},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

use crate::utils::{http, tarball};

use super::error::MinerdError;

const VERSION_MINERD: &str = "2.5.1";

fn get_minerd_filename(os: &str, arch: &str) -> Result<String, MinerdError> {
    match (os, arch) {
        ("macos", "aarch64") => Ok(format!(
            "pooler-cpuminer-{VERSION_MINERD}-arm64-apple-darwin.tar.gz"
        )),
        ("macos", "x86_64") => Ok(format!(
            "pooler-cpuminer-{VERSION_MINERD}-x86_64-apple-darwin.tar.gz"
        )),
        ("linux", "x86_64") => Ok(format!(
            "pooler-cpuminer-{VERSION_MINERD}-linux-x86_64.tar.gz"
        )),
        ("linux", "aarch64") => Ok(format!(
            "pooler-cpuminer-{VERSION_MINERD}-linux-arm64.tar.gz"
        )),
        _ => Err(MinerdError::OsArchNotSupported(format!(
            "OS or architecture not supported: {} {}",
            os, arch
        ))),
    }
}

/// A wrapper struct for the minerd process that provides:
/// - TCP proxy functionality to intercept communications
/// - Process management for spawning and killing minerd
#[derive(Debug)]
pub struct MinerdProcess {
    /// Path to the minerd binary
    minerd_binary: PathBuf,
    /// Handle to the spawned minerd process
    process: Arc<Mutex<Option<TokioChild>>>,
    /// Address where the wrapper listens for minerd connections
    local_address: SocketAddr,
    /// Address of the upstream mining server
    upstream_address: SocketAddr,
    /// Whether to kill the process after the first mining.submit
    single_submit: bool,
    /// Cancellation token to coordinate shutdown of all tasks
    cancellation_token: CancellationToken,
}

impl MinerdProcess {
    /// Creates a new MinerdProcess with the given upstream address
    pub async fn new(
        upstream_address: SocketAddr,
        single_submit: bool,
    ) -> Result<Self, MinerdError> {
        let current_dir: PathBuf = std::env::current_dir().expect("failed to read current dir");
        let minerd_dir = current_dir.join("minerd");

        let os = std::env::consts::OS;
        let arch = std::env::consts::ARCH;
        let download_filename = get_minerd_filename(os, arch)?;

        if !minerd_dir.exists() {
            fs::create_dir_all(minerd_dir.clone()).expect("failed to create minerd directory");
            let download_endpoint = format!(
                "https://github.com/stratum-mining/cpuminer/releases/download/v{VERSION_MINERD}/"
            );
            let url = format!("{download_endpoint}{download_filename}");
            let tarball_bytes = http::make_get_request(&url, 5);
            println!("tarball_bytes: {:?}", tarball_bytes);
            tarball::unpack(&tarball_bytes, &minerd_dir);
        }

        let minerd_binary = minerd_dir.join("minerd");

        if os == "macos" {
            std::process::Command::new("codesign")
                .arg("--sign")
                .arg("-")
                .arg(&minerd_binary)
                .output()
                .expect("failed to sign minerd binary");
        }

        // Bind to local address for the proxy
        // use 0 to let the OS assign a randomly available port
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(MinerdError::ProxySetup)?;
        let local_address = listener.local_addr().map_err(MinerdError::ProxySetup)?;

        Ok(MinerdProcess {
            minerd_binary,
            process: Arc::new(Mutex::new(None)),
            local_address,
            upstream_address,
            single_submit,
            cancellation_token: CancellationToken::new(),
        })
    }

    /// Returns the local address where the wrapper is listening
    pub fn local_address(&self) -> SocketAddr {
        self.local_address
    }

    /// Returns the upstream address that minerd will connect to through the proxy
    pub fn upstream_address(&self) -> SocketAddr {
        self.upstream_address
    }

    /// Spawns the minerd process with the given parameters
    pub async fn spawn_minerd(
        &mut self,
        username: Option<String>,
        password: Option<String>,
    ) -> Result<(), MinerdError> {
        let mut process_guard = self
            .process
            .lock()
            .map_err(|_| MinerdError::MutexPoisoned)?;
        if process_guard.is_some() {
            return Err(MinerdError::ProcessAlreadyRunning);
        }

        let mut cmd = TokioCommand::new(&self.minerd_binary);

        // Kill the process on drop
        cmd.kill_on_drop(true);

        // Set the algorithm to sha256d
        cmd.arg("-a").arg("sha256d");

        // Set the number of threads to use for mining
        cmd.arg("--threads").arg("1");

        // Set the retry pause to 1 second
        cmd.arg("--retry-pause").arg("1");

        // Set the URL to connect to our local proxy instead of upstream directly
        cmd.arg("--url").arg(format!(
            "stratum+tcp://{}:{}",
            self.local_address.ip(),
            self.local_address.port()
        ));

        // Add username and password if provided
        if let Some(ref username) = username {
            cmd.arg("--userpass").arg(format!(
                "{}:{}",
                username,
                password.as_deref().unwrap_or("")
            ));
        }

        info!("Spawning minerd with command: {:?}", cmd);

        let child = cmd.spawn().map_err(MinerdError::ProcessSpawn)?;
        *process_guard = Some(child);
        info!("minerd process spawned successfully");
        Ok(())
    }

    /// Starts the TCP proxy to intercept communications between minerd and the upstream server
    pub async fn start_tcp_proxy(&mut self) -> Result<(), MinerdError> {
        let listener = TcpListener::bind(self.local_address)
            .await
            .map_err(MinerdError::ProxySetup)?;
        let upstream_address = self.upstream_address;
        let single_submit = self.single_submit;
        let process = Arc::clone(&self.process);
        let cancellation_token = self.cancellation_token.clone();

        tokio::spawn(async move {
            info!("Proxy server started, waiting for connections...");

            loop {
                tokio::select! {
                    accept_result = listener.accept() => {
                        match accept_result {
                            Ok((downstream_stream, _)) => {
                                info!("New connection from minerd");

                                // Connect to upstream server
                                match TcpStream::connect(upstream_address).await {
                                    Ok(upstream_stream) => {
                                        // Split streams for bidirectional communication
                                        let (downstream_read, downstream_write) = downstream_stream.into_split();
                                        let (upstream_read, upstream_write) = upstream_stream.into_split();

                                        let process_clone = Arc::clone(&process);
                                        let token_clone1 = cancellation_token.clone();
                                        let token_clone2 = cancellation_token.clone();

                                        // Task for downstream -> upstream (minerd -> pool)
                                        if single_submit {
                                            tokio::spawn(async move {
                                                let _ = Self::proxy_tcp_data_single_submit(
                                                    downstream_read,
                                                    upstream_write,
                                                    process_clone,
                                                    token_clone1,
                                                ).await;
                                            });
                                        } else {
                                            tokio::spawn(async move {
                                                let _ = Self::proxy_tcp_data(
                                                    downstream_read,
                                                    upstream_write,
                                                    token_clone1,
                                                ).await;
                                            });
                                        }

                                        // Task for upstream -> downstream (pool -> minerd)
                                        tokio::spawn(async move {
                                            let _ = Self::proxy_tcp_data(
                                                upstream_read,
                                                downstream_write,
                                                token_clone2,
                                            ).await;
                                        });
                                    }
                                    Err(e) => {
                                        error!("Failed to connect to upstream server {}: {}", upstream_address, e);
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Failed to accept connection: {}", e);
                                break;
                            }
                        }
                    }
                    _ = cancellation_token.cancelled() => {
                        info!("Proxy server shutting down due to cancellation");
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    /// Proxies data between two TCP streams with monitoring for mining.submit
    /// This function will automatically kill the process and trigger shutdown when mining.submit is
    /// detected
    async fn proxy_tcp_data_single_submit(
        mut from: tokio::net::tcp::OwnedReadHalf,
        mut to: tokio::net::tcp::OwnedWriteHalf,
        process: Arc<Mutex<Option<TokioChild>>>,
        cancellation_token: CancellationToken,
    ) {
        let mut buffer = [0; 4096];

        loop {
            tokio::select! {
                read_result = from.read(&mut buffer) => {
                    match read_result {
                        Ok(0) => {
                            debug!("Connection closed");
                            break;
                        }
                        Ok(n) => {
                            let data = &buffer[..n];

                            // Check for mining.submit and trigger shutdown
                            if let Ok(data_str) = std::str::from_utf8(data) {
                                if data_str.contains("\"mining.submit\"") {
                                    info!("Detected mining.submit, killing minerd process and triggering shutdown");

                                    // Forward the data first
                                    if let Err(e) = to.write_all(data).await {
                                        error!("Failed to write data: {}", e);
                                    }

                                    // Kill the process
                                    let child = {
                                        match process.lock() {
                                            Ok(mut process_guard) => process_guard.take(),
                                            Err(_) => {
                                                error!("Mutex poisoned while trying to kill process");
                                                None
                                            }
                                        }
                                    }; // Lock is released here

                                    if let Some(mut child) = child {
                                        if let Err(e) = child.kill().await {
                                            error!("Failed to kill minerd process: {}", e);
                                        } else {
                                            info!("minerd process killed successfully after mining.submit");
                                        }
                                    }

                                    // Trigger cancellation to stop all tasks
                                    cancellation_token.cancel();
                                    break;
                                }
                            }

                            // Forward the data
                            if let Err(e) = to.write_all(data).await {
                                error!("Failed to write data: {}", e);
                                break;
                            }
                        }
                        Err(e) => {
                            error!("Failed to read data: {}", e);
                            break;
                        }
                    }
                }
                _ = cancellation_token.cancelled() => {
                    info!("Proxy task (downstream->upstream) shutting down due to cancellation");
                    break;
                }
            }
        }
    }

    /// Proxies data between two TCP streams
    async fn proxy_tcp_data(
        mut from: tokio::net::tcp::OwnedReadHalf,
        mut to: tokio::net::tcp::OwnedWriteHalf,
        cancellation_token: CancellationToken,
    ) -> Result<(), MinerdError> {
        let mut buffer = [0; 4096];

        loop {
            tokio::select! {
                read_result = from.read(&mut buffer) => {
                    match read_result {
                        Ok(0) => {
                            debug!("Connection closed");
                            return Ok(());
                        }
                        Ok(n) => {
                            let data = &buffer[..n];

                            // Forward the data
                            if let Err(e) = to.write_all(data).await {
                                error!("Failed to write data: {}", e);
                                return Err(MinerdError::Io(e));
                            }
                        }
                        Err(e) => {
                            error!("Failed to read data: {}", e);
                            return Err(MinerdError::Io(e));
                        }
                    }
                }
                _ = cancellation_token.cancelled() => {
                    info!("TCP proxy shutting down due to cancellation");
                    return Ok(());
                }
            }
        }
    }

    /// Checks if the minerd process is still running
    pub fn is_running(&self) -> Result<bool, MinerdError> {
        let mut process_guard = self
            .process
            .lock()
            .map_err(|_| MinerdError::MutexPoisoned)?;
        if let Some(ref mut process) = *process_guard {
            match process.try_wait() {
                Ok(Some(_)) => Ok(false), // Process has exited
                Ok(None) => Ok(true),     // Process is still running
                Err(_) => Ok(false),      // Error checking process status
            }
        } else {
            Ok(false)
        }
    }

    /// Measures the hashrate of the local minerd binary in benchmark mode
    /// Returns the hashrate in hashes per second
    pub async fn measure_hashrate(&self) -> Result<f64, MinerdError> {
        info!("Starting hashrate measurement using minerd benchmark mode");

        let mut cmd = TokioCommand::new(&self.minerd_binary);

        // Set benchmark mode with specific parameters for consistent measurement
        cmd.arg("-a")
            .arg("sha256d") // Use sha256d algorithm
            .arg("-t")
            .arg("1") // Use 1 thread for consistent measurement
            .arg("--benchmark") // Enable benchmark mode (no network connection)
            .arg("-q"); // Quiet mode to reduce output noise

        // Capture stderr to parse the hashrate output (minerd outputs to stderr)
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::piped());

        info!("Running minerd benchmark: {:?}", cmd);

        let mut child = cmd.spawn().map_err(MinerdError::ProcessSpawn)?;

        let stderr = child.stderr.take().ok_or_else(|| {
            MinerdError::ProcessSpawn(std::io::Error::other("Failed to get stderr"))
        })?;

        // Give minerd some time to run and produce hashrate output
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

        // Kill the benchmark process
        if let Err(e) = child.kill().await {
            error!("Failed to kill benchmark process: {}", e);
        }

        // Read and parse the output from stderr
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();
        let mut hashrate_hashes_per_sec = None;

        // Read output lines to find hashrate information
        let mut all_output = Vec::new();
        while let Ok(bytes_read) = reader.read_line(&mut line).await {
            if bytes_read == 0 {
                break;
            }

            let line_trimmed = line.trim();
            all_output.push(line_trimmed.to_string());
            debug!("Benchmark output: {}", line_trimmed);

            // Parse hashrate from lines like:
            // "[2025-08-29 20:10:39] thread 0: 2097152 hashes, 1441 khash/s"
            // "[2025-08-29 20:10:39] Total: 1441 khash/s"
            if let Some(hashrate_khash) = parse_hashrate_from_benchmark_line(line_trimmed) {
                info!("Detected benchmark hashrate: {} khash/s", hashrate_khash);
                // Convert khash/s to hashes/s (multiply by 1000)
                hashrate_hashes_per_sec = Some(hashrate_khash * 1000.0);
                // We can break after finding the first hashrate measurement
                break;
            }

            line.clear();
        }

        // If we couldn't parse hashrate, log all output for debugging
        if hashrate_hashes_per_sec.is_none() {
            error!("Failed to parse hashrate from minerd benchmark output. Full output:");
            for (i, line) in all_output.iter().enumerate() {
                error!("  Line {}: {}", i + 1, line);
            }
        }

        hashrate_hashes_per_sec.ok_or(MinerdError::HashrateParseError)
    }
}

impl Drop for MinerdProcess {
    fn drop(&mut self) {
        // Trigger cancellation to signal all tasks to stop
        self.cancellation_token.cancel();

        match self.process.lock() {
            Ok(mut process_guard) => {
                if let Some(mut process) = process_guard.take() {
                    if let Err(e) = process.start_kill() {
                        error!("Error killing minerd process on drop: {}", e);
                    } else {
                        info!("minerd process killed on drop");
                    }
                }
            }
            Err(_) => {
                error!("Mutex poisoned in Drop implementation, cannot kill process cleanly");
            }
        }
    }
}

/// Parses hashrate from a minerd benchmark output line
/// Examples:
/// - "[2025-08-29 20:10:39] thread 0: 2097152 hashes, 1441 khash/s"
/// - "[2025-08-29 20:10:39] Total: 1441 khash/s"
fn parse_hashrate_from_benchmark_line(line: &str) -> Option<f64> {
    // Look for pattern: "X khash/s" or "X.Y khash/s"
    if let Some(pos) = line.find(" khash/s") {
        // Find the number before " khash/s"
        let before_khash = &line[..pos];
        if let Some(space_pos) = before_khash.rfind(' ') {
            let number_str = &before_khash[space_pos + 1..];
            if let Ok(hashrate) = number_str.parse::<f64>() {
                return Some(hashrate);
            }
        }
    }

    // Also try to parse "hash/s" patterns (without the 'k' prefix) - convert to khash/s
    if let Some(pos) = line.find(" hash/s") {
        let before_hash = &line[..pos];
        if let Some(space_pos) = before_hash.rfind(' ') {
            let number_str = &before_hash[space_pos + 1..];
            if let Ok(hashrate) = number_str.parse::<f64>() {
                // Convert hash/s to khash/s
                return Some(hashrate / 1000.0);
            }
        }
    }

    None
}

pub async fn start_minerd(
    upstream_address: SocketAddr,
    username: Option<String>,
    password: Option<String>,
    single_submit: bool,
) -> Result<(MinerdProcess, SocketAddr), MinerdError> {
    if username.is_none() && password.is_some() || username.is_some() && password.is_none() {
        return Err(MinerdError::InvalidConfiguration(
            "Username and password must be provided together".to_string(),
        ));
    }

    let mut minerd_process = MinerdProcess::new(upstream_address, single_submit).await?;
    let local_address = minerd_process.local_address();

    minerd_process.start_tcp_proxy().await?;
    minerd_process.spawn_minerd(username, password).await?;

    Ok((minerd_process, local_address))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddr};

    #[tokio::test]
    async fn test_measure_hashrate() {
        let minerd_process = MinerdProcess::new(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)), false)
            .await
            .unwrap();
        let hashrate = minerd_process.measure_hashrate().await.unwrap();
        println!("Hashrate: {} hashes/s", hashrate);
        assert!(hashrate > 0.0);
    }

    #[test]
    fn test_parse_hashrate_from_benchmark_line() {
        // Test the parsing logic with known good inputs
        assert_eq!(
            parse_hashrate_from_benchmark_line(
                "[2025-08-29 20:10:39] thread 0: 2097152 hashes, 1441 khash/s"
            ),
            Some(1441.0)
        );

        assert_eq!(
            parse_hashrate_from_benchmark_line("[2025-08-29 20:10:39] Total: 1441 khash/s"),
            Some(1441.0)
        );

        assert_eq!(
            parse_hashrate_from_benchmark_line(
                "[2025-08-29 20:10:39] thread 0: 2097152 hashes, 1441.5 khash/s"
            ),
            Some(1441.5)
        );

        // Test hash/s conversion
        assert_eq!(
            parse_hashrate_from_benchmark_line(
                "[2025-08-29 20:10:39] thread 0: 2097152 hashes, 1441000 hash/s"
            ),
            Some(1441.0)
        );

        // Test invalid lines
        assert_eq!(
            parse_hashrate_from_benchmark_line("random text without hashrate"),
            None
        );
    }
}
