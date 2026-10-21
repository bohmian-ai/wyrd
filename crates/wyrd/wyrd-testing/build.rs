//! Exposes the cargo profile directory this harness is built into, where
//! `cargo build -p wyrd-server` places the `wyrd-server` binary that every
//! in-process test server spawns as its Monty worker.

use std::env;
use std::path::Path;

/// Emit `WYRD_TESTING_PROFILE_DIR`: `OUT_DIR` is
/// `<profile dir>/build/<package>-<hash>/out`, so the profile directory is its
/// third ancestor.
///
/// # Panics
/// Panics when cargo provides no `OUT_DIR` or a shallower one than its
/// documented layout.
fn main() {
    let out = env::var_os("OUT_DIR").expect("cargo sets OUT_DIR for build scripts");
    let profile = Path::new(&out)
        .ancestors()
        .nth(3)
        .expect("OUT_DIR sits three levels under the profile directory");
    println!("cargo:rustc-env=WYRD_TESTING_PROFILE_DIR={}", profile.display());
    println!("cargo:rerun-if-changed=build.rs");
}
