//! Test-only client wiring shared by client-tier crates.

use std::sync::Arc;

use crate::auth::AuthMiddleware;
use crate::config::ClientConfig;
use crate::error::WyrdClientError;
use crate::transport::credential::ResolvedCredential;

/// Build a bearer-token middleware for isolated client tests.
pub fn test_auth_middleware() -> Result<Arc<AuthMiddleware>, WyrdClientError> {
    AuthMiddleware::new(
        &ClientConfig::default(),
        ResolvedCredential::BearerToken("test-bearer".to_owned().into()),
    )
}
