//! Embedded Postgres fixture for local development.

#[cfg(feature = "dev")]
mod embedded {
    use std::path::PathBuf;
    use thiserror::Error;

    #[derive(Debug, Error)]
    pub enum Error {
        #[error("embedded postgres setup failed: {0}")]
        Setup(String),
        #[error("embedded postgres start failed: {0}")]
        Start(String),
    }

    pub struct EmbeddedPg {
        inner: postgresql_embedded::blocking::PostgreSQL,
    }

    impl EmbeddedPg {
        pub fn start() -> Result<Self, Error> {
            let cache = std::env::var("WYRD_DEV_PG_CACHE")
                .map(PathBuf::from)
                .unwrap_or_else(|_| dirs_cache_dir().join("wyrd").join("pg"));
            let settings = postgresql_embedded::Settings {
                version: postgresql_embedded::VersionReq::parse("=16")
                    .map_err(|e| Error::Setup(e.to_string()))?,
                installation_dir: cache,
                ..Default::default()
            };
            let mut pg = postgresql_embedded::blocking::PostgreSQL::new(settings);
            pg.setup().map_err(|e| Error::Setup(e.to_string()))?;
            pg.start().map_err(|e| Error::Start(e.to_string()))?;
            Ok(Self { inner: pg })
        }

        pub fn connection_url(&self) -> String {
            self.inner.settings().url("postgres")
        }
    }

    impl Drop for EmbeddedPg {
        fn drop(&mut self) {
            let _ = self.inner.stop();
        }
    }

    fn dirs_cache_dir() -> PathBuf {
        std::env::var("HOME")
            .map(|home| PathBuf::from(home).join(".cache"))
            .unwrap_or_else(|_| PathBuf::from("/tmp"))
    }
}

#[cfg(feature = "dev")]
pub use embedded::{EmbeddedPg, Error};
