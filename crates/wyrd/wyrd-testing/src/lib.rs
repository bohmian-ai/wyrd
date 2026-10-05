//! Shared Wyrd integration-test harness.

pub mod bifrost;
pub mod human_login;
pub mod interleaving;
pub mod keys;
pub mod load;
pub mod oidc_fixture;
pub mod principal;
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
