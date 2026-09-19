//! Shared Wyrd integration-test harness.

pub mod bifrost;
pub mod interleaving;
pub mod keys;
pub mod load;
pub mod oidc_fixture;
pub mod principal;
pub mod server;
pub mod time;

pub use oidc_fixture::{DiscoveryFixture, KeycloakAdmin, LoginResult, OidcIssuerFixture};
pub use principal::{Bootstrap, CheckResult};
pub use server::{
    OracleRuntimeInspection, WyrdTestServer, WyrdTestServerBuilder, WyrdTestServerError,
    materialize_test_peer_config, server_postgres_from_fixture,
};
pub use time::ClockHandle;

#[cfg(feature = "python")]
pub mod python;
