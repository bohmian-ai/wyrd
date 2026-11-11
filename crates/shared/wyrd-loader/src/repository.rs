//! `wyrd declare wyrd.toml`: the inferred Repository root.
//!
//! The `[repository]` table becomes one ordinary Repository Card whose
//! `verified_by` holds one binding per listed Verifier, so the rest of the
//! pipeline resolves only those Verifiers and their transitive graph. The
//! checkout's origin remote, read from the local Git config without running
//! Git or touching the network, is only a hint the server verifies.

use std::path::Path;

use wyrd_config::RepositoryConfig;
use wyrd_spec::api_version::ApiVersion;
use wyrd_spec::card::repository::{
    RepositoryProvider, RepositoryRemote, RepositorySpec, infer_card_name,
};
use wyrd_spec::card::verifier::VerificationBinding;
use wyrd_spec::envelope::{CardKind, Metadata, Spec};
use wyrd_spec::error::WyrdError;
use wyrd_spec::registry::RepositoryHint;

use crate::diagnose::{Diagnostic, Severity};
use crate::parse::AuthoredCard;

/// Build the Repository root authored by `[repository]` in `toml_path`.
///
/// `remote` is the explicit `--repository` URL; without it the origin
/// remote of the checkout containing `toml_path` is the hint. An explicit
/// `[repository.metadata] name` wins over inference; otherwise the local
/// name is inferred from the remote and the server may replace it with the
/// established family name.
///
/// # Errors
/// Returns a diagnostic when `remote` is not a Git remote URL, when neither
/// a remote nor `[repository.spec] repository_id` identifies the
/// repository, or when no name is set and none can be inferred.
pub(crate) fn repository_root(
    toml_path: &Path,
    repository: &RepositoryConfig,
    remote: Option<&str>,
) -> Result<(AuthoredCard, RepositoryHint), Diagnostic> {
    let path = toml_path.to_path_buf();
    let remote = match remote {
        Some(url) => Some(RepositoryRemote::parse(url).ok_or_else(|| {
            Diagnostic::invalid_envelope(
                path.clone(),
                format!("--repository `{url}` is not a Git remote URL of the form https://host/owner/name"),
            )
        })?),
        None => toml_path.parent().and_then(origin_remote),
    };
    if remote.is_none() && repository.spec.repository_id.is_none() {
        return Err(Diagnostic::from_wyrd_error(
            path,
            Severity::Error,
            None,
            &WyrdError::RegistryRepositoryIdentityMissing {
                message: "wyrd.toml declares a Repository but no origin remote, --repository or [repository.spec] repository_id identifies it".to_owned(),
                details: serde_json::Value::Null,
            },
        ));
    }
    let name = match (&repository.metadata.name, &remote) {
        (Some(name), _) => name.clone(),
        (None, Some(remote)) => infer_card_name(&remote.full_name).ok_or_else(|| {
            Diagnostic::invalid_envelope(
                path.clone(),
                format!(
                    "cannot infer a Card name from `{}`; set [repository.metadata] name",
                    remote.full_name
                ),
            )
        })?,
        (None, None) => {
            return Err(Diagnostic::invalid_envelope(
                path,
                "set [repository.metadata] name when no remote identifies the repository"
                    .to_owned(),
            ));
        }
    };
    let metadata = &repository.metadata;
    let bindings = &repository.verified_by;
    let card = AuthoredCard {
        source_path: path,
        api_version: ApiVersion::default(),
        kind: CardKind::Repository,
        metadata: Metadata {
            name,
            version: metadata.version.clone(),
            bump: metadata.bump.clone(),
            uid: None,
            labels: metadata.labels.clone(),
            annotations: metadata.annotations.clone(),
            spec_hash: None,
            artifact_hash: None,
            origin: None,
        },
        spec: Spec::Repository(RepositorySpec {
            provider: RepositoryProvider::default(),
            host: repository.spec.host.clone(),
            repository_id: repository.spec.repository_id.clone(),
            verified_by: bindings
                .verifier
                .iter()
                .map(|verifier| VerificationBinding {
                    verifier: verifier.clone(),
                    runs_on: bindings.runs_on.clone(),
                    on_failure: Vec::new(),
                    on_verdict_change: Vec::new(),
                })
                .collect(),
        }),
        artifacts: Vec::new(),
    };
    let hint = RepositoryHint {
        remote,
        infer_name: repository.metadata.name.is_none(),
    };
    Ok((card, hint))
}

/// Read the `origin` remote of the Git checkout containing `start`.
///
/// Walks ancestors to the first `.git` directory or `gitdir:` file, follows
/// a linked worktree's `commondir` to the shared config, and reads
/// `[remote "origin"] url`, whose variable name Git matches in any case and
/// whose value may be double-quoted. Returns `None` outside a checkout, without an
/// origin, or when the URL is not a repository remote.
fn origin_remote(start: &Path) -> Option<RepositoryRemote> {
    let dot_git = start
        .ancestors()
        .map(|dir| dir.join(".git"))
        .find(|dot_git| dot_git.exists())?;
    let git_dir = if dot_git.is_file() {
        let text = std::fs::read_to_string(&dot_git).ok()?;
        dot_git
            .parent()?
            .join(text.trim().strip_prefix("gitdir:")?.trim())
    } else {
        dot_git
    };
    let common_dir = match std::fs::read_to_string(git_dir.join("commondir")) {
        Ok(common) => git_dir.join(common.trim()),
        Err(_) => git_dir,
    };
    let config = std::fs::read_to_string(common_dir.join("config")).ok()?;
    let mut in_origin = false;
    for line in config.lines().map(str::trim) {
        if line.starts_with('[') {
            in_origin = line == r#"[remote "origin"]"#;
        } else if in_origin
            && let Some((key, value)) = line.split_once('=')
            && key.trim().eq_ignore_ascii_case("url")
        {
            let value = value.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|quoted| quoted.strip_suffix('"'))
                .unwrap_or(value);
            return RepositoryRemote::parse(value);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;
    use wyrd_spec::card::repository::RepositoryRemote;

    use super::origin_remote;

    /// The origin URL is read from a checkout and from a linked worktree,
    /// and other remotes are ignored.
    ///
    /// # Panics
    /// Panics when either checkout layout yields a different origin.
    #[test]
    fn origin_remote_reads_checkouts_and_linked_worktrees() {
        let root = TempDir::new().unwrap();
        let main = root.path().join("main");
        fs::create_dir_all(main.join(".git/worktrees/feature")).unwrap();
        fs::write(
            main.join(".git/config"),
            "[core]\n\tbare = false\n[remote \"upstream\"]\n\turl = git@github.com:other/fork.git\n[remote \"origin\"]\n\turl = https://github.com/Acme/Widget.git\n",
        )
        .unwrap();
        fs::write(main.join(".git/worktrees/feature/commondir"), "../..\n").unwrap();
        let worktree = root.path().join("feature");
        fs::create_dir_all(worktree.join("nested")).unwrap();
        fs::write(
            worktree.join(".git"),
            format!(
                "gitdir: {}\n",
                main.join(".git/worktrees/feature").display()
            ),
        )
        .unwrap();
        let expected = Some(RepositoryRemote {
            host: "github.com".to_owned(),
            full_name: "Acme/Widget".to_owned(),
        });
        assert_eq!(origin_remote(&main), expected);
        assert_eq!(origin_remote(&worktree.join("nested")), expected);
        assert_eq!(origin_remote(&root.path().join("elsewhere")), None);
    }

    /// Git's quoted values and case-insensitive variable names in the
    /// origin section yield the same remote as the plain form.
    ///
    /// # Errors
    /// Returns the IO error when the temporary checkout cannot be written.
    ///
    /// # Panics
    /// Panics when a legal origin form yields a different remote.
    #[test]
    fn origin_remote_decodes_quoted_values_and_any_key_case() -> std::io::Result<()> {
        for line in [
            "url = \"https://github.com/acme/widget.git\"",
            "URL = https://github.com/acme/widget.git",
            "Url=\"https://github.com/acme/widget.git\" ",
        ] {
            let root = TempDir::new()?;
            fs::create_dir_all(root.path().join(".git"))?;
            fs::write(
                root.path().join(".git/config"),
                format!("[remote \"origin\"]\n\t{line}\n"),
            )?;
            assert_eq!(
                origin_remote(root.path()),
                Some(RepositoryRemote {
                    host: "github.com".to_owned(),
                    full_name: "acme/widget".to_owned(),
                }),
                "{line}"
            );
        }
        Ok(())
    }
}
