//! Deferred evidence publication: supply a deferred Verifier input for one
//! repository commit, independent of any execution.
//!
//! The server matches the evidence to every Verifier subscribed to the input
//! on that repository and commit, and answers with a durable receipt. The
//! publisher names the repository as `owner/name` and the commit it is about;
//! each comes from the explicit argument, else `WYRD_REPOSITORY` /
//! `WYRD_COMMIT`, else the Git checkout of the working directory read from
//! `.git`. Nothing here detects or reads a particular CI system, and no
//! checkout is needed when the first two supply both.

use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::path::Path;
use std::sync::Arc;

use reqwest::Method;
use serde_json::json;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::SlotName;
use wyrd_spec::origin::CommitSha;
pub use wyrd_spec::verification::execution::{
    EvidenceRepository, EvidenceStatus, PublishEvidenceResponse,
};
use wyrd_spec::verification::execution::{EvidenceTags, InputDeclaration, PublishEvidenceRequest};

use super::{ArtifactRef, Input, RunEvidence, input_invalid};
use crate::client::WyrdClient;

/// The publication route.
const PATH: &str = "/v1/verification/evidence/publish";
/// The variable naming the repository when no argument does.
pub const REPOSITORY_VAR: &str = "WYRD_REPOSITORY";
/// The variable naming the commit when no argument does.
pub const COMMIT_VAR: &str = "WYRD_COMMIT";

/// Cheap-to-clone, tenant-scoped deferred evidence publisher.
#[derive(Clone)]
pub struct Evidence {
    /// Shared authenticated client owning transport, credentials, and the
    /// access-token cache.
    client: Arc<WyrdClient>,
}

impl Debug for Evidence {
    /// Prints the handle without its client, which holds credential material.
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("Evidence").finish_non_exhaustive()
    }
}

impl Evidence {
    /// Connect the handle through the ambient client, as
    /// [`WyrdClient::from_global`] resolves it.
    ///
    /// # Errors
    /// Returns a Wyrd error when the global config file cannot be read or
    /// parsed, no credential can be resolved, or a required exchange is
    /// refused or unreachable.
    pub async fn connect() -> Result<Self, WyrdError> {
        Ok(Self {
            client: Arc::new(WyrdClient::connect_ambient().await?),
        })
    }

    /// Connect the handle as `client`'s principal, used as given.
    ///
    /// # Errors
    /// Returns the server's stable error when the client's required exchange
    /// is refused or unreachable.
    pub async fn connect_with(client: &WyrdClient) -> Result<Self, WyrdError> {
        Ok(Self {
            client: Arc::new(client.connected().await?),
        })
    }

    /// Publish `value` as the deferred input `name` of `repository` at
    /// `commit` and return the durable receipt.
    ///
    /// A JSON value is sent inline; a [`File`](super::File) is hashed and
    /// uploaded first unless the tenant already holds it, and its format is
    /// the declared one or its extension's; an [`ArtifactRef`] names a
    /// registered Artifact's file. `repository` (`owner/name`) and `commit`
    /// default as [`source`] resolves them from the working directory. A
    /// receipt means every subscribed Verifier holds the evidence durably;
    /// publishing the same content again replays it.
    ///
    /// # Errors
    /// Returns `WYRD_VERIFICATION_400_INPUT_INVALID` for an invalid input name,
    /// an unresolvable repository or commit, or an unreadable or formatless
    /// File; and the server's refusal: unmatched or conflicting evidence
    /// (409), a repository the tenant has not connected (403), or a value its
    /// subscribers do not accept (400/413).
    pub async fn publish(
        &self,
        name: &str,
        value: impl Into<Input>,
        repository: Option<&str>,
        commit: Option<&str>,
    ) -> Result<PublishEvidenceResponse, WyrdError> {
        let input = SlotName::new(name).map_err(|error| {
            input_invalid(
                "the input name is not a valid slot name",
                json!({ "input": name, "reason": error.to_string() }),
            )
        })?;
        let cwd = std::env::current_dir().map_err(|error| {
            input_invalid(
                "the working directory cannot be read to resolve the source",
                json!({ "reason": error.to_string() }),
            )
        })?;
        let (repository, commit) = source(
            repository,
            commit,
            |name| std::env::var(name).ok().filter(|value| !value.is_empty()),
            &cwd,
        )?;
        let value = self.declare(&input, value.into()).await?;
        let request = PublishEvidenceRequest {
            repository,
            tags: EvidenceTags { commit },
            input,
            value,
        };
        self.client
            .http
            .request_json(Method::POST, PATH, Some(&request))
            .await
    }

    /// The wire declaration of `value`, uploading a File the tenant lacks.
    ///
    /// # Errors
    /// As [`RunEvidence::pin`] for a File.
    async fn declare(&self, input: &SlotName, value: Input) -> Result<InputDeclaration, WyrdError> {
        match value {
            Input::Value(value) => Ok(InputDeclaration::Json { value }),
            Input::Artifact(ArtifactRef { artifact, path }) => {
                Ok(InputDeclaration::Artifact { artifact, path })
            }
            Input::File(file) => {
                let format = file.format(&[]).ok_or_else(|| {
                    input_invalid(
                        "declare the file's format; it has no recognised extension",
                        json!({ "input": input }),
                    )
                })?;
                let mut evidence = RunEvidence::default();
                evidence
                    .files
                    .push((input.clone(), file, vec![format], None));
                evidence
                    .pin(&self.client)
                    .await?
                    .remove(input)
                    .ok_or_else(|| WyrdError::internal("the pinned File has no declaration"))
            }
        }
    }
}

/// Resolve the repository slug and commit a publication is about.
///
/// Each is taken from its argument, else from `env` ([`REPOSITORY_VAR`],
/// [`COMMIT_VAR`]), else from the Git checkout containing `dir`: `origin`'s
/// `owner/name` and the `HEAD` commit, read without running `git`.
///
/// # Errors
/// Returns `WYRD_VERIFICATION_400_INPUT_INVALID` naming the argument and the
/// variable when no source supplies a value, or when the commit is not a
/// commit SHA.
pub fn source(
    repository: Option<&str>,
    commit: Option<&str>,
    env: impl Fn(&str) -> Option<String>,
    dir: &Path,
) -> Result<(String, CommitSha), WyrdError> {
    let repository = repository
        .map(str::to_owned)
        .or_else(|| env(REPOSITORY_VAR))
        .or_else(|| wyrd_loader::origin_remote(dir).map(|remote| remote.full_name))
        .ok_or_else(|| {
            input_invalid(
                "name the repository as owner/name with --repository or WYRD_REPOSITORY; no Git checkout supplies one",
                json!({ "argument": "repository", "variable": REPOSITORY_VAR }),
            )
        })?;
    let commit = commit
        .map(str::to_owned)
        .or_else(|| env(COMMIT_VAR))
        .or_else(|| wyrd_loader::head_commit(dir))
        .ok_or_else(|| {
            input_invalid(
                "name the commit with --commit or WYRD_COMMIT; no Git checkout supplies one",
                json!({ "argument": "commit", "variable": COMMIT_VAR }),
            )
        })?;
    let commit = CommitSha::new(commit.clone()).map_err(|error| {
        input_invalid(
            "the commit is not a commit SHA",
            json!({ "commit": commit, "reason": error.to_string() }),
        )
    })?;
    Ok((repository, commit))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;

    use tempfile::TempDir;

    use super::{COMMIT_VAR, REPOSITORY_VAR, source};

    /// An argument wins over its variable, a variable over the checkout, and
    /// each value resolves independently; without a checkout the variables
    /// suffice, and a missing value names its argument and variable.
    ///
    /// # Errors
    /// Returns the IO error when the temporary checkout cannot be written.
    ///
    /// # Panics
    /// Panics when a source is not taken in that order or a missing value
    /// does not refuse.
    #[test]
    fn source_prefers_arguments_then_variables_then_the_checkout() -> std::io::Result<()> {
        let head = "a".repeat(40);
        let pinned = "b".repeat(40);
        let checkout = TempDir::new()?;
        fs::create_dir_all(checkout.path().join(".git"))?;
        fs::write(checkout.path().join(".git/HEAD"), format!("{head}\n"))?;
        fs::write(
            checkout.path().join(".git/config"),
            "[remote \"origin\"]\n\turl = git@github.com:acme/widget.git\n",
        )?;
        let none = |_: &str| None;
        let vars = HashMap::from([
            (REPOSITORY_VAR, "acme/from-env".to_owned()),
            (COMMIT_VAR, pinned.clone()),
        ]);
        let env = |name: &str| vars.get(name).cloned();
        let resolved = |repository, commit, env: &dyn Fn(&str) -> Option<String>, dir| {
            source(repository, commit, env, dir)
                .map(|(repository, commit)| (repository, commit.as_str().to_owned()))
        };

        assert_eq!(
            resolved(None, None, &none, checkout.path()).ok(),
            Some(("acme/widget".to_owned(), head.clone()))
        );
        assert_eq!(
            resolved(None, None, &env, checkout.path()).ok(),
            Some(("acme/from-env".to_owned(), pinned.clone()))
        );
        assert_eq!(
            resolved(Some("acme/explicit"), None, &none, checkout.path()).ok(),
            Some(("acme/explicit".to_owned(), head)),
            "each value resolves independently"
        );
        let elsewhere = TempDir::new()?;
        assert_eq!(
            resolved(None, None, &env, elsewhere.path()).ok(),
            Some(("acme/from-env".to_owned(), pinned)),
            "no checkout is needed"
        );
        let missing = resolved(Some("acme/explicit"), None, &none, elsewhere.path())
            .expect_err("no commit source");
        assert_eq!(missing.code(), "WYRD_VERIFICATION_400_INPUT_INVALID");
        assert!(missing.to_string().contains("WYRD_COMMIT"), "{missing}");
        let invalid = resolved(
            Some("acme/explicit"),
            Some("not-a-sha"),
            &none,
            elsewhere.path(),
        )
        .expect_err("invalid commit");
        assert_eq!(invalid.code(), "WYRD_VERIFICATION_400_INPUT_INVALID");
        Ok(())
    }
}
