//! Emits the canonical server OpenAPI document to standard output.

use std::io::Write;

use utoipa::OpenApi;

/// Generates deterministic YAML for the checked-in OpenAPI artifact.
///
/// # Errors
///
/// Returns serialization or standard-output errors to the example runtime.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let yaml = wyrd_server::http::openapi::WyrdApiDoc::openapi().to_yaml()?;
    std::io::stdout().lock().write_all(yaml.as_bytes())?;
    Ok(())
}
