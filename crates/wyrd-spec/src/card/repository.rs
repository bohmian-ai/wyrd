//! Repository Card spec: a connected source repository and its review policy.
//!
//! A Repository names one provider repository by its stable identity, the
//! configured `host` plus the provider's positive `repository_id`, and lists
//! the Verifiers that review it in `verified_by`. The list is the complete
//! desired policy: an empty list selects no automatic reviews. The display
//! name, access state and the selected version are server-managed
//! projections, never authored.
//!
//! `wyrd declare wyrd.toml` infers a Repository from the checkout's origin
//! remote ([`RepositoryRemote`]); the server verifies that hint against the
//! tenant's GitHub connection and fills `host` and `repository_id` before
//! the Card is registered, so a registered Repository always carries both.

use serde::{Deserialize, Serialize};

use crate::card::verifier::VerificationBinding;
use crate::ids::{CardName, RepositoryId};

/// Source-hosting provider of a Repository.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum RepositoryProvider {
    /// GitHub, through the deployment's GitHub App.
    #[default]
    Github,
}

/// A connected provider repository and the Verifiers bound to it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct RepositorySpec {
    /// Source-hosting provider; `github` when omitted.
    #[serde(default)]
    pub provider: RepositoryProvider,
    /// Canonical configured provider web host, such as `github.com`.
    ///
    /// May be omitted when authoring; the server sets the configured host
    /// and refuses any other value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// Stable provider repository id, the Repository's durable identity.
    ///
    /// May be omitted when the declaration carries a repository hint, from
    /// which the server resolves it through the tenant's connection; a
    /// registered Repository always carries it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_id: Option<RepositoryId>,
    /// The complete desired list of Verifier bindings. Each binding's
    /// `runs_on`, when present, must be the `pull_request` activation.
    #[serde(default)]
    pub verified_by: Vec<VerificationBinding>,
}

/// Repository identity read from a Git remote URL: a hint, never authority.
///
/// The server resolves `full_name` to the stable repository id through the
/// tenant's connection, so a renamed or forged remote cannot select another
/// tenant's or an unconnected repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct RepositoryRemote {
    /// Lowercase remote host without user information; a web remote keeps a
    /// non-default port, as the configured provider host does.
    pub host: String,
    /// `owner/name` path of the remote, without a `.git` suffix.
    pub full_name: String,
}

impl RepositoryRemote {
    /// Parse a Git remote URL in the `https://`, `http://`, `ssh://`,
    /// `git://` or scp-like `git@host:owner/name` forms.
    ///
    /// Returns `None` when the URL has no host or its path is not exactly
    /// `owner/name` (with an optional `.git` suffix and trailing slash).
    #[must_use]
    pub fn parse(url: &str) -> Option<Self> {
        let url = url.trim();
        let (scheme, (authority, path)) = if let Some((scheme, rest)) = url.split_once("://") {
            (scheme, rest.split_once('/')?)
        } else {
            // scp-like `user@host:path`; a Windows drive or local path has no `@`.
            let (authority, path) = url.split_once(':')?;
            if !authority.contains('@') {
                return None;
            }
            ("ssh", (authority, path))
        };
        let host = authority.rsplit('@').next()?.to_ascii_lowercase();
        let host = match (scheme, host.split_once(':')) {
            ("https", Some((name, "443"))) | ("http", Some((name, "80"))) => name.to_owned(),
            ("https" | "http", _) => host,
            (_, Some((name, _))) => name.to_owned(),
            (_, None) => host,
        };
        let path = path.trim_matches('/');
        let path = path.strip_suffix(".git").unwrap_or(path);
        let (owner, name) = path.split_once('/')?;
        let valid = |part: &str| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        };
        (!host.is_empty() && valid(owner) && valid(name)).then(|| Self {
            host,
            full_name: format!("{owner}/{name}"),
        })
    }
}

/// Infer a Repository Card name from a provider `owner/name`.
///
/// The repository part is lowercased, every character outside
/// `[a-z0-9_-]` becomes `-`, and leading characters before the first letter
/// are dropped. Returns `None` when the result is not a valid [`CardName`],
/// so the author must set `[repository.metadata] name` instead.
#[must_use]
pub fn infer_card_name(full_name: &str) -> Option<CardName> {
    let name = full_name.rsplit('/').next()?;
    let normalized: String = name
        .chars()
        .map(|ch| match ch.to_ascii_lowercase() {
            ch @ ('a'..='z' | '0'..='9' | '_' | '-') => ch,
            _ => '-',
        })
        .skip_while(|ch| !ch.is_ascii_lowercase())
        .collect();
    CardName::new(normalized).ok()
}

#[cfg(test)]
mod tests {
    use super::{RepositoryRemote, RepositorySpec, infer_card_name};

    /// Every supported remote form yields the same host and `owner/name`.
    ///
    /// # Panics
    /// Panics when a supported form fails to parse or parses differently.
    #[test]
    fn remote_forms_parse_to_one_identity() {
        for url in [
            "https://github.com/Acme/Widget.git",
            "https://github.com/Acme/Widget",
            "https://token@github.com/Acme/Widget/",
            "ssh://git@github.com:22/Acme/Widget.git",
            "git@github.com:Acme/Widget.git",
            "git://GitHub.com/Acme/Widget.git",
            "https://github.com:443/Acme/Widget.git",
        ] {
            assert_eq!(
                RepositoryRemote::parse(url),
                Some(RepositoryRemote {
                    host: "github.com".to_owned(),
                    full_name: "Acme/Widget".to_owned(),
                }),
                "{url}"
            );
        }
    }

    /// A web remote keeps a non-default port, which is part of the
    /// configured provider host; a transport port is not.
    ///
    /// # Panics
    /// Panics when the port is dropped from a web remote or kept elsewhere.
    #[test]
    fn web_remote_keeps_its_port() {
        let host = |url| RepositoryRemote::parse(url).map(|remote| remote.host);
        assert_eq!(
            host("https://git.example:8443/acme/widget"),
            Some("git.example:8443".to_owned())
        );
        assert_eq!(
            host("ssh://git@git.example:2222/acme/widget"),
            Some("git.example".to_owned())
        );
    }

    /// Local paths and URLs without exactly `owner/name` are not remotes.
    ///
    /// # Panics
    /// Panics when an invalid remote parses.
    #[test]
    fn non_repository_remotes_are_refused() {
        for url in [
            "/srv/git/widget.git",
            "C:/repos/widget",
            "https://github.com/acme",
            "https://github.com/acme/widget/tree/main",
            "git@github.com:",
        ] {
            assert_eq!(RepositoryRemote::parse(url), None, "{url}");
        }
    }

    /// Names normalize to the Card name grammar or are refused.
    ///
    /// # Panics
    /// Panics when a name infers incorrectly.
    #[test]
    fn card_names_infer_from_the_repository_part() {
        let infer = |full_name| infer_card_name(full_name).map(|name| name.to_string());
        assert_eq!(
            infer("acme/Widget.Service").as_deref(),
            Some("widget-service")
        );
        assert_eq!(infer("acme/2fa_tool").as_deref(), Some("fa_tool"));
        assert_eq!(infer("acme/go"), None);
    }

    /// The spec is closed and defaults provider and an empty binding list.
    ///
    /// # Panics
    /// Panics when the closed shape or its defaults change.
    #[test]
    fn spec_is_closed_with_github_default() {
        let spec: RepositorySpec =
            serde_json::from_value(serde_json::json!({"repository_id": "42"})).unwrap();
        assert_eq!(
            serde_json::to_value(&spec).unwrap(),
            serde_json::json!({"provider": "github", "repository_id": "42", "verified_by": []})
        );
        for refused in [
            serde_json::json!({"repository_id": "042"}),
            serde_json::json!({"repository_id": "0"}),
            serde_json::json!({"provider": "gitlab"}),
            serde_json::json!({"full_name": "acme/widget"}),
        ] {
            assert!(
                serde_json::from_value::<RepositorySpec>(refused.clone()).is_err(),
                "{refused}"
            );
        }
    }
}
