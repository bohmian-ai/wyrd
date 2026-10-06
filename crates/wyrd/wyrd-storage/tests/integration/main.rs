//! `wyrd-storage` integration target.
//!
//! Every integration test of this crate links into this one binary so the
//! crate's dependency cone is linked once. Each submodule owns one surface;
//! nextest selects a surface with `-E 'test(/^<module>::/)'`.

#[cfg(feature = "emulator")]
mod handle_crud;
#[cfg(feature = "emulator")]
mod integration_azurite;
#[cfg(feature = "emulator")]
mod integration_gcs;
#[cfg(feature = "emulator")]
mod integration_rustfs;
mod pg_sweeper;
