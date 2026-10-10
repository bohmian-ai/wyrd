//! `wyrd server`: obtain and manage the official Wyrd server for this host.
//!
//! The CLI owns only downloaded release artifacts here. Migration,
//! credentials, and durable Wyrd state stay with the server it installs.

mod install;

use std::process::ExitCode;

use clap::Subcommand;

pub use install::{InstalledServer, ServerInstaller};

use crate::error::WyrdCliError;

/// `wyrd server` subcommands.
#[derive(Debug, Subcommand)]
pub enum ServerCommand {
    /// Install, or update to, the newest stable official server release for
    /// this host after verifying its signature and digest.
    Install,
}

impl ServerCommand {
    /// Run the selected server subcommand.
    ///
    /// `install` resolves the official release service and signing key,
    /// installs the newest compatible stable release, and prints its version
    /// and location.
    ///
    /// # Errors
    /// Returns the installer's [`WyrdCliError`] when the host is unsupported,
    /// the release is unavailable, unverified, or incompatible, or local IO
    /// fails. A failure leaves any previous installation active.
    pub async fn dispatch(self) -> Result<ExitCode, WyrdCliError> {
        match self {
            Self::Install => {
                let installed = ServerInstaller::official()?.install().await?;
                println!(
                    "wyrd-server {} installed at {}",
                    installed.version,
                    installed.dir.display()
                );
                Ok(ExitCode::SUCCESS)
            }
        }
    }
}
