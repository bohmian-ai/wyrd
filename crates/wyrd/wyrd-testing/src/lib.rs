//! Shared Wyrd integration-test harness.

pub mod bifrost;
pub mod capacity;
pub mod human_login;
pub mod gateway_capture;
pub mod interleaving;
pub mod keys;
pub mod load;
pub mod logs;
pub mod oidc_fixture;
pub mod principal;
pub mod profile;
pub mod release_server;
pub mod server;
pub mod time;
pub mod verification;

pub use oidc_fixture::{
    DiscoveryFixture, KeycloakAdmin, LoginResult, OidcIssuerFixture, provider_sign_in,
};
pub use principal::Bootstrap;
pub use server::{
    OracleRuntimeInspection, WyrdTestServer, WyrdTestServerBuilder, WyrdTestServerError,
    server_postgres_from_fixture,
};
pub use time::ClockHandle;
/// Isolated empty database for a test that runs a deployment's own migration,
/// projected to the Python SDK's testing module.
pub use wyrd_dev_fixtures::pg::UnmigratedDatabase;
