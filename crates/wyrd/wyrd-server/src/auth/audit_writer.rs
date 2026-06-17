//! Authz-check audit writer seam.

/// Audit writer used by the authz-check route.
///
/// This stage only needs the trait object in application state. The durable
/// PostgreSQL writer and write method land with the authz-check route stage.
pub trait AuthzAuditWriter: Send + Sync {
    /// Whether this writer is a placeholder default unsuitable for production.
    fn is_stub_default(&self) -> bool {
        false
    }
}

/// Placeholder authz-check audit writer.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopAuthzAuditWriter;

impl AuthzAuditWriter for NoopAuthzAuditWriter {
    fn is_stub_default(&self) -> bool {
        true
    }
}
