//! Configures napi symbol export generation for the test-tier Node addon.

/// Emits the platform-specific napi build configuration.
fn main() {
    napi_build::setup();
}
