pub mod diff_management;
pub mod upstream;
pub mod upstream_connection;

#[derive(Clone, Copy, Debug)]
pub struct MiningConnection {
    _version: u16,
    // _setup_connection_flags: u32,
    // #[allow(dead_code)]
    // setup_connection_success_flags: u32,
}
