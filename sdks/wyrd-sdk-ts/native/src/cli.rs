//! Thin napi projection of the installed `wyrd` executable.
//!
//! The in-process command functions are test-only and live in the
//! `@wyrd/testing` harness addon.

use napi::Env;
use napi::bindgen_prelude::PromiseRaw;
use napi_derive::napi;

/// Runs the `wyrd` executable over `args` (argv including the program name)
/// and resolves its process exit code.
#[napi]
pub fn run_wyrd_cli(env: &Env, args: Vec<String>) -> napi::Result<PromiseRaw<'_, u32>> {
    crate::trace::spawn(env, async move {
        Ok(u32::from(wyrd_cli::run_cli_code(args).await))
    })
}
