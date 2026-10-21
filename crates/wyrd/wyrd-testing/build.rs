//! Exposes the cargo profile directory where `cargo build -p wyrd-server`
//! places the `wyrd-server` binary that every in-process test server spawns
//! as its Monty worker.

use std::env;
use std::path::Path;

/// Emit `WYRD_TESTING_PROFILE_DIR`: `OUT_DIR` is
/// `<profile dir>/build/<package>-<hash>/out`, so the profile directory is its
/// third ancestor.
///
/// The worker is built for the host without `--target`, so it lands in
/// `<target dir>/<profile>`. A harness built with an explicit `--target`
/// naming the host (napi always passes one) sits under
/// `<target dir>/<triple>/<profile>` instead; that triple component is
/// dropped so both resolve to the same worker.
///
/// # Panics
/// Panics when cargo provides no `OUT_DIR`, `TARGET`, or `HOST`, or a
/// shallower `OUT_DIR` than its documented layout.
fn main() {
    let out = env::var_os("OUT_DIR").expect("cargo sets OUT_DIR for build scripts");
    let target = env::var("TARGET").expect("cargo sets TARGET for build scripts");
    let host = env::var("HOST").expect("cargo sets HOST for build scripts");
    let profile = Path::new(&out)
        .ancestors()
        .nth(3)
        .expect("OUT_DIR sits three levels under the profile directory");
    let triple_dir = profile.parent().filter(|parent| {
        target == host
            && parent
                .file_name()
                .is_some_and(|name| name == target.as_str())
    });
    let profile = match (triple_dir.and_then(Path::parent), profile.file_name()) {
        (Some(target_dir), Some(name)) => target_dir.join(name),
        _ => profile.to_path_buf(),
    };
    println!(
        "cargo:rustc-env=WYRD_TESTING_PROFILE_DIR={}",
        profile.display()
    );
    println!("cargo:rerun-if-changed=build.rs");
}
