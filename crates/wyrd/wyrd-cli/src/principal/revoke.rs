use std::process::ExitCode;

use clap::Args;
use url::Url;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag, RevokePrincipalRequest};

use crate::error::WyrdCliError;

/// CLI mirror of [`PrincipalKindTag`].
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum PrincipalKindCli {
    User,
    Service,
    Agent,
}

impl From<PrincipalKindCli> for PrincipalKindTag {
    fn from(value: PrincipalKindCli) -> Self {
        match value {
            PrincipalKindCli::User => Self::User,
            PrincipalKindCli::Service => Self::Service,
            PrincipalKindCli::Agent => Self::Agent,
        }
    }
}

#[derive(Debug, Args)]
pub struct RevokeArgs {
    /// Principal ID (UUID) to revoke.
    pub id: String,
    /// Principal kind (required — IDs are unique only within a kind table).
    #[arg(long, value_name = "KIND")]
    pub kind: PrincipalKindCli,
    /// Audit reason for the revocation.
    #[arg(long, value_name = "TEXT")]
    pub reason: String,
    /// Wyrd server base URL. The credential is read from the ambient chain
    /// (`WYRD_ACCESS_TOKEN`, workload identity, `WYRD_API_KEY`, or
    /// `credentials.toml`), never from an argument.
    #[arg(long, value_name = "URL", env = "WYRD_SERVER_URL")]
    pub server: Url,
}

/// Suspend one principal so every issuance path refuses its next token.
///
/// A token it already holds is a five-minute permission snapshot and lapses
/// at expiry; nothing is checked per request.
///
/// # Errors
/// Returns [`WyrdCliError::InvalidArgument`] when the id is not a principal
/// UUID, a client-construction error for a rejected endpoint, and the server's
/// stable Wyrd error when the caller is unauthorized or the principal is
/// unknown in the caller's tenant.
pub async fn dispatch(args: RevokeArgs) -> Result<ExitCode, WyrdCliError> {
    let principal_id: PrincipalId = args.id.parse().map_err(|_| WyrdCliError::InvalidArgument {
        field: "id".to_owned(),
        value: args.id.clone(),
        expected: "a principal UUID".to_owned(),
    })?;

    wyrd_client::Principals::with_client(crate::client::from_global(Some(args.server.as_str()))?)
        .revoke_principal(
            &principal_id,
            &RevokePrincipalRequest {
                principal_kind: PrincipalKindTag::from(args.kind),
                reason: args.reason,
            },
        )
        .await
        .map_err(|source| WyrdCliError::Server { source })?;

    println!("revoked: {principal_id}");
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::RevokeArgs;

    #[derive(Debug, Parser)]
    struct Cli {
        #[command(flatten)]
        args: RevokeArgs,
    }

    #[test]
    fn revoke_requires_kind() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "p-id-123",
            "--reason",
            "test",
            "--server",
            "https://acme.wyrd.cloud",
        ]);
        assert!(parsed.is_err(), "revoke must require --kind");
    }

    #[test]
    fn revoke_parses_all_kinds() {
        for kind in ["user", "service", "agent"] {
            let parsed = Cli::try_parse_from([
                "wyrd",
                "p-id-123",
                "--kind",
                kind,
                "--reason",
                "test",
                "--server",
                "https://acme.wyrd.cloud",
            ]);
            assert!(parsed.is_ok(), "kind={kind} failed: {parsed:?}");
        }
    }
}
