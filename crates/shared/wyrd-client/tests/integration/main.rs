//! `wyrd-client` integration target.
//!
//! Every integration test of this crate links into this one binary so the
//! crate's dependency cone is linked once. Each submodule owns one surface;
//! nextest selects a surface with `-E 'test(/^<module>::/)'`.

mod cards_transport;
mod pg_auth_e2e_against_fixture;
#[cfg(feature = "test-support")]
mod pg_bifrost_e2e;
mod startup_image_journey;
mod storage_dispatch;
mod transport;
mod workflow_transport;
