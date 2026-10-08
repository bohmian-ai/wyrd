use std::process::ExitCode;

use clap::Args;
use reqwest::Method;
use url::Url;
use wyrd_client::WyrdClient;
use wyrd_spec::auth::{GrantRoleRequest, GrantRoleResponse};
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::CardRef;

use crate::auth::issue_key::card_ref;
use crate::error::WyrdCliError;

/// Arguments for `wyrd auth grant-role`.
///
/// `--version` names the target Card's version, so the auto-generated
/// `--version` flag is disabled as it is on `wyrd auth issue-key`.
#[derive(Debug, Args)]
#[command(disable_version_flag = true)]
pub struct GrantRoleArgs {
    /// Card kind, spelled as the contract spells it (`Service` or `Agent`).
    #[arg(long, value_name = "KIND")]
    pub kind: String,
    /// Card name.
    #[arg(long, value_name = "NAME")]
    pub name: String,
    /// Card version (e.g. 1.0.0).
    #[arg(long, value_name = "VERSION")]
    pub version: String,
    /// Card space.
    #[arg(long, value_name = "SPACE")]
    pub space: String,
    /// Built-in or tenant Role to grant (e.g. `workload`).
    #[arg(long, value_name = "ROLE")]
    pub role: String,
    /// Wyrd server base URL. The credential is read from the ambient chain
    /// (`WYRD_ACCESS_TOKEN`, workload identity, `WYRD_API_KEY`, or
    /// `credentials.toml`), never from an argument.
    #[arg(long, value_name = "URL", env = "WYRD_SERVER_URL")]
    pub server: Url,
}

/// Grant one Role to the principal bound to a Service or Agent Card.
///
/// The in-process form of `wyrd auth grant-role`. The request runs as
/// `client`, or, when it is omitted, as the ambient credential chain, and
/// requires tenant administration. Granting a Role already held succeeds with
/// `granted: false`. The Role reaches the principal's tokens at its next key
/// exchange.
///
/// # Arguments
/// - `kind`: Card kind, `Service` or `Agent`.
/// - `name`: Card name.
/// - `version`: Card version.
/// - `space`: Card space.
/// - `role`: built-in or tenant Role name.
/// - `client`: the administrator to act as; `None` resolves the ambient chain.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` when the coordinates do not form a
/// `CardRef` or the role is unknown, a `WYRD_CLIENT_*` error for a rejected
/// endpoint or missing credential, `WYRD_PERMISSION_403_DENIED_RBAC` when the
/// caller is not a tenant administrator, and `WYRD_AUTH_404_PRINCIPAL_NOT_FOUND`
/// when no principal is bound to the Card in the caller's tenant.
pub async fn grant_role(
    kind: &str,
    name: &str,
    version: &str,
    space: &str,
    role: &str,
    client: Option<WyrdClient>,
) -> Result<GrantRoleResponse, WyrdError> {
    let card_ref = card_ref(kind, name, version, space)?;
    let client = crate::client::explicit_or_ambient(client)?;
    request(client, card_ref, role)
        .await
        .map_err(WyrdError::from)
}

/// Send one grant-role request as `client`.
///
/// # Errors
/// Returns [`WyrdCliError::Server`] for the server's refusal.
async fn request(
    client: WyrdClient,
    card_ref: CardRef,
    role: &str,
) -> Result<GrantRoleResponse, WyrdCliError> {
    client
        .request_json(
            Method::POST,
            "/v1/auth/grant-role",
            Some(&GrantRoleRequest {
                card_ref,
                role: role.to_owned(),
            }),
        )
        .await
        .map_err(|source| WyrdCliError::Server { source })
}

/// Grant a Role to a Card-bound principal and print its resulting roles.
///
/// # Errors
/// Returns [`WyrdCliError::InvalidArgument`] when the card coordinates do not
/// form a `CardRef`, a client-construction error for a rejected endpoint, and
/// the server's stable Wyrd error when the caller is not a tenant
/// administrator, the role is unknown, or the Card binds no principal.
pub async fn dispatch(args: GrantRoleArgs) -> Result<ExitCode, WyrdCliError> {
    let card_ref = card_ref(&args.kind, &args.name, &args.version, &args.space)?;
    let client = crate::client::from_global(Some(args.server.as_str()))?;
    let response = request(client, card_ref, &args.role).await?;

    println!("principal_id: {}", response.principal_id);
    println!("card_ref:     {}", response.card_ref);
    println!("roles:        {}", response.roles.join(", "));
    println!("granted:      {}", response.granted);

    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::GrantRoleArgs;

    /// Wraps the grant-role arguments in a parser so clap's validation runs.
    #[derive(Debug, Parser)]
    struct Cli {
        /// The arguments under test.
        #[command(flatten)]
        args: GrantRoleArgs,
    }

    /// The command parses its Card coordinates and the role, and refuses an
    /// invocation without `--role`.
    ///
    /// # Panics
    /// Panics when the full invocation fails to parse or the role-less one
    /// parses.
    #[test]
    fn grant_role_requires_card_coordinates_and_role() {
        let mut argv = vec![
            "wyrd",
            "--kind",
            "Service",
            "--name",
            "my-svc",
            "--version",
            "1.0.0",
            "--space",
            "default",
            "--server",
            "https://acme.wyrd.cloud",
        ];
        assert!(Cli::try_parse_from(&argv).is_err(), "must require --role");
        argv.extend(["--role", "workload"]);
        let parsed = Cli::try_parse_from(&argv).expect("full invocation parses");
        assert_eq!(parsed.args.role, "workload");
    }
}
