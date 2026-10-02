//! `wyrd auth login`, `wyrd auth logout`, and `wyrd auth status`: the saved
//! human user login every local SDK resolves.
//!
//! Login begins a server-owned one-use handoff, sends the system browser to the
//! tenant's provider, and polls the handoff with the in-memory verifier until
//! the browser sign-in completes. The claimed Wyrd user credential goes
//! straight into the private saved-login store owned by `wyrd-client`; no token
//! is printed, logged, or placed in argv. Logout tombstones the record, revokes
//! that login's refresh chain on the server, and deletes the record even when
//! the server cannot be reached.

use std::process::{ExitCode, Stdio};
use std::time::Duration;

use clap::Args;
use url::Url;
use wyrd_client::auth::TokenExchange;
use wyrd_client::saved_login::{SavedLogin, SavedLogins, canonical_origin};
use wyrd_client::transport::HttpConfig;
use wyrd_spec::auth::{CliHandoff, CliHandoffClaim, CliHandoffProof, CliLogin};
use wyrd_spec::ids::TenantSlug;

use crate::client::map_client_error;
use crate::error::WyrdCliError;

/// Exit status after the person interrupts a login with Ctrl-C.
const INTERRUPTED: u8 = 130;

/// Arguments for `wyrd auth login`.
#[derive(Debug, Args)]
pub struct LoginArgs {
    /// Wyrd server base URL.
    #[arg(long, value_name = "URL", env = "WYRD_SERVER_URL")]
    pub server: Url,
    /// Route key (slug) of the tenant to sign in to.
    #[arg(long, value_name = "KEY", env = "WYRD_TENANT")]
    pub tenant: TenantSlug,
    /// Print the sign-in URL without opening a browser.
    #[arg(long)]
    pub no_browser: bool,
}

/// Arguments for `wyrd auth logout`.
#[derive(Debug, Args)]
pub struct LogoutArgs {
    /// Wyrd server base URL of the login to end.
    #[arg(long, value_name = "URL", env = "WYRD_SERVER_URL")]
    pub server: Url,
    /// Tenant route key or id of the login to end; required when the server
    /// has saved logins for several tenants.
    #[arg(long, value_name = "KEY_OR_ID", env = "WYRD_TENANT")]
    pub tenant: Option<String>,
}

/// One `wyrd auth login` against one server and tenant.
struct LoginFlow {
    /// The server's unauthenticated `/auth` surface.
    exchange: TokenExchange,
    /// Canonical origin the saved record is keyed by.
    origin: String,
    /// Tenant the login is for.
    tenant: TenantSlug,
    /// Where the claimed credential is saved.
    store: SavedLogins,
}

impl LoginFlow {
    /// Prepare a login against `args.server` for `args.tenant`.
    ///
    /// # Errors
    /// Returns a client-configuration error for an unusable server URL or
    /// when no Wyrd configuration directory can be resolved.
    fn new(args: &LoginArgs) -> Result<Self, WyrdCliError> {
        Ok(Self {
            exchange: TokenExchange::new(args.server.as_str(), HttpConfig::default().timeout_ms)
                .map_err(map_client_error)?,
            origin: canonical_origin(args.server.as_str()).map_err(map_client_error)?,
            tenant: args.tenant.clone(),
            store: saved_logins()?,
        })
    }

    /// Begin the handoff, send the person to the provider, wait for the
    /// browser sign-in, and save the claimed credential.
    ///
    /// Ctrl-C cancels the handoff on the server and exits `130` without
    /// saving anything.
    ///
    /// # Errors
    /// Returns the server's stable error when the tenant has no usable SSO
    /// login or the handoff is refused or expires, and a saved-login error
    /// when the credential cannot be stored.
    async fn run(self, open_browser: bool) -> Result<ExitCode, WyrdCliError> {
        let handoff = self
            .exchange
            .begin_cli_handoff(&self.tenant)
            .await
            .map_err(server_error)?;
        eprintln!(
            "Sign in to tenant {} in your browser:\n  {}",
            self.tenant,
            handoff.login_url.as_str()
        );
        if open_browser {
            open_in_browser(handoff.login_url.as_str());
        }
        let proof = CliHandoffProof {
            tenant_route_key: self.tenant.clone(),
            poll_verifier: handoff.poll_verifier.clone(),
        };
        let login = tokio::select! {
            login = self.poll(&handoff, &proof) => login?,
            _ = tokio::signal::ctrl_c() => {
                if let Err(error) = self.exchange.cancel_cli_handoff(handoff.handoff_id, &proof).await {
                    eprintln!("warning: could not cancel the login on the server: {}", error.into_wyrd());
                }
                eprintln!("Login cancelled.");
                return Ok(ExitCode::from(INTERRUPTED));
            }
        };
        self.save(login).await
    }

    /// Claim the handoff until the browser sign-in completes, waiting the
    /// server's retry interval between claims.
    ///
    /// # Errors
    /// Returns the server's stable refusal, including the one an expired
    /// handoff answers with.
    async fn poll(
        &self,
        handoff: &CliHandoff,
        proof: &CliHandoffProof,
    ) -> Result<CliLogin, WyrdCliError> {
        loop {
            match self
                .exchange
                .claim_cli_handoff(handoff.handoff_id, proof)
                .await
                .map_err(server_error)?
            {
                CliHandoffClaim::Complete(login) => return Ok(login),
                CliHandoffClaim::Pending {
                    retry_after_seconds,
                } => {
                    tokio::time::sleep(Duration::from_secs(u64::from(retry_after_seconds.max(1))))
                        .await;
                }
            }
        }
    }

    /// Save `login` under this flow's origin and tenant and print its
    /// token-free summary.
    ///
    /// # Errors
    /// Returns a saved-login error when the store is unsafe or the write
    /// fails.
    async fn save(self, login: CliLogin) -> Result<ExitCode, WyrdCliError> {
        let record = SavedLogin::from_cli_login(self.origin, self.tenant, login);
        let summary = record.summary();
        let store = self.store;
        blocking(move || store.save(record)).await?;
        println!(
            "Logged in to {} as {}.",
            summary.origin, summary.principal_id
        );
        print_summary(&summary);
        Ok(ExitCode::SUCCESS)
    }
}

/// Log in and save the Wyrd user credential.
///
/// # Errors
/// See [`LoginFlow::run`].
pub async fn login(args: LoginArgs) -> Result<ExitCode, WyrdCliError> {
    let open_browser = !args.no_browser;
    LoginFlow::new(&args)?.run(open_browser).await
}

/// End the saved login for `args.server` and the selected tenant.
///
/// The record is tombstoned first, so no concurrent SDK renewal can use it,
/// then its refresh chain is revoked and the record deleted. A failed
/// revocation is reported as a warning: the local record is still deleted.
///
/// # Errors
/// Returns a saved-login error when the selection is ambiguous or names no
/// saved tenant, the store is unsafe, or the record cannot be changed.
pub async fn logout(args: LogoutArgs) -> Result<ExitCode, WyrdCliError> {
    let origin = canonical_origin(args.server.as_str()).map_err(map_client_error)?;
    let store = saved_logins()?;
    let selected = {
        let (store, origin) = (store.clone(), origin.clone());
        blocking(move || store.select(&origin, args.tenant.as_deref())).await?
    };
    let Some(record) = selected else {
        println!("No saved login for {origin}.");
        return Ok(ExitCode::SUCCESS);
    };
    let tenant_id = record.tenant_id;
    let refresh_token = {
        let (store, origin) = (store.clone(), origin.clone());
        blocking(move || store.begin_logout(&origin, tenant_id)).await?
    };
    if let Some(refresh_token) = refresh_token {
        let revoked =
            match TokenExchange::new(args.server.as_str(), HttpConfig::default().timeout_ms) {
                Ok(exchange) => exchange
                    .revoke_refresh_token(&refresh_token)
                    .await
                    .map_err(wyrd_client::auth::AuthError::into_wyrd),
                Err(error) => Err(error.into()),
            };
        if let Err(error) = revoked {
            eprintln!(
                "warning: the server did not confirm revocation ({error}); the saved login was \
                 removed locally, but its refresh token stays valid on the server until it expires"
            );
        }
    }
    {
        let origin = origin.clone();
        blocking(move || store.finish_logout(&origin, tenant_id)).await?;
    }
    println!("Logged out of {origin} tenant {}.", record.tenant_key);
    Ok(ExitCode::SUCCESS)
}

/// Print every saved login without its tokens.
///
/// # Errors
/// Returns a saved-login error when the store is unsafe or corrupt.
pub async fn status() -> Result<ExitCode, WyrdCliError> {
    let store = saved_logins()?;
    let logins = blocking(move || store.list()).await?;
    if logins.is_empty() {
        println!("No saved logins. Run `wyrd auth login --server URL --tenant KEY`.");
    }
    for login in logins {
        print_summary(&login.summary());
    }
    Ok(ExitCode::SUCCESS)
}

/// The saved-login store under the user's Wyrd configuration directory.
///
/// # Errors
/// Returns a client-configuration error when no configuration directory can
/// be resolved.
fn saved_logins() -> Result<SavedLogins, WyrdCliError> {
    SavedLogins::locate().ok_or_else(|| WyrdCliError::ClientConfig {
        detail: "no Wyrd configuration directory: set WYRD_CONFIG_HOME or HOME".to_owned(),
    })
}

/// Run one saved-login store operation, which takes an OS file lock, off the
/// async runtime.
///
/// # Errors
/// Returns the operation's own error.
///
/// # Panics
/// Re-raises a panic from the operation.
async fn blocking<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T, wyrd_client::error::WyrdClientError> + Send + 'static,
) -> Result<T, WyrdCliError> {
    match tokio::task::spawn_blocking(operation).await {
        Ok(result) => result.map_err(map_client_error),
        Err(join) => std::panic::resume_unwind(join.into_panic()),
    }
}

/// Print one saved login's token-free summary.
fn print_summary(summary: &wyrd_client::saved_login::SavedLoginSummary) {
    println!("server:     {}", summary.origin);
    println!("tenant:     {} ({})", summary.tenant_key, summary.tenant_id);
    println!("principal:  {}", summary.principal_id);
    println!("status:     {}", summary.status);
    if let Some(expires_at) = summary.access_expires_at {
        println!("expires_at: {expires_at}");
    }
}

/// Map a `/auth` exchange failure to the CLI's server error.
fn server_error(error: wyrd_client::auth::AuthError) -> WyrdCliError {
    WyrdCliError::Server {
        source: error.into_wyrd(),
    }
}

/// Ask the platform to open `url` in the person's browser; the printed URL
/// remains the fallback, so a failure is only reported.
fn open_in_browser(url: &str) {
    let mut command = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(windows) {
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "start", ""]);
        command
    } else {
        std::process::Command::new("xdg-open")
    };
    let spawned = command
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    if spawned.is_err() {
        eprintln!("Could not open a browser; open the URL above yourself.");
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{LoginArgs, LogoutArgs};

    /// Bare wrapper so login arguments parse without the binary's tree.
    #[derive(Parser)]
    struct Login {
        /// The arguments under test.
        #[command(flatten)]
        args: LoginArgs,
    }

    /// Bare wrapper so logout arguments parse without the binary's tree.
    #[derive(Parser)]
    struct Logout {
        /// The arguments under test.
        #[command(flatten)]
        args: LogoutArgs,
    }

    /// Login needs a server and a valid tenant route key; logout's tenant is
    /// optional.
    #[test]
    fn login_requires_a_server_and_a_valid_tenant() {
        let parsed = Login::try_parse_from([
            "login",
            "--server",
            "https://wyrd.example.com",
            "--tenant",
            "acme",
            "--no-browser",
        ])
        .expect("parses");
        assert_eq!(parsed.args.tenant.as_str(), "acme");
        assert!(parsed.args.no_browser);
        assert!(
            Login::try_parse_from(["login", "--server", "https://x", "--tenant", "Not A Slug"])
                .is_err()
        );
        let logout = Logout::try_parse_from(["logout", "--server", "https://x"]).expect("parses");
        assert!(logout.args.tenant.is_none());
    }
}
