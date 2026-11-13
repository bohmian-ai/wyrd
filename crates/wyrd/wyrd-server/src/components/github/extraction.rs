//! Pull-request extraction: the accepted source of one queued PR execution,
//! acquired once through the code-review pipeline as typed Verifier
//! evidence.
//!
//! The baseline is Wyrd's choice, read from the PR's completed execution
//! history; the pipeline acquires and verifies the pinned source and
//! selects context for the active changes. Wyrd never clones, indexes or
//! reads the repository itself.

use bohmian_code_review_pipeline as pipeline;
use bohmian_code_review_pipeline::{
    BaselineProblem, ColdBuildError, ComparisonError, ContextError, Disposition, GitHubClient,
    GitHubError, Identity, InstallationId, ObjectSha, PinnedReview, PipelineError, PipelineLimits,
    PullRequestPins, RepositoryIdentity, ReviewPipeline, TenantId,
};
use output::{
    ChangeKind, ChangeSet, ChangedFile, ExclusionReason, FallbackReason, FileSource, Hunk,
    PreviousReview, PullRequestExtraction, PullRequestFormat, PullRequestHead, PullRequestMetadata,
    PullRequestRepository, PullRequestReview, PullRequestState, ReviewMode,
};
use vala_verify::blocks::BlockError;
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardUid, ExecutionId, RepositoryId};
use wyrd_spec::reference::CardRef;
use wyrd_spec::verification::execution::{PullRequestActivation, PullRequestRef};
use wyrd_sql::queries::cards::get_card_by_uid;
use wyrd_sql::queries::verifier_executions::{PullRequestBaseline, VerifierExecutionQueue};

use super::review_cache::{CodeReviewMetadata, CodeReviewObjects};
use super::service::GithubConnections;

mod output;

/// The extractor version a `pull_request` extraction's provenance records.
pub const PULL_REQUEST_EXTRACT_VERSION: &str = "pull_request/1";

/// The accepted PR source one queued execution extracts.
#[derive(Debug, Clone)]
pub struct PullRequestSource {
    /// The executing PR execution, whose completed predecessors supply
    /// the baseline.
    pub execution_id: ExecutionId,
    /// The Repository Card the execution verifies.
    pub repository: CardUid,
    /// The admitted provider pins.
    pub activation: PullRequestActivation,
}

impl GithubConnections<'_> {
    /// Extract `source` for `tenant`: choose its baseline from completed
    /// history, then acquire its pinned evidence with one pipeline call
    /// through the tenant connection's installation token.
    ///
    /// The newest completed review of the same PR under the exact same
    /// Verifier makes the review incremental from its head; without one
    /// the review is full, as the first review or because the policy
    /// changed, and when the pipeline reports that baseline rewritten or
    /// unavailable it is repeated in full under that reason. No SQL
    /// transaction is open during GitHub or object-store IO.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] when the accepted source lacks its
    /// head repository or a pin is malformed, the connection no longer
    /// reaches the repository, or the pipeline refuses the source, and
    /// [`BlockError::Retryable`] when execution history, the connection,
    /// GitHub or the cache is unavailable now.
    #[tracing::instrument(
        skip_all,
        fields(operation = "verification.extract.pull_request", tenant_id = %tenant,
               execution_id = %source.execution_id)
    )]
    pub(crate) async fn extract_pull_request(
        &self,
        tenant: DataTenantId,
        source: &PullRequestSource,
    ) -> Result<PullRequestExtraction, BlockError> {
        let activation = &source.activation;
        let head_repository = activation.head_repository_id.as_ref().ok_or_else(|| {
            terminal("the accepted pull request source has no head repository, so its source cannot be acquired")
        })?;
        let repository_id = provider_id(&activation.repository_id)?;
        let pins = PullRequestPins {
            number: activation.number,
            base: commit(&activation.base.sha)?,
            head: commit(&activation.head.sha)?,
            head_repository_id: provider_id(head_repository)?,
        };
        let app = self.app().map_err(connection_failure)?;
        let (card, baseline) = self.history(tenant, source).await?;
        let access = self
            .installation_access(tenant, None, repository_id)
            .await
            .map_err(connection_failure)?;
        let (owner, name) = access
            .repository
            .full_name
            .split_once('/')
            .ok_or_else(|| terminal("the selected repository has no owner/name label"))?;
        let identity = Identity {
            tenant: TenantId::new(&tenant.to_string())
                .map_err(|_| terminal("the tenant is not a code-review tenant"))?,
            installation: InstallationId::new(access.installation)
                .map_err(|_| terminal("the installation id is invalid"))?,
            repository: RepositoryIdentity::new(repository_id, owner, name)
                .map_err(|_| terminal("the selected repository label is invalid"))?,
        };
        let client = app
            .review_client(&access.token)
            .map_err(connection_failure)?;
        let reviews = ReviewPipeline::new(
            &app.review_objects,
            &app.review_metadata,
            PipelineLimits::default(),
        );
        let (review, previous, fallback) =
            review(&reviews, &client, &identity, &pins, &baseline).await?;
        let repository = PullRequestRepository {
            card,
            host: activation.host.clone(),
            repository_id: activation.repository_id.clone(),
        };
        extraction(repository, head_repository, review, previous, fallback)
    }

    /// The Repository Card `source` verifies and its review baseline, read
    /// in one short transaction committed before any provider IO.
    ///
    /// # Errors
    /// Returns [`BlockError::Retryable`] when the registry or execution
    /// history cannot be read now.
    async fn history(
        &self,
        tenant: DataTenantId,
        source: &PullRequestSource,
    ) -> Result<(CardRef, PullRequestBaseline), BlockError> {
        let unavailable = |what: &str| BlockError::Retryable {
            reason: format!("{what} is unavailable now"),
        };
        let mut conn = self
            .conn(tenant)
            .await
            .map_err(|_| unavailable("execution history"))?;
        let card = get_card_by_uid(&mut conn, &source.repository)
            .await
            .map_err(|_| unavailable("the Repository Card"))?;
        let baseline = VerifierExecutionQueue::default()
            .pull_request_baseline(&mut conn, source.execution_id)
            .await
            .map_err(|error| {
                tracing::warn!(error = %error, "pull request history read failed");
                unavailable("execution history")
            })?;
        conn.commit()
            .await
            .map_err(|_| unavailable("execution history"))?;
        Ok((
            CardRef {
                kind: card.kind,
                name: card.name,
                version: card.version,
                uid: Some(card.card_uid),
            },
            baseline,
        ))
    }
}

/// Review `pins` incrementally from `baseline`'s head, or in full with the
/// reason no usable baseline exists: none, another Verifier version, or a
/// head the pipeline reports rewritten or unavailable.
///
/// # Errors
/// Returns [`pipeline_failure`]'s classification of a pipeline error other
/// than an unusable baseline.
async fn review(
    reviews: &ReviewPipeline<'_, CodeReviewObjects, CodeReviewMetadata>,
    client: &GitHubClient,
    identity: &Identity,
    pins: &PullRequestPins,
    baseline: &PullRequestBaseline,
) -> Result<
    (
        PinnedReview,
        Option<(ExecutionId, ObjectSha)>,
        Option<FallbackReason>,
    ),
    BlockError,
> {
    let reason = match baseline {
        PullRequestBaseline::FirstReview => FallbackReason::FirstReview,
        PullRequestBaseline::PolicyChanged => FallbackReason::PolicyChanged,
        PullRequestBaseline::Previous { execution_id, head } => match ObjectSha::new(head) {
            Ok(head) => match reviews.pinned(client, identity, pins, Some(&head)).await {
                Ok(review) => return Ok((review, Some((*execution_id, head)), None)),
                Err(PipelineError::Comparison(ComparisonError::BaselineUnusable {
                    problem: BaselineProblem::Rewritten,
                    ..
                })) => FallbackReason::HistoryRewritten,
                Err(PipelineError::Comparison(ComparisonError::BaselineUnusable {
                    problem: BaselineProblem::Unavailable,
                    ..
                })) => FallbackReason::HistoryUnavailable,
                Err(error) => return Err(pipeline_failure(&error)),
            },
            // A stored head that is no commit id cannot anchor a comparison.
            Err(_) => FallbackReason::HistoryUnavailable,
        },
    };
    let review = reviews
        .pinned(client, identity, pins, None)
        .await
        .map_err(|error| pipeline_failure(&error))?;
    Ok((review, None, Some(reason)))
}

/// The typed extraction of `review` under `repository`.
///
/// # Errors
/// Returns [`BlockError::Terminal`] when the library context cannot be
/// represented as JSON.
fn extraction(
    repository: PullRequestRepository,
    head_repository: &RepositoryId,
    review: PinnedReview,
    previous: Option<(ExecutionId, ObjectSha)>,
    fallback: Option<FallbackReason>,
) -> Result<PullRequestExtraction, BlockError> {
    let source = review.pull_request;
    let context = serde_json::to_value(&review.context.context)
        .map_err(|_| terminal("the review context is not representable as JSON"))?;
    Ok(PullRequestExtraction {
        format: PullRequestFormat::PullRequest,
        pr: PullRequestMetadata {
            repository,
            number: source.number,
            url: source.url,
            title: source.title,
            body: source.body,
            state: match source.state {
                pipeline::PullRequestState::Open => PullRequestState::Open,
                pipeline::PullRequestState::Closed => PullRequestState::Closed,
            },
            draft: source.draft,
            base: PullRequestRef {
                ref_name: source.base_ref,
                sha: source.base.as_str().to_owned(),
            },
            head: PullRequestHead {
                repository_id: head_repository.clone(),
                ref_name: source.head_ref,
                sha: source.head.as_str().to_owned(),
            },
            merge_base: source.merge_base.as_str().to_owned(),
        },
        review: PullRequestReview {
            mode: if previous.is_some() {
                ReviewMode::Incremental
            } else {
                ReviewMode::Full
            },
            comparison_base: review.active.base.as_str().to_owned(),
            previous: previous.map(|(execution_id, head)| PreviousReview {
                execution_id,
                head: head.as_str().to_owned(),
            }),
            fallback_reason: fallback,
            full_pr: change_set(review.full),
        },
        changes: change_set(review.active),
        context,
    })
}

/// The spec projection of a library change set.
fn change_set(set: pipeline::ChangeSet) -> ChangeSet {
    ChangeSet {
        base: set.base.as_str().to_owned(),
        head: set.head.as_str().to_owned(),
        files: set
            .files
            .into_iter()
            .map(|file| ChangedFile {
                path: file.path.as_str().to_owned(),
                previous_path: file.previous_path.map(|path| path.as_str().to_owned()),
                status: match file.status {
                    pipeline::ChangeKind::Added => ChangeKind::Added,
                    pipeline::ChangeKind::Modified => ChangeKind::Modified,
                    pipeline::ChangeKind::Deleted => ChangeKind::Deleted,
                    pipeline::ChangeKind::Renamed => ChangeKind::Renamed,
                    pipeline::ChangeKind::TypeChanged => ChangeKind::TypeChanged,
                },
                before: file.before.map(file_source),
                after: file.after.map(file_source),
                hunks: file
                    .hunks
                    .into_iter()
                    .map(|hunk| Hunk {
                        old_start: hunk.old_start,
                        old_lines: hunk.old_lines,
                        new_start: hunk.new_start,
                        new_lines: hunk.new_lines,
                        patch: hunk.patch,
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// The spec projection of one side of a changed file.
fn file_source(source: pipeline::Source) -> FileSource {
    match source {
        pipeline::Source::Text { blob, text } => FileSource::Text {
            blob: blob.as_str().to_owned(),
            text,
        },
        pipeline::Source::Binary { blob } => FileSource::Binary {
            blob: blob.as_str().to_owned(),
        },
        pipeline::Source::Unavailable {
            blob,
            reason: pipeline::UnavailableReason::Oversize { limit },
        } => FileSource::Unavailable {
            blob: blob.as_str().to_owned(),
            limit_bytes: limit,
        },
        pipeline::Source::Excluded { blob, reason } => FileSource::Excluded {
            blob: blob.as_str().to_owned(),
            reason: match reason {
                pipeline::ExclusionReason::Symlink => ExclusionReason::Symlink,
                pipeline::ExclusionReason::Submodule => ExclusionReason::Submodule,
                pipeline::ExclusionReason::Unsupported => ExclusionReason::Unsupported,
                pipeline::ExclusionReason::VendoredSegment => ExclusionReason::VendoredSegment,
                pipeline::ExclusionReason::MinifiedOutput => ExclusionReason::MinifiedOutput,
                pipeline::ExclusionReason::GeneratedAttribute => {
                    ExclusionReason::GeneratedAttribute
                }
                pipeline::ExclusionReason::VendoredAttribute => ExclusionReason::VendoredAttribute,
                pipeline::ExclusionReason::Oversize => ExclusionReason::Oversize,
            },
        },
    }
}

/// A terminal extraction failure.
fn terminal(reason: &str) -> BlockError {
    BlockError::Terminal {
        reason: reason.to_owned(),
    }
}

/// A provider repository id as GitHub's number.
///
/// # Errors
/// Returns [`BlockError::Terminal`] when `id` is not a positive integer.
fn provider_id(id: &RepositoryId) -> Result<u64, BlockError> {
    id.as_str()
        .parse()
        .map_err(|_| terminal("the accepted repository id is not a GitHub id"))
}

/// A pinned commit.
///
/// # Errors
/// Returns [`BlockError::Terminal`] when `sha` is not a full commit id.
fn commit(sha: &str) -> Result<ObjectSha, BlockError> {
    ObjectSha::new(sha).map_err(|_| terminal("an accepted pin is not a full commit id"))
}

/// The block failure of a connection or token error: retryable while
/// GitHub or the registry is unavailable or the connection changed while
/// minting, terminal when the connection no longer reaches the repository.
fn connection_failure(error: WyrdError) -> BlockError {
    let reason = error.to_string();
    match error {
        WyrdError::GithubUnavailable { .. } | WyrdError::GithubConnectionConflict { .. } => {
            BlockError::Retryable { reason }
        }
        WyrdError::GithubNotConfigured { .. }
        | WyrdError::GithubNotConnected { .. }
        | WyrdError::GithubRepositoryInaccessible { .. } => BlockError::Terminal { reason },
        _ => BlockError::Retryable { reason },
    }
}

/// The block failure of a pipeline error: retryable when GitHub asks for a
/// retry or the cache is unavailable, terminal when the source itself
/// cannot be acquired or verified, which never becomes a pass.
fn pipeline_failure(error: &PipelineError) -> BlockError {
    let provider = |error: &GitHubError| matches!(error.disposition(), Disposition::Retry { .. });
    let snapshot = |error: &ColdBuildError| match error {
        ColdBuildError::GitHub(error) => provider(error),
        ColdBuildError::Storage(_) | ColdBuildError::Persistence(_) => true,
        _ => false,
    };
    let retryable = match error {
        PipelineError::Snapshot(error) => snapshot(error),
        PipelineError::Context(error) => match error {
            ContextError::GitHub(error) => provider(error),
            ContextError::Snapshot(error) => snapshot(error),
            ContextError::Storage(_) | ContextError::Persistence(_) => true,
            _ => false,
        },
        PipelineError::Comparison(ComparisonError::Provider(error)) => provider(error),
        PipelineError::Comparison(_) => false,
    };
    let reason = format!("pull request source acquisition failed: {error}");
    if retryable {
        BlockError::Retryable { reason }
    } else {
        BlockError::Terminal { reason }
    }
}
