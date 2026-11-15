//! The benchmark recorder and heap sampler of a `bench` build of this addon,
//! for the L1 Verification benchmark's TypeScript driver only.
//!
//! The whole module, its global allocator, and the recorder it installs
//! exist only with the `bench` feature, which no published addon enables: a
//! production addon has none of these functions, no recorder, and the system
//! allocator. A `bench` build records the shared `wyrd_client` timings and
//! this addon's own `sdk.verify` boundary into the one `wyrd_benchmark`
//! registry, and reports every native allocation of this addon to the shared
//! heap sampler, which ignores them until started.

use napi::bindgen_prelude::Buffer;
use napi_derive::napi;

/// This addon's allocator: the system allocator reporting each native
/// allocation to [`wyrd_benchmark::heap`].
#[global_allocator]
static GLOBAL: wyrd_benchmark::heap::SampledSystem = wyrd_benchmark::heap::SampledSystem;

/// Installs the benchmark recorder as this process's `metrics` recorder.
///
/// # Errors
///
/// Returns a napi error when another `metrics` recorder is installed.
#[napi]
pub fn benchmark_install() -> napi::Result<()> {
    wyrd_benchmark::install().map_err(|error| napi::Error::from_reason(error.to_string()))
}

/// The cumulative benchmark snapshot of this process, as JSON.
///
/// # Errors
///
/// Returns a napi error when the snapshot cannot be serialized.
#[napi]
pub fn benchmark_snapshot() -> napi::Result<String> {
    serde_json::to_string(&wyrd_benchmark::snapshot())
        .map_err(|error| napi::Error::from_reason(error.to_string()))
}

/// Starts sampling this addon's native allocations.
#[napi]
pub fn benchmark_heap_start() {
    wyrd_benchmark::heap::start();
}

/// The sampled native allocations since [`benchmark_heap_start`], allocated
/// and still live, as a gzipped pprof profile.
#[napi]
pub fn benchmark_heap_profile() -> Buffer {
    wyrd_benchmark::heap::profile().into()
}
