//! # rust-bitcoincore-rpc integration test
//!
//! The test methods are named to mention the methods tested.
//! Individual test methods don't use any methods not tested before or
//! mentioned in the test method name.
//!
//! The goal of this test is not to test the correctness of the server, but
//! to test the serialization of arguments and deserialization of responses.
//!

#![deny(unused)]
#![allow(deprecated)]

#[macro_use]
extern crate lazy_static;

use std::cell::RefCell;
use std::time::Duration;
use std::{fs, mem, panic};

use backtrace::Backtrace;

use jsonrpc::http::minreq_http;
use jsonrpc::{Client, Request};

struct StdLogger;

impl log::Log for StdLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.target().contains("jsonrpc") || metadata.target().contains("bitcoincore_rpc")
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            println!("[{}][{}]: {}", record.level(), record.metadata().target(), record.args());
        }
    }

    fn flush(&self) {}
}

static LOGGER: StdLogger = StdLogger;

fn get_rpc_url() -> String {
    return std::env::var("RPC_URL").expect("RPC_URL must be set");
}

fn get_auth() -> (String, Option<String>) {
    if let Ok(cookie) = std::env::var("RPC_COOKIE") {
        let contents =
            fs::read_to_string(&cookie).expect(&format!("failed to read cookie file: {}", cookie));
        let mut split = contents.split(':');
        let user = split.next().expect("failed to get username from cookie file");
        let pass = split.next().map_or("".to_string(), |s| s.to_string());
        return (user.to_string(), Some(pass));
    } else if let Ok(user) = std::env::var("RPC_USER") {
        return (user, std::env::var("RPC_PASS").ok());
    } else {
        panic!("Either RPC_COOKIE or RPC_USER + RPC_PASS must be set.");
    };
}

fn make_client() -> Client {
    let (user, pass) = get_auth();
    let tp = minreq_http::Builder::new()
        .timeout(Duration::from_secs(1))
        .url(&get_rpc_url())
        .unwrap()
        .basic_auth(user, pass)
        .build();
    Client::with_transport(tp)
}

lazy_static! {
    static ref CLIENT: Client = make_client();
}

thread_local! {
    static LAST_PANIC: RefCell<Option<(String, Backtrace)>> = RefCell::new(None);
}

/// Here we will collect all the results of the individual tests, preserving ordering.
/// Ideally this would be preset with capacity, but static prevents this.
static mut RESULTS: Vec<(&'static str, bool)> = Vec::new();

macro_rules! run_test {
    ($method:ident) => {
        println!("Running {}...", stringify!($method));
        let result = panic::catch_unwind(|| {
            $method(&*CLIENT);
        });
        if result.is_err() {
            let (msg, bt) = LAST_PANIC.with(|b| b.borrow_mut().take()).unwrap();
            println!("{}", msg);
            println!("{:?}", bt);
            println!("--");
        }

        unsafe {
            RESULTS.push((stringify!($method), result.is_ok()));
        }
    };
}

fn main() {
    log::set_logger(&LOGGER).map(|()| log::set_max_level(log::LevelFilter::max())).unwrap();

    // let default_hook = std::panic::take_hook()
    std::panic::set_hook(Box::new(|panic_info| {
        let bt = Backtrace::new();
        LAST_PANIC.with(move |b| b.borrow_mut().replace((panic_info.to_string(), bt)));
    }));

    run_test!(test_get_network_info);

    // Print results
    println!("");
    println!("");
    println!("Summary:");
    let mut error_count = 0;
    for (name, success) in mem::replace(unsafe { &mut RESULTS }, Vec::new()).into_iter() {
        if !success {
            println!(" - {}: FAILED", name);
            error_count += 1;
        } else {
            println!(" - {}: PASSED", name);
        }
    }

    println!("");

    if error_count == 0 {
        println!("All tests succesful!");
    } else {
        println!("{} tests failed", error_count);
        std::process::exit(1);
    }
}

fn test_get_network_info(cl: &Client) {
    let request = Request {
        method: "getnetworkinfo".into(),
        params: &[],
        id: serde_json::json!(1),
        jsonrpc: Some("2.0"),
    };

    let _ = cl.send_request(request).unwrap();
}
