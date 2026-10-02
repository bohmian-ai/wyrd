//! `wyrd auth login`, `wyrd auth logout`, and `wyrd auth status`: the saved
//! human user login every local SDK resolves.
//!
//! Login uses the RFC 8628 device authorization grant, as `gh` and
//! `aws sso login` do: it prints a one-time user code, opens the server's
//! verification page, where the person approves the code and signs in to the
//! tenant's provider, and polls the token endpoint with the in-memory device
//! code until the sign-in completes. The Wyrd user credential goes straight
//! into the private saved-login store owned by `wyrd-client`; no token is
//! printed, logged, or placed in argv. Logout deletes the record, then revokes
//! that login's refresh chain on the server best-effort and warns when it
//! cannot.

use std::process::{ExitCode, Stdio};
use std::time::Duration;

use clap::Args;
use url::Url;
use wyrd_client::auth::{AuthError, TokenExchange};
use wyrd_client::saved_login::{SavedLogin, SavedLogins, canonical_origin};
use wyrd_client::transport::HttpConfig;
use wyrd_spec::auth::{DeviceAuthorization, TokenRequest, TokenResponse};
use wyrd_spec::error::WyrdError;
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
    /// Print the verification URL without opening a browser.
    #[arg(long)]
    pub no_browser: bool,
}

/// Arguments for `wyrd auth logout`.
#[derive(Debug, Args)]
pub struct LogoutArgs {
    /// Wyrd server base URL of the login to end.
    #[arg(long, value_name = "URL", env = "WYRD_SERVER_URL")]
    pub server: Url,
    /// Tenant route key of the login to end; defaults to the most recent
    /// login for the server.
    #[arg(long, value_name = "KEY", env = "WYRD_TENANT")]
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
    /// Where the credential is saved.
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

    /// Authorize a device code, send the person to the verification page,
    /// wait for the sign-in, and save the credential.
    ///
    /// Ctrl-C exits `130` without saving anything; the unredeemed device code
    /// expires on the server.
    ///
    /// # Errors
    /// Returns the server's stable error when the tenant has no usable SSO
    /// login or the device code is denied or expires, and a saved-login error
    /// when the credential cannot be stored.
    async fn run(self, open_browser: bool) -> Result<ExitCode, WyrdCliError> {
        let device = self
            .exchange
            .device_authorization(&self.tenant)
            .await
            .map_err(server_error)?;
        eprintln!(
            "First copy your one-time code: {}\nThen approve it and sign in to tenant {} at:\n  {}",
            device.user_code,
            self.tenant,
            device.verification_uri_complete.as_str()
        );
        if open_browser {
            open_in_browser(device.verification_uri_complete.as_str());
        }
        let token = tokio::select! {
            token = self.poll(device) => token?,
            _ = tokio::signal::ctrl_c() => {
                eprintln!("Login cancelled.");
                return Ok(ExitCode::from(INTERRUPTED));
            }
        };
        self.save(token).await
    }

    /// Poll the token endpoint with the device code until the sign-in
    /// completes (RFC 8628 §3.5): wait the interval after
    /// `authorization_pending` and five seconds longer from each
    /// `slow_down` on.
    ///
    /// # Errors
    /// Returns the server's stable refusal for every other answer, including
    /// `access_denied` and `expired_token`.
    async fn poll(&self, device: DeviceAuthorization) -> Result<TokenResponse, WyrdCliError> {
        let mut interval = device.interval.max(1);
        let request = TokenRequest::DeviceCode {
            device_code: device.device_code,
        };
        loop {
            tokio::time::sleep(Duration::from_secs(interval)).await;
            match self.exchange.exchange(&request).await {
                Ok(token) => return Ok(token),
                Err(AuthError::Server(WyrdError::DeviceAuthorization { details, .. }))
                    if details["error"] == "authorization_pending" => {}
                Err(AuthError::Server(WyrdError::DeviceAuthorization { details, .. }))
                    if details["error"] == "slow_down" =>
                {
                    interval += 5;
                }
                Err(error) => return Err(server_error(error)),
            }
        }
    }

    /// Save `token` under this flow's origin and tenant and print its
    /// token-free summary.
    ///
    /// # Errors
    /// Returns a saved-login error when the server issued no refresh token,
    /// the store is unsafe, or the write fails.
    async fn save(self, token: TokenResponse) -> Result<ExitCode, WyrdCliError> {
        let record =
            SavedLogin::from_token(self.origin, self.tenant, token).map_err(map_client_error)?;
        let summary = record.summary();
        let store = self.store;
        blocking(move || store.save(record)).await?;
        println!(
            "Logged in to {} tenant {}.",
            summary.origin, summary.tenant_key
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

/// End the saved login for `args.server` and the selected tenant, or the
/// most recent one for the server.
///
/// The record is deleted first, then its refresh chain is revoked on the
/// server; a failed revocation is reported as a warning.
///
/// # Errors
/// Returns a saved-login error when the selector names no saved tenant, the
/// store is unsafe, or the record cannot be deleted.
pub async fn logout(args: LogoutArgs) -> Result<ExitCode, WyrdCliError> {
    let origin = canonical_origin(args.server.as_str()).map_err(map_client_error)?;
    let store = saved_logins()?;
    let removed = {
        let origin = origin.clone();
        blocking(move || {
            let Some(selected) = store.select(&origin, args.tenant.as_deref())? else {
                return Ok(None);
            };
            store.remove(&origin, &selected.tenant_key)
        })
        .await?
    };
    let Some(record) = removed else {
        println!("No saved login for {origin}.");
        return Ok(ExitCode::SUCCESS);
    };
    let revoked = match TokenExchange::new(args.server.as_str(), HttpConfig::default().timeout_ms) {
        Ok(exchange) => exchange
            .revoke_refresh_token(&record.refresh_token)
            .await
            .map_err(AuthError::into_wyrd),
        Err(error) => Err(error.into()),
    };
    if let Err(error) = revoked {
        eprintln!(
            "warning: the server did not confirm revocation ({error}); the saved login was \
             removed locally, but its refresh token stays valid on the server until it expires"
        );
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
    println!("tenant:     {}", summary.tenant_key);
    println!("expires_at: {}", summary.access_expires_at);
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
