//! Configures napi symbol export generation for the Node binding.

/// Emits the platform-specific napi build configuration.
fn main() {
    napi_build::setup();
}
