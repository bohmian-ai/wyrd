//! Shared Wyrd integration-test harness.

pub mod bifrost;
pub mod interleaving;
pub mod keys;
pub mod load;
pub mod multipart_client;
pub mod oidc_fixture;
pub mod otlp;
pub mod principal;
pub mod server;
pub mod time;

pub use multipart_client::{MultipartClient, MultipartClientError};
pub use oidc_fixture::{KeycloakAdmin, LoginResult, OidcIssuerFixture};
pub use otlp::RandomTraceGenerator;
pub use principal::{Bootstrap, CheckResult};
pub use server::{
    OracleRuntimeInspection, WyrdTestServer, WyrdTestServerBuilder, WyrdTestServerError,
    server_postgres_from_fixture,
};
pub use time::ClockHandle;

#[cfg(feature = "python")]
pub mod python;
