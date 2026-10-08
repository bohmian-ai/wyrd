//! Thin napi projection of the installed `wyrd` executable.
//!
//! The in-process command functions are test-only and live in the
//! `@wyrd/testing` harness addon.

use napi_derive::napi;

/// Runs the `wyrd` executable over `args` (argv including the program name)
/// and resolves its process exit code.
#[napi]
pub async fn run_wyrd_cli(args: Vec<String>) -> u32 {
    u32::from(wyrd_cli::run_cli_code(args).await)
}
