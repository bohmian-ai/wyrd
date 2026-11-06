//! Inspection and cancellation of queued executions.
//!
//! Every operation reads the execution's Verifier from its queue row, then
//! authorizes `verifier:read` (or `verifier:run` to cancel) on exactly that
//! Verifier, so a grant scoped to one Verifier inspects only its
//! executions. An absent or other-tenant execution is the same not-found.
//! Invocation and publication pages read the execution's accepted state at a
//! cut their first page fixes, so a listing neither skips nor repeats entries
//! while the execution progresses, and never returns publication content. A
//! state that cannot be read now is unavailable, never empty.

use base64::Engine as _;
use bytes::Bytes;
use futures_util::stream::BoxStream;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use wyrd_runtime::Permission;
use wyrd_runtime::permission::{Action, PermissionScope, Resource};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::ExecutionId;
use wyrd_spec::storage::{DownloadInitResponse, StorageBackendKind};
use wyrd_spec::verification::execution::{
    CancelExecutionResponse, EXECUTION_PAGE_DEFAULT, EXECUTION_PAGE_MAX, ExecutionInvocation,
    ExecutionPageQuery, HistoryProjection, InvocationPage, InvocationProgress,
    InvocationSettlement, Publication, PublicationContent, PublicationFile, PublicationPage,
    PublicationSelector, PublicationSummary, Speculation,
};
use wyrd_spec::verification::state::{AcceptedPublication, ExecutionState, InvocationRecord};
use wyrd_spec::verification::{PublicationName, TaskId};
use wyrd_sql::TenantConn;
use wyrd_sql::queries::verifier_executions::ExecutionStatus;
use wyrd_storage::error::StorageError;
use wyrd_storage::tenant_path::ValidatedPath;

use super::VerificationControl;
use crate::audit;
use crate::components::auth::Caller;
use crate::state::registry_db_error;
use crate::verification::hosted::publications::published_object;
use crate::verification::inputs::InputResolver;
use crate::verification::state::RunState;

/// Audit operation of an invocation page read.
const LIST_INVOCATIONS: &str = "verification.execution.invocations";

/// Audit operation of a publication page read.
const LIST_PUBLICATIONS: &str = "verification.execution.publications";

/// Audit operation of one publication read.
const READ_PUBLICATION: &str = "verification.execution.publication";

/// Audit operation of one publication file download plan.
const DOWNLOAD_PUBLICATION: &str = "verification.execution.publication.download";

/// Audit operation of one publication file's local bytes.
const READ_PUBLICATION_CONTENT: &str = "verification.execution.publication.content";

/// Audit operation of a cancellation.
const CANCEL: &str = "verification.execution.cancel";

/// Longest cursor accepted, in bytes.
const CURSOR_MAX_BYTES: usize = 2048;

/// The position a page resumes at, bound to its tenant, execution, and
/// listing.
#[derive(Serialize, Deserialize)]
struct Cursor {
    /// Tenant the listing was read in.
    t: Uuid,
    /// Execution listed.
    e: Uuid,
    /// The listing: `invocations` or `publications`.
    l: String,
    /// The listing's cut.
    c: i64,
    /// Sequence of the last entry returned.
    a: i64,
}

/// Where a page of one listing starts.
#[derive(Debug, Clone, Copy)]
struct PagePosition {
    /// The listing's cut: the state sequence its first page saw.
    through: i64,
    /// The sequence of the last entry already returned; `0` before any.
    after: i64,
}

/// The validation refusal naming `field`.
fn invalid(message: &str, field: &str) -> WyrdError {
    WyrdError::Validation {
        message: message.to_owned(),
        details: serde_json::json!({ "field": field }),
    }
}

/// The page size and resume position `query` asks for in `listing` of
/// `execution` in `tenant`; `None` starts a new listing.
///
/// # Errors
/// Returns [`WyrdError::Validation`] for a limit outside 1–200 or a cursor
/// that is empty, oversized, malformed, or bound to another tenant,
/// execution, or listing.
fn position(
    query: &ExecutionPageQuery,
    tenant: Uuid,
    execution: ExecutionId,
    listing: &str,
) -> Result<(u32, Option<PagePosition>), WyrdError> {
    let limit = query.limit.unwrap_or(EXECUTION_PAGE_DEFAULT);
    if !(1..=EXECUTION_PAGE_MAX).contains(&limit) {
        return Err(invalid("limit must be between 1 and 200", "limit"));
    }
    let position = query
        .cursor
        .as_deref()
        .map(|raw| {
            (!raw.is_empty() && raw.len() <= CURSOR_MAX_BYTES)
                .then(|| {
                    base64::engine::general_purpose::URL_SAFE_NO_PAD
                        .decode(raw)
                        .ok()
                })
                .flatten()
                .and_then(|bytes| serde_json::from_slice::<Cursor>(&bytes).ok())
                .filter(|cursor| {
                    cursor.t == tenant && cursor.e == execution.as_uuid() && cursor.l == listing
                })
                .map(|cursor| PagePosition {
                    through: cursor.c,
                    after: cursor.a,
                })
                .ok_or_else(|| invalid("cursor is not a continuation of this listing", "cursor"))
        })
        .transpose()?;
    Ok((limit, position))
}

/// Keep at most `limit` of the `entries` of a listing at `position`, or at a
/// new cut of `state` when `None`, and the cursor continuing after them when
/// more remain.
fn page<T>(
    entries: impl IntoIterator<Item = (i64, T)>,
    state: &ExecutionState,
    position: Option<PagePosition>,
    limit: u32,
    tenant: Uuid,
    execution: ExecutionId,
    listing: &str,
) -> (Vec<T>, Option<String>) {
    let position = position.unwrap_or(PagePosition {
        through: state.sequence,
        after: 0,
    });
    let through = position.through;
    let limit = limit as usize;
    let mut entries: Vec<_> = entries
        .into_iter()
        .filter(|(sequence, _)| *sequence > position.after && *sequence <= through)
        .take(limit + 1)
        .collect();
    let more = entries.len() > limit;
    entries.truncate(limit);
    let next_cursor = entries.last().filter(|_| more).map(|(after, _)| {
        let cursor = Cursor {
            t: tenant,
            e: execution.as_uuid(),
            l: listing.to_owned(),
            c: through,
            a: *after,
        };
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&cursor).unwrap_or_default())
    });
    (
        entries.into_iter().map(|(_, entry)| entry).collect(),
        next_cursor,
    )
}

/// One invocation attempt's public progress.
fn invocation(entry: InvocationRecord) -> ExecutionInvocation {
    let (progress, reason) = match entry.settlement {
        None => (InvocationProgress::Started, None),
        Some(InvocationSettlement::Action { result, .. }) => (
            InvocationProgress::Action {
                status: result.status,
            },
            result.reason,
        ),
        Some(InvocationSettlement::Requirement { result }) => (
            InvocationProgress::Requirement {
                outcome: result.outcome,
            },
            Some(result.reason),
        ),
    };
    ExecutionInvocation {
        producer: entry.producer,
        progress,
        reason,
        speculation: Speculation::None,
    }
}

/// The not-found refusal of an execution the caller's tenant does not hold.
pub(super) fn execution_not_found(execution_id: ExecutionId) -> WyrdError {
    WyrdError::VerificationExecutionNotFound {
        message: "no verification execution with this ID in the caller's tenant".to_owned(),
        details: serde_json::json!({ "execution_id": execution_id }),
    }
}

impl<'a> VerificationControl<'a> {
    /// Refuse, and audit the refusal, when `caller` holds no Verifier grant
    /// for `action` at any scope, before anything is read.
    ///
    /// A caller who may never inspect or cancel cannot learn which
    /// execution IDs exist.
    ///
    /// # Errors
    /// Returns [`WyrdError::PermissionDeniedRbac`] without any such grant.
    pub(super) fn require_verifier(
        &self,
        caller: &Caller,
        action: Action,
        operation: &str,
        resource: &str,
    ) -> Result<(), WyrdError> {
        if caller
            .principal
            .effective_permissions
            .covers_operation(&Resource::Verifier, &action)
        {
            return Ok(());
        }
        let any = Permission {
            resource: Resource::Verifier,
            action,
            scope: PermissionScope::All,
        };
        audit::authorize(self.state, caller, &any, operation, resource)
    }

    /// Open the caller's tenant transaction, read queued `execution_id`, and
    /// authorize Verifier `action` on its Verifier for `operation`.
    ///
    /// # Errors
    /// Returns [`WyrdError::PermissionDeniedRbac`] without the grant,
    /// [`WyrdError::VerificationExecutionNotFound`] when the tenant has no
    /// such queued execution, and a registry error when a read fails.
    async fn authorized(
        &self,
        caller: &Caller,
        execution_id: ExecutionId,
        action: Action,
        operation: &str,
    ) -> Result<(TenantConn<'a>, ExecutionStatus), WyrdError> {
        let resource = format!("verification.execution:{execution_id}");
        self.require_verifier(caller, action.clone(), operation, &resource)?;
        let mut conn = self
            .state
            .registry_tenant_conn(caller.data_tenant_id)
            .await?;
        let status = self
            .queue
            .execution_status(&mut conn, execution_id)
            .await
            .map_err(registry_db_error)?
            .ok_or_else(|| execution_not_found(execution_id))?;
        let required = Permission {
            resource: Resource::Verifier,
            action,
            scope: PermissionScope::Verifier(status.verifier_card_uid.clone()),
        };
        audit::authorize(self.state, caller, &required, operation, &resource)?;
        Ok((conn, status))
    }

    /// Authorize `operation` as [`Self::authorized`] does, then read the
    /// execution's accepted state and revision as the tenant's SYSTEM
    /// principal serving `caller`, without writing it; an execution with no
    /// accepted revision yet has fresh state at revision 0.
    ///
    /// # Errors
    /// The errors of [`Self::authorized`], and
    /// `WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE` when the accepted state
    /// cannot be read now.
    async fn accepted(
        &self,
        caller: &Caller,
        execution_id: ExecutionId,
        operation: &str,
    ) -> Result<(ExecutionStatus, i64, ExecutionState), WyrdError> {
        let (mut conn, status) = self
            .authorized(caller, execution_id, Action::Read, operation)
            .await?;
        let query = self.records(&mut conn, caller).await?;
        conn.commit().await.map_err(registry_db_error)?;
        let (revision, state) = RunState::accepted(
            self.state,
            caller.data_tenant_id,
            execution_id,
            &InputResolver::new(&query, None),
        )
        .await?;
        Ok((status, revision, state))
    }

    /// One page of queued `execution_id`'s invocation attempts, its status,
    /// and the accepted state revision it was read at.
    ///
    /// Progress is read from the accepted execution state, so every revision
    /// Scribe accepted is current.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] for a bad limit or cursor, and the
    /// errors of [`Self::authorized`].
    pub(crate) async fn invocations(
        &self,
        caller: &Caller,
        execution_id: ExecutionId,
        query: &ExecutionPageQuery,
    ) -> Result<InvocationPage, WyrdError> {
        let tenant = caller.data_tenant_id.as_uuid();
        let (limit, position) = position(query, tenant, execution_id, "invocations")?;
        let (status, revision, state) = self
            .accepted(caller, execution_id, LIST_INVOCATIONS)
            .await?;
        let (items, next_cursor) = page(
            state
                .invocations
                .iter()
                .map(|record| (record.sequence, record.clone())),
            &state,
            position,
            limit,
            tenant,
            execution_id,
            "invocations",
        );
        Ok(InvocationPage {
            execution_id,
            status: status.state.status,
            items: items.into_iter().map(invocation).collect(),
            next_cursor,
            history: HistoryProjection {
                acknowledged: u64::try_from(revision).unwrap_or_default(),
                pending: 0,
                failed: 0,
            },
        })
    }

    /// One page of queued `execution_id`'s publication names, without content.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] for a bad limit or cursor, and the
    /// errors of [`Self::authorized`].
    pub(crate) async fn publications(
        &self,
        caller: &Caller,
        execution_id: ExecutionId,
        query: &ExecutionPageQuery,
    ) -> Result<PublicationPage, WyrdError> {
        let tenant = caller.data_tenant_id.as_uuid();
        let (limit, position) = position(query, tenant, execution_id, "publications")?;
        let (_, _, state) = self
            .accepted(caller, execution_id, LIST_PUBLICATIONS)
            .await?;
        let (items, next_cursor) = page(
            state.publications.iter().map(|accepted| {
                let publication = &accepted.publication;
                (
                    accepted.sequence,
                    PublicationSummary {
                        name: publication.name.clone(),
                        kind: publication.content.kind(),
                        producer: publication.producer.clone(),
                        speculation: Speculation::None,
                    },
                )
            }),
            &state,
            position,
            limit,
            tenant,
            execution_id,
            "publications",
        );
        Ok(PublicationPage {
            execution_id,
            items,
            next_cursor,
        })
    }

    /// The publication `name` of `step`'s invocation attempt `selector`
    /// names, with its value or manifest.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] for a zero attempt or a `path`, which
    /// only a download takes,
    /// [`WyrdError::VerificationPublicationNotFound`] when the attempt
    /// published no such name, and the errors of [`Self::authorized`].
    pub(crate) async fn publication(
        &self,
        caller: &Caller,
        execution_id: ExecutionId,
        step: &TaskId,
        name: &PublicationName,
        selector: &PublicationSelector,
    ) -> Result<Publication, WyrdError> {
        if selector.path.is_some() {
            return Err(invalid("only a publication download takes a path", "path"));
        }
        self.select(caller, execution_id, step, name, selector, READ_PUBLICATION)
            .await
            .map(|stored| stored.publication)
    }

    /// A download plan for one file of a file or directory publication: the
    /// file publication's only file, or the directory's regular file at
    /// `selector.path`.
    ///
    /// Cloud backends answer with a presigned URL. The local backend has
    /// none, so its plan points at [`Self::content`]'s root-relative route,
    /// which authorizes `verifier:read` again rather than the Card read the
    /// local blob route requires.
    ///
    /// # Errors
    /// The errors of [`Self::file`] and the storage planning error.
    pub(crate) async fn download(
        &self,
        caller: &Caller,
        execution_id: ExecutionId,
        step: &TaskId,
        name: &PublicationName,
        selector: &PublicationSelector,
    ) -> Result<DownloadInitResponse, WyrdError> {
        let (object, file) = self
            .file(
                caller,
                execution_id,
                step,
                name,
                selector,
                DOWNLOAD_PUBLICATION,
            )
            .await?;
        let mut plan = wyrd_storage::service::download_plan(
            &self.state.storage,
            &object,
            self.state.storage.presign_ttl_secs(),
            file.size_bytes,
            base64::engine::general_purpose::STANDARD.encode(file.sha256.as_bytes()),
        )
        .await?;
        if self.state.storage.backend() == StorageBackendKind::Local {
            plan.plan.get_url = content_url(execution_id, step, name, selector);
        }
        Ok(plan)
    }

    /// The bytes of the publication file `selector` names, streamed from the
    /// local backend; the URL of a local download plan.
    ///
    /// # Errors
    /// The errors of [`Self::file`], and the storage error when the object
    /// cannot be opened or the backend is not local.
    pub(crate) async fn content(
        &self,
        caller: &Caller,
        execution_id: ExecutionId,
        step: &TaskId,
        name: &PublicationName,
        selector: &PublicationSelector,
    ) -> Result<(BoxStream<'static, Result<Bytes, StorageError>>, u64), WyrdError> {
        let (object, _) = self
            .file(
                caller,
                execution_id,
                step,
                name,
                selector,
                READ_PUBLICATION_CONTENT,
            )
            .await?;
        wyrd_storage::service::download_local_object(&self.state.storage, &object).await
    }

    /// Authorize `operation` on the publication `selector` names and locate
    /// its one file: the file publication's only file, or the directory's
    /// regular file at `selector.path`, through the completed File
    /// descriptor its acceptance kept, never an inventory lookup.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] for a JSON publication, a path on a
    /// file publication, or a directory path its manifest does not list;
    /// [`WyrdError::Internal`] for a descriptor that does not match its
    /// manifest file; and the errors of [`Self::publication`].
    async fn file(
        &self,
        caller: &Caller,
        execution_id: ExecutionId,
        step: &TaskId,
        name: &PublicationName,
        selector: &PublicationSelector,
        operation: &str,
    ) -> Result<(ValidatedPath, PublicationFile), WyrdError> {
        let stored = self
            .select(caller, execution_id, step, name, selector, operation)
            .await?;
        let path = selector.path.as_deref();
        let found = match stored.publication.content {
            PublicationContent::File { manifest } if path.is_none() => {
                manifest.files.into_iter().zip(stored.objects).next()
            }
            PublicationContent::File { .. } => {
                return Err(invalid(
                    "a file publication downloads without a path",
                    "path",
                ));
            }
            PublicationContent::Directory { manifest } if path.is_some() => manifest
                .files
                .into_iter()
                .zip(stored.objects)
                .find(|(file, _)| Some(file.path.as_str()) == path),
            PublicationContent::Directory { .. } => {
                return Err(invalid(
                    "a directory publication download names one of its manifest's files",
                    "path",
                ));
            }
            PublicationContent::Json { .. } => {
                return Err(invalid(
                    "a JSON publication is read through its value, not downloaded",
                    "name",
                ));
            }
        };
        let (file, object) =
            found.ok_or_else(|| invalid("the publication manifest lists no such file", "path"))?;
        let object = published_object(caller.data_tenant_id, &file, &object)?;
        Ok((object, file))
    }

    /// Read and authorize the publication `selector` names for `operation`.
    ///
    /// # Errors
    /// The errors of [`Self::publication`].
    async fn select(
        &self,
        caller: &Caller,
        execution_id: ExecutionId,
        step: &TaskId,
        name: &PublicationName,
        selector: &PublicationSelector,
        operation: &str,
    ) -> Result<AcceptedPublication, WyrdError> {
        if selector.attempt == 0 {
            return Err(invalid("attempt is one-based", "attempt"));
        }
        let (_, _, state) = self.accepted(caller, execution_id, operation).await?;
        state
            .publications
            .into_iter()
            .find(|stored| {
                let publication = &stored.publication;
                publication.producer.step_id == *step
                    && publication.name == *name
                    && publication.producer.attempt == selector.attempt
                    && publication.producer.item_index == selector.item_index
                    && publication.producer.iteration_index == selector.iteration_index
            })
            .ok_or_else(|| WyrdError::VerificationPublicationNotFound {
                message: "the execution has no such publication of this producer attempt"
                    .to_owned(),
                details: serde_json::json!({
                    "execution_id": execution_id,
                    "step_id": step,
                    "name": name,
                    "attempt": selector.attempt,
                    "item_index": selector.item_index,
                    "iteration_index": selector.iteration_index,
                }),
            })
    }

    /// Cancel queued `execution_id` and answer its status after the
    /// cancellation fence commits.
    ///
    /// Unfinished work settles `cancelled`; settled work keeps its status,
    /// result, and publications, so a repeat answers the current status.
    ///
    /// # Errors
    /// The errors of [`Self::authorized`] for `verifier:run`, and a registry
    /// error when the update or commit fails.
    pub(crate) async fn cancel(
        &self,
        caller: &Caller,
        execution_id: ExecutionId,
    ) -> Result<CancelExecutionResponse, WyrdError> {
        let (mut conn, _) = self
            .authorized(caller, execution_id, Action::Run, CANCEL)
            .await?;
        self.queue
            .cancel(&mut conn, execution_id)
            .await
            .map_err(registry_db_error)?;
        let status = self
            .queue
            .execution_status(&mut conn, execution_id)
            .await
            .map_err(registry_db_error)?
            .ok_or_else(|| execution_not_found(execution_id))?;
        conn.commit().await.map_err(registry_db_error)?;
        Ok(CancelExecutionResponse {
            execution_id,
            status: status.state.status,
        })
    }
}

/// The root-relative content route of the publication file `selector` names,
/// which a local download plan hands out. Step and name are URL-safe by
/// validation; the selector is form-encoded.
fn content_url(
    execution_id: ExecutionId,
    step: &TaskId,
    name: &PublicationName,
    selector: &PublicationSelector,
) -> String {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    query.append_pair("attempt", &selector.attempt.to_string());
    if let Some(item) = selector.item_index {
        query.append_pair("item_index", &item.to_string());
    }
    if let Some(iteration) = selector.iteration_index {
        query.append_pair("iteration_index", &iteration.to_string());
    }
    if let Some(path) = &selector.path {
        query.append_pair("path", path);
    }
    format!(
        "/v1/verification/executions/{execution_id}/publications/{}/{}/content?{}",
        step.as_str(),
        name.as_str(),
        query.finish()
    )
}
