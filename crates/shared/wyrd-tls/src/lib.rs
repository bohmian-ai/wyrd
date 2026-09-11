//! Process-wide TLS crypto-provider ownership for Wyrd transports.

use std::sync::OnceLock;

/// Cached result of Wyrd's one permitted provider installation attempt.
static INSTALL_RESULT: OnceLock<Result<(), InstallError>> = OnceLock::new();

/// Installs Wyrd's AWS-LC Rustls provider exactly once.
///
/// Call this before constructing any Rustls-backed client, server, database,
/// or storage transport. Repeated calls return the original installation result.
///
/// # Errors
///
/// Returns [`InstallError`] when another provider already owns the process.
pub fn install_crypto_provider() -> Result<(), InstallError> {
    *INSTALL_RESULT.get_or_init(|| {
        rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .map_err(|_| InstallError)
    })
}

/// A conflicting Rustls provider was installed before Wyrd initialized TLS.
#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("a conflicting Rustls crypto provider is already installed")]
pub struct InstallError;

#[cfg(test)]
mod tests {
    use std::process::Command;

    /// Repeated initialization reuses the original successful provider result.
    #[test]
    fn provider_installation_is_idempotent() {
        if std::env::var_os("WYRD_TLS_CONFLICT_CHILD").is_some() {
            rustls::crypto::aws_lc_rs::default_provider()
                .install_default()
                .expect("isolated child preinstalls a provider outside Wyrd ownership");
            let error = super::install_crypto_provider()
                .expect_err("Wyrd provider installation must fail structurally");
            assert_eq!(
                error.to_string(),
                "a conflicting Rustls crypto provider is already installed"
            );
            return;
        }
        super::install_crypto_provider().expect("first AWS-LC installation succeeds");
        super::install_crypto_provider().expect("repeated AWS-LC installation succeeds");
    }

    /// A preinstalled conflicting provider produces an error instead of panicking.
    #[test]
    fn conflicting_provider_returns_structured_error_in_isolated_process() {
        let status = Command::new(std::env::current_exe().expect("current test executable"))
            .arg("provider_installation_is_idempotent")
            .arg("--exact")
            .arg("--nocapture")
            .env("WYRD_TLS_CONFLICT_CHILD", "1")
            .status()
            .expect("isolated provider-conflict process starts");
        assert!(
            status.success(),
            "isolated provider-conflict assertion failed"
        );
    }
}
