//! Support binary for the Tier-2 Bifrost peer-network harness.
//!
//! One simulated pod per process. The implementation lives in the library so
//! the control-protocol types are shared with the parent and so composing a
//! server can reach crate-internal builder seats; this root only hands control
//! to it.

fn main() -> std::process::ExitCode {
    wyrd_testing::bifrost::process_cluster::run_peer_test_node()
}
