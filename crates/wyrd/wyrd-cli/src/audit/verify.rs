//! `wyrd audit verify` — POST /v1/admin/audit/verify for a tenant.
//!
//! Exits with code 0 on `clean` or `no_checkpoints`, exits non-zero (1) on
//! `tampered` or `gap`.

use std::process::ExitCode;

use clap::Args;

use crate::error::WyrdCliError;

/// Arguments for `wyrd audit verify`.
#[derive(Debug, Args)]
pub struct AuditVerifyArgs {
    /// Wyrd server base URL.
    #[arg(long, env = "WYRD_SERVER_URL")]
    pub server_url: String,

    /// Bearer access token for authentication.
    #[arg(long, env = "WYRD_ACCESS_TOKEN")]
    pub token: String,
}

/// Dispatch `wyrd audit verify`.
///
/// Exits 0 on clean or no_checkpoints; exits 1 on tampered or gap.
pub async fn dispatch(args: AuditVerifyArgs) -> Result<ExitCode, WyrdCliError> {
    let client = reqwest::Client::builder()
        .build()
        .map_err(|source| WyrdCliError::HttpBuild { source })?;

    let url = format!(
        "{}/v1/admin/audit/verify",
        args.server_url.trim_end_matches('/')
    );

    let resp = client
        .post(&url)
        .bearer_auth(&args.token)
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .map_err(|source| WyrdCliError::Http { source })?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|source| WyrdCliError::Http { source })?;

    if status.is_success() {
        // Parse outcome for user-friendly output.
        let outcome_str: String = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| {
                v.get("outcome")
                    .and_then(|o| o.as_str().map(ToOwned::to_owned))
            })
            .unwrap_or_else(|| "ok".to_owned());
        println!("audit verify: {outcome_str}");
        Ok(ExitCode::SUCCESS)
    } else if status.as_u16() == 409 {
        // Tampered or gap.
        eprintln!("audit verify FAILED: {status}\n{body}");
        Ok(ExitCode::FAILURE)
    } else {
        Err(WyrdCliError::AdminFailed {
            status: status.as_u16(),
            detail: body,
        })
    }
}
