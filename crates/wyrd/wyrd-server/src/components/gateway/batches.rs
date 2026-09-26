//! Governed Batches lifecycle and batch input files.
//!
//! [`GatewayBatches`] runs every provider action as a governed Batches call
//! through [`GatewayInvocation::invoke`], so authorization, audit, admission,
//! and accounting apply. Tenant Postgres keeps what every replica must agree
//! on: each uploaded file pinned to the deployment holding it, and each batch
//! claimed under the digest of its canonical create request before the
//! provider is called, so a replayed creation never creates a second upstream
//! batch. Public ids are Wyrd's; provider ids never reach the caller.

use std::collections::BTreeMap;
use std::time::Duration;

use axum::body::{Body, Bytes};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use chrono::Utc;
use serde_json::value::to_raw_value;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use wyrd_gateway::{BatchAction, BatchInput, IngressDialect, ResponseBody, UploadContent};
use wyrd_runtime::{Action, PermissionScope, Resource};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::GatewayAccess;
use wyrd_spec::error::WyrdError;
use wyrd_spec::gateway::openai::{GatewayFile, GatewayFileDeleted};
use wyrd_spec::gateway::{GatewayContractError, GatewayOperation, ModelRef};
use wyrd_spec::ids::ProviderDeploymentName;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::gateway::{
    claim_gateway_batch, delete_gateway_batch_file, gateway_batch, gateway_batch_by_request,
    gateway_batch_file, gateway_batches, insert_gateway_batch_file, record_gateway_batch,
    release_gateway_batch,
};
use wyrd_sql::row_types::gateway::{GatewayBatchFileRow, GatewayBatchRow};

use super::invocation::{
    CallFailure, GatewayCallRequest, GatewayCallResponse, GatewayInvocation, OPERATION,
    invalid_request,
};
use super::multipart::{FileSink, FormReader};
use super::service::{internal, unavailable};
use crate::components::auth::Caller;
use crate::http::error::permission_deny_reason_to_wyrd;
use crate::state::AppState;

/// Deadline of one Batches provider action.
const BATCH_CALL_TIMEOUT: Duration = Duration::from_mins(5);

/// Batches listed per page when the caller names no limit.
const DEFAULT_LIST_LIMIT: u32 = 20;

/// Largest page the caller may request.
const MAX_LIST_LIMIT: u32 = 100;

/// Answer of a Batches operation: the gateway's object, or the provider's
/// refusal or file content relayed unchanged.
#[derive(Debug)]
pub struct BatchAnswer {
    /// HTTP status.
    pub status: u16,
    /// JSON object or relayed provider bytes.
    pub body: ResponseBody,
}

impl BatchAnswer {
    /// Answers `200` with a gateway `object`.
    ///
    /// # Errors
    /// Returns `Internal` when the object cannot be serialized.
    fn object(object: &impl serde::Serialize) -> Result<Self, WyrdError> {
        Ok(Self {
            status: 200,
            body: ResponseBody::Json(to_raw_value(object).map_err(internal)?),
        })
    }
}

impl From<GatewayCallResponse> for BatchAnswer {
    /// Relays a provider answer with its status.
    fn from(response: GatewayCallResponse) -> Self {
        Self {
            status: response.status,
            body: response.body,
        }
    }
}

/// Public file id, which names an uploaded input file or a batch's output or
/// error file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileId {
    /// `file-<id>`: an uploaded batch input file.
    Uploaded(Uuid),
    /// `file-<batch>-output`: the output file of a batch.
    Output(Uuid),
    /// `file-<batch>-error`: the error file of a batch.
    Error(Uuid),
}

impl FileId {
    /// Parses a public file id; `None` for any id the gateway never issued.
    fn parse(raw: &str) -> Option<Self> {
        let rest = raw.strip_prefix("file-")?;
        if let Some(batch) = rest.strip_suffix("-output") {
            return simple(batch).map(Self::Output);
        }
        if let Some(batch) = rest.strip_suffix("-error") {
            return simple(batch).map(Self::Error);
        }
        simple(rest).map(Self::Uploaded)
    }
}

/// Parses `raw` as a UUID in its canonical simple form only.
fn simple(raw: &str) -> Option<Uuid> {
    Uuid::try_parse(raw)
        .ok()
        .filter(|id| id.simple().to_string() == raw)
}

/// Parses a public `batch_<id>` id.
fn batch_id(raw: &str) -> Option<Uuid> {
    simple(raw.strip_prefix("batch_")?)
}

/// Public id of an uploaded file.
fn public_file(file_id: Uuid) -> String {
    format!("file-{}", file_id.simple())
}

/// OpenAI file object of an uploaded batch input file.
fn file_object(row: &GatewayBatchFileRow) -> GatewayFile {
    GatewayFile {
        id: public_file(row.file_id),
        object: "file".to_owned(),
        bytes: row.size_bytes,
        created_at: row.created_at.timestamp(),
        filename: row.filename.clone(),
        purpose: "batch".to_owned(),
    }
}

/// Provider batch `object` of `row` with every id replaced by its public id.
fn public_batch(row: &GatewayBatchRow, object: &Value) -> Value {
    let mut object = object.clone();
    if let Some(members) = object.as_object_mut() {
        let batch = row.batch_id.simple();
        members.insert("id".to_owned(), json!(format!("batch_{batch}")));
        members.insert("input_file_id".to_owned(), json!(public_file(row.file_id)));
        for (member, suffix) in [("output_file_id", "output"), ("error_file_id", "error")] {
            if members.get(member).is_some_and(Value::is_string) {
                members.insert(member.to_owned(), json!(format!("file-{batch}-{suffix}")));
            }
        }
    }
    object
}

/// Provider ids and exact `<provider>/<model>` projections the caller's
/// gateway invoke grants reach, or `None` when a grant reaches every model.
///
/// Only prunes the batch listing: a model outside both lists can never pass
/// the audited invoke decision, which stays authoritative for the rest.
fn invocable(caller: &Caller) -> Option<(Vec<String>, Vec<String>)> {
    let (mut providers, mut models) = (Vec::new(), Vec::new());
    for permission in caller
        .principal
        .effective_permissions
        .iter()
        .filter(|permission| {
            permission.resource.covers(&Resource::Gateway)
                && permission.action.covers(&Action::Invoke)
        })
    {
        match &permission.scope {
            PermissionScope::All => return None,
            PermissionScope::Gateway(GatewayAccess::Provider { provider }) => {
                providers.push(provider.as_str().to_owned());
            }
            PermissionScope::Gateway(GatewayAccess::Model { provider, model }) => {
                models.push(format!("{}/{}", provider.as_str(), model.as_str()));
            }
            PermissionScope::Bifrost(_) => {}
        }
    }
    Some((providers, models))
}

/// Governed Batches call of `action` for the exact `model`, pinned to
/// `deployment` when it continues provider state.
fn request(
    model: ModelRef,
    deployment: Option<ProviderDeploymentName>,
    action: BatchAction,
    body: Value,
) -> GatewayCallRequest {
    GatewayCallRequest {
        operation: GatewayOperation::Batches,
        ingress: IngressDialect::OpenAi,
        model,
        fallback: None,
        body,
        media: None,
        batch: Some(action),
        deployment,
        stream: false,
        usage_bound: None,
        timeout: BATCH_CALL_TIMEOUT,
    }
}

/// Stable absence of a tenant file or batch.
fn missing(resource: &str, id: &str) -> WyrdError {
    WyrdError::GatewayResourceNotFound {
        message: format!("{resource} {id} not found in tenant"),
        details: json!({ "resource": resource, "name": id }),
    }
}

/// A provider success that is not the expected JSON object.
fn unexpected() -> WyrdError {
    WyrdError::GatewayUpstreamUnavailable {
        message: "the provider answered without the expected object".to_owned(),
        details: json!({}),
    }
}

/// Decodes a provider success body as JSON.
///
/// # Errors
/// Returns `GatewayUpstreamUnavailable` for a body that is not JSON.
fn provider_object(body: ResponseBody) -> Result<Value, WyrdError> {
    match body {
        ResponseBody::Json(raw) => serde_json::from_str(raw.get()).map_err(|_| unexpected()),
        ResponseBody::Media(_) | ResponseBody::Events(_) | ResponseBody::MediaStream { .. } => {
            Err(unexpected())
        }
    }
}

/// Model and deployment a stored row pins.
///
/// # Errors
/// Returns `Internal` when the stored names are invalid.
fn pinned(model: &str, deployment: &str) -> Result<(ModelRef, ProviderDeploymentName), WyrdError> {
    Ok((
        ModelRef::from_projection(model).map_err(internal)?,
        ProviderDeploymentName::new(deployment).map_err(internal)?,
    ))
}

/// Dependency-owning handle for governed Batches operations.
pub struct GatewayBatches<'a> {
    /// Server state carrying storage and the governed invocation pipeline.
    state: &'a AppState,
}

impl<'a> GatewayBatches<'a> {
    /// Borrows Batches dependencies from server state.
    #[must_use]
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    /// Uploads one batch input file from a `multipart/form-data` body.
    ///
    /// The form carries `purpose=batch` and exactly one `file`, validated as
    /// batch JSONL before any provider work. The file uploads as a governed
    /// call for the model its lines name; the serving deployment is recorded
    /// with the file's size, SHA-256, and content type so every later action
    /// reaches it. An upload whose row cannot be stored leaves the provider
    /// file unreferenced.
    ///
    /// # Errors
    /// Returns `GatewayInvalidRequest` naming `body`, `purpose`, `file`, or the
    /// offending line member; every error of [`GatewayInvocation::invoke`];
    /// `GatewayUpstreamUnavailable` when the provider answers without a file
    /// id; and `ServiceUnavailable` when storage fails.
    pub async fn upload(
        &self,
        caller: &Caller,
        content_type: Option<&str>,
        bytes: Bytes,
    ) -> Result<BatchAnswer, WyrdError> {
        let limit = bytes.len();
        let (fields, mut files) = FormReader::new(Body::from(bytes), limit)
            .decode(content_type, FileSink::Memory)
            .await?;
        if fields.get("purpose").and_then(Value::as_str) != Some("batch") {
            return Err(invalid_request(GatewayContractError::new(
                "purpose",
                "must be batch",
            )));
        }
        let file = match (files.pop(), files.is_empty()) {
            (Some(file), true) if file.field == "file" => file,
            _ => {
                return Err(invalid_request(GatewayContractError::new(
                    "file",
                    "must be exactly one uploaded file",
                )));
            }
        };
        let UploadContent::Bytes(content) = &file.content else {
            return Err(invalid_request(GatewayContractError::new(
                "file",
                "must be decoded into memory",
            )));
        };
        let input = BatchInput::parse(content).map_err(invalid_request)?;
        let (model, endpoint) = (input.model.clone(), input.endpoint.clone());
        let action = BatchAction::UploadFile {
            filename: file.filename.clone(),
            input,
        };
        let response = self
            .call(
                caller,
                request(model, None, action, json!({ "purpose": "batch" })),
                false,
                &self.state.shutdown_token,
            )
            .await?;
        if response.status != 200 {
            return Ok(response.into());
        }
        let resolved = response.resolved.to_string();
        let deployment = response.deployment.clone().ok_or_else(unexpected)?;
        let object = provider_object(response.body)?;
        let upstream = object
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(unexpected)?;
        let row = GatewayBatchFileRow {
            file_id: Uuid::now_v7(),
            filename: file.filename,
            size_bytes: i64::try_from(content.len()).map_err(internal)?,
            sha256: STANDARD.encode(Sha256::digest(content)),
            content_type: file.content_type,
            endpoint,
            model: resolved,
            deployment: deployment.as_str().to_owned(),
            upstream_file_id: upstream.to_owned(),
            created_at: Utc::now(),
        };
        let mut conn = self.conn(caller.data_tenant_id).await?;
        let row = insert_gateway_batch_file(&mut conn, &row)
            .await
            .map_err(unavailable)?;
        conn.commit().await.map_err(unavailable)?;
        BatchAnswer::object(&file_object(&row))
    }

    /// Creates a batch from an OpenAI create request, at most once per
    /// canonical request.
    ///
    /// Invoke permission for the file's model is decided and audited once,
    /// before anything is claimed. The request's RFC 8785 canonical digest is
    /// then claimed in tenant Postgres before the provider is called. A replay
    /// of a created batch returns it refreshed under that decision; a replay
    /// while the first creation is pending is refused as a conflict. Creation
    /// runs on the input file's deployment with the provider's file id, in
    /// tracked server work that settles the claim as [`Self::settle`]
    /// describes.
    ///
    /// # Cancellation
    /// Dropping this future cancels the creation call, which starts no
    /// successor attempt; the tracked work still releases the claim when no
    /// attempt began dispatch and keeps it pending when one did.
    ///
    /// # Errors
    /// Returns `GatewayInvalidRequest` for a missing `input_file_id` or an
    /// `endpoint` other than the file's, `GatewayResourceNotFound` for an
    /// unknown file, `GatewayResourceConflict` for a pending replay, the
    /// permission denial and every error of [`GatewayInvocation::invoke`],
    /// `GatewayUpstreamUnavailable` when the provider answers without a batch
    /// id, and `ServiceUnavailable` or `Internal` on storage failure.
    pub async fn create(&self, caller: &Caller, body: Value) -> Result<BatchAnswer, WyrdError> {
        let tenant = caller.data_tenant_id;
        let input = body
            .get("input_file_id")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                invalid_request(GatewayContractError::new(
                    "input_file_id",
                    "must be a file id",
                ))
            })?
            .to_owned();
        let Some(FileId::Uploaded(file_id)) = FileId::parse(&input) else {
            return Err(missing("file", &input));
        };
        let file = self
            .file_row(tenant, file_id)
            .await?
            .ok_or_else(|| missing("file", &input))?;
        if body.get("endpoint").and_then(Value::as_str) != Some(file.endpoint.as_str()) {
            return Err(invalid_request(GatewayContractError::new(
                "endpoint",
                "must be the endpoint every input file request targets",
            )));
        }
        let digest = hex::encode(Sha256::digest(serde_jcs::to_vec(&body).map_err(internal)?));
        let (model, deployment) = pinned(&file.model, &file.deployment)?;
        GatewayInvocation::new(self.state)
            .decide(caller, OPERATION, &model)
            .map_err(permission_deny_reason_to_wyrd)?;
        let claim = GatewayBatchRow {
            batch_id: Uuid::now_v7(),
            file_id,
            model: file.model,
            deployment: file.deployment,
            upstream_batch_id: None,
            batch: None,
        };
        let mut conn = self.conn(tenant).await?;
        let claimed = claim_gateway_batch(&mut conn, &claim, &digest)
            .await
            .map_err(unavailable)?;
        let existing = if claimed {
            None
        } else {
            gateway_batch_by_request(&mut conn, &digest)
                .await
                .map_err(unavailable)?
        };
        conn.commit().await.map_err(unavailable)?;
        if !claimed {
            if let Some(row) = existing.filter(|row| row.upstream_batch_id.is_some()) {
                return self
                    .refresh(caller, row, BatchAction::RetrieveBatch, true)
                    .await;
            }
            return Err(WyrdError::GatewayResourceConflict {
                message: "an identical batch creation is pending; vary metadata to create a distinct batch".to_owned(),
                details: json!({ "input_file_id": input }),
            });
        }

        let mut native = body;
        native["input_file_id"] = Value::String(file.upstream_file_id);
        let request = request(model, Some(deployment), BatchAction::CreateBatch, native);
        let cancel = self.state.shutdown_token.child_token();
        // Dropping this future cancels the call; the tracked task still
        // settles the claim from the call's dispatch evidence.
        let _cancel_on_drop = cancel.clone().drop_guard();
        let state = self.state.clone();
        let caller = caller.clone();
        self.state
            .gateway_tasks
            .spawn(async move {
                GatewayBatches::new(&state)
                    .settle(&caller, claim, request, &cancel)
                    .await
            })
            .await
            .map_err(unavailable)?
    }

    /// Creates the claimed batch and settles its claim.
    ///
    /// Runs on tracked server work so the claim settles even after the
    /// requester leaves: `cancel` stops the call without a successor attempt,
    /// and the claim is released only when no provider can have created the
    /// batch (no attempt began dispatch, or the provider refused below `500`),
    /// kept pending when one may have, and recorded with the provider batch on
    /// success. The invoke decision was already taken and audited by
    /// [`Self::create`].
    ///
    /// # Errors
    /// Returns every error of [`GatewayInvocation::invoke`],
    /// `GatewayUpstreamUnavailable` when the provider answers without a batch
    /// id, and `ServiceUnavailable` or `Internal` on storage failure.
    async fn settle(
        &self,
        caller: &Caller,
        claim: GatewayBatchRow,
        request: GatewayCallRequest,
        cancel: &CancellationToken,
    ) -> Result<BatchAnswer, WyrdError> {
        let tenant = caller.data_tenant_id;
        let response = match self.call(caller, request, true, cancel).await {
            Ok(response) => response,
            Err(failure) => {
                if !failure.dispatched {
                    self.release(tenant, claim.batch_id).await?;
                }
                return Err(failure.error);
            }
        };
        if response.status != 200 {
            // A provider 5xx may still have created the batch, so the claim
            // stays pending and identical replays conflict.
            if response.status < 500 {
                self.release(tenant, claim.batch_id).await?;
            }
            return Ok(response.into());
        }
        let object = provider_object(response.body)?;
        let upstream = object
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(unexpected)?;
        self.record(tenant, claim.batch_id, upstream, &object)
            .await?;
        BatchAnswer::object(&public_batch(&claim, &object))
    }

    /// Reads a batch, refreshing its stored status from the provider.
    ///
    /// # Errors
    /// Returns `GatewayResourceNotFound` for an unknown or pending batch, and
    /// the errors of [`Self::refresh`].
    pub async fn retrieve(&self, caller: &Caller, id: &str) -> Result<BatchAnswer, WyrdError> {
        let row = self.created(caller.data_tenant_id, id).await?;
        self.refresh(caller, row, BatchAction::RetrieveBatch, false)
            .await
    }

    /// Cancels a batch and stores the provider's resulting status.
    ///
    /// # Errors
    /// Returns `GatewayResourceNotFound` for an unknown or pending batch, and
    /// the errors of [`Self::refresh`].
    pub async fn cancel(&self, caller: &Caller, id: &str) -> Result<BatchAnswer, WyrdError> {
        let row = self.created(caller.data_tenant_id, id).await?;
        self.refresh(caller, row, BatchAction::CancelBatch, false)
            .await
    }

    /// Lists created batches newest first with their last observed status.
    ///
    /// One tenant read returns at most `limit + 1` batches, newest first,
    /// pruned to models the caller's invoke grants can reach; `has_more`
    /// reports that an eligible batch follows the page, and `after` is the
    /// last listed batch. Invoke permission is then decided and audited once
    /// per distinct returned model under `gateway.batches.list`, and batches
    /// of a denied model are omitted, so a page shrinks rather than scanning
    /// further when a decision denies what the grants reach.
    ///
    /// # Errors
    /// Returns `GatewayInvalidRequest` naming `limit` outside `1..=100` or an
    /// `after` that is not a batch id, and `ServiceUnavailable` or `Internal`
    /// on storage failure.
    pub async fn list(
        &self,
        caller: &Caller,
        after: Option<&str>,
        limit: Option<u32>,
    ) -> Result<BatchAnswer, WyrdError> {
        let limit = limit.unwrap_or(DEFAULT_LIST_LIMIT);
        if !(1..=MAX_LIST_LIMIT).contains(&limit) {
            return Err(invalid_request(GatewayContractError::new(
                "limit",
                "must be between 1 and 100",
            )));
        }
        let cursor = after
            .map(|raw| {
                batch_id(raw).ok_or_else(|| {
                    invalid_request(GatewayContractError::new("after", "must be a batch id"))
                })
            })
            .transpose()?;
        let eligible = invocable(caller);
        let mut conn = self.conn(caller.data_tenant_id).await?;
        let rows = gateway_batches(
            &mut conn,
            cursor,
            i64::from(limit) + 1,
            eligible
                .as_ref()
                .map(|(providers, models)| (providers.as_slice(), models.as_slice())),
        )
        .await
        .map_err(unavailable)?;
        conn.commit().await.map_err(unavailable)?;
        let has_more = rows.len() > limit as usize;
        let invocation = GatewayInvocation::new(self.state);
        let mut verdicts: BTreeMap<&str, bool> = BTreeMap::new();
        let mut data = Vec::new();
        for row in rows.iter().take(limit as usize) {
            let visible = match verdicts.get(row.model.as_str()) {
                Some(visible) => *visible,
                None => {
                    let model = ModelRef::from_projection(&row.model).map_err(internal)?;
                    let visible = invocation
                        .decide(caller, "gateway.batches.list", &model)
                        .is_ok();
                    verdicts.insert(&row.model, visible);
                    visible
                }
            };
            if let Some(object) = row.batch.as_ref().filter(|_| visible) {
                data.push(public_batch(row, object));
            }
        }
        let id = |object: Option<&Value>| object.map(|object| object["id"].clone());
        BatchAnswer::object(&json!({
            "object": "list",
            "first_id": id(data.first()),
            "last_id": id(data.last()),
            "has_more": has_more,
            "data": data,
        }))
    }

    /// Reads an uploaded file's stored metadata.
    ///
    /// Invoke permission for the file's model is decided and audited under
    /// `gateway.files.read`.
    ///
    /// # Errors
    /// Returns `GatewayResourceNotFound` for an unknown id or a batch output
    /// or error id, the permission denial, and `ServiceUnavailable` on storage
    /// failure.
    pub async fn file(&self, caller: &Caller, id: &str) -> Result<BatchAnswer, WyrdError> {
        let row = self.uploaded(caller.data_tenant_id, id).await?;
        let model = ModelRef::from_projection(&row.model).map_err(internal)?;
        GatewayInvocation::new(self.state)
            .decide(caller, "gateway.files.read", &model)
            .map_err(permission_deny_reason_to_wyrd)?;
        BatchAnswer::object(&file_object(&row))
    }

    /// Relays a file's content from the deployment holding it.
    ///
    /// An uploaded file's content is its provider copy with native models; a
    /// batch output or error id resolves through the batch's last observed
    /// provider object, so it exists once a read or cancel observed it. The
    /// content is bounded by the dispatch response limit.
    ///
    /// # Errors
    /// Returns `GatewayResourceNotFound` for an unknown id or a file not yet
    /// observed, and every error of [`GatewayInvocation::invoke`].
    pub async fn file_content(&self, caller: &Caller, id: &str) -> Result<BatchAnswer, WyrdError> {
        let tenant = caller.data_tenant_id;
        let (model, deployment, upstream) = match FileId::parse(id) {
            Some(FileId::Uploaded(_)) => {
                let row = self.uploaded(tenant, id).await?;
                (row.model, row.deployment, row.upstream_file_id)
            }
            Some(FileId::Output(batch) | FileId::Error(batch)) => {
                let member = if id.ends_with("-output") {
                    "output_file_id"
                } else {
                    "error_file_id"
                };
                let row = self
                    .batch_row(tenant, batch)
                    .await?
                    .ok_or_else(|| missing("file", id))?;
                let upstream = row
                    .batch
                    .as_ref()
                    .and_then(|object| object.get(member))
                    .and_then(Value::as_str)
                    .ok_or_else(|| missing("file", id))?
                    .to_owned();
                (row.model, row.deployment, upstream)
            }
            None => return Err(missing("file", id)),
        };
        let (model, deployment) = pinned(&model, &deployment)?;
        let response = self
            .call(
                caller,
                request(
                    model,
                    Some(deployment),
                    BatchAction::FileContent(upstream),
                    json!({}),
                ),
                false,
                &self.state.shutdown_token,
            )
            .await?;
        Ok(response.into())
    }

    /// Deletes an uploaded file from its deployment, then its stored row.
    ///
    /// A provider refusal is relayed and keeps the row. Batches created from
    /// the file keep their records.
    ///
    /// # Errors
    /// Returns `GatewayResourceNotFound` for an unknown id or a batch output
    /// or error id, every error of [`GatewayInvocation::invoke`], and
    /// `ServiceUnavailable` on storage failure.
    pub async fn delete_file(&self, caller: &Caller, id: &str) -> Result<BatchAnswer, WyrdError> {
        let tenant = caller.data_tenant_id;
        let row = self.uploaded(tenant, id).await?;
        let (model, deployment) = pinned(&row.model, &row.deployment)?;
        let response = self
            .call(
                caller,
                request(
                    model,
                    Some(deployment),
                    BatchAction::DeleteFile(row.upstream_file_id),
                    json!({}),
                ),
                false,
                &self.state.shutdown_token,
            )
            .await?;
        if response.status != 200 {
            return Ok(response.into());
        }
        let mut conn = self.conn(tenant).await?;
        delete_gateway_batch_file(&mut conn, row.file_id)
            .await
            .map_err(unavailable)?;
        conn.commit().await.map_err(unavailable)?;
        BatchAnswer::object(&GatewayFileDeleted {
            id: id.to_owned(),
            object: "file".to_owned(),
            deleted: true,
        })
    }

    /// Runs `action` on a created batch `row` and stores the provider's
    /// resulting batch object; `authorized` reuses an invoke decision the
    /// caller already took and audited for the batch's model.
    ///
    /// # Errors
    /// Returns every error of [`GatewayInvocation::invoke`],
    /// `GatewayUpstreamUnavailable` for a success that is not JSON, and
    /// `ServiceUnavailable` or `Internal` on storage failure.
    async fn refresh(
        &self,
        caller: &Caller,
        row: GatewayBatchRow,
        action: fn(String) -> BatchAction,
        authorized: bool,
    ) -> Result<BatchAnswer, WyrdError> {
        let upstream = row
            .upstream_batch_id
            .clone()
            .ok_or_else(|| internal("a refreshed batch has no provider id"))?;
        let (model, deployment) = pinned(&row.model, &row.deployment)?;
        let response = self
            .call(
                caller,
                request(model, Some(deployment), action(upstream.clone()), json!({})),
                authorized,
                &self.state.shutdown_token,
            )
            .await?;
        if response.status != 200 {
            return Ok(response.into());
        }
        let object = provider_object(response.body)?;
        self.record(caller.data_tenant_id, row.batch_id, &upstream, &object)
            .await?;
        BatchAnswer::object(&public_batch(&row, &object))
    }

    /// Runs one governed Batches `request` through [`GatewayInvocation::run`],
    /// skipping the invoke decision when `authorized` and cancelling with
    /// `cancel`.
    ///
    /// # Errors
    /// Returns every error of [`GatewayInvocation::invoke`], with whether a
    /// provider may have received the request.
    async fn call(
        &self,
        caller: &Caller,
        request: GatewayCallRequest,
        authorized: bool,
        cancel: &CancellationToken,
    ) -> Result<GatewayCallResponse, CallFailure> {
        GatewayInvocation::new(self.state)
            .run(caller, request, authorized, cancel)
            .await
    }

    /// Opens a tenant transaction.
    ///
    /// # Errors
    /// Returns `ServiceUnavailable` when storage fails.
    async fn conn(&self, tenant: DataTenantId) -> Result<TenantConn<'a>, WyrdError> {
        self.state
            .postgres
            .tenant_conn(tenant)
            .await
            .map_err(unavailable)
    }

    /// Reads the uploaded file a public `id` names.
    ///
    /// # Errors
    /// Returns `GatewayResourceNotFound` for any other id and
    /// `ServiceUnavailable` when storage fails.
    async fn uploaded(
        &self,
        tenant: DataTenantId,
        id: &str,
    ) -> Result<GatewayBatchFileRow, WyrdError> {
        let Some(FileId::Uploaded(file_id)) = FileId::parse(id) else {
            return Err(missing("file", id));
        };
        self.file_row(tenant, file_id)
            .await?
            .ok_or_else(|| missing("file", id))
    }

    /// Reads the created batch a public `id` names.
    ///
    /// # Errors
    /// Returns `GatewayResourceNotFound` for an unknown or pending batch and
    /// `ServiceUnavailable` when storage fails.
    async fn created(&self, tenant: DataTenantId, id: &str) -> Result<GatewayBatchRow, WyrdError> {
        let batch = batch_id(id).ok_or_else(|| missing("batch", id))?;
        self.batch_row(tenant, batch)
            .await?
            .filter(|row| row.upstream_batch_id.is_some())
            .ok_or_else(|| missing("batch", id))
    }

    /// Reads one stored file row.
    ///
    /// # Errors
    /// Returns `ServiceUnavailable` when storage fails.
    async fn file_row(
        &self,
        tenant: DataTenantId,
        file_id: Uuid,
    ) -> Result<Option<GatewayBatchFileRow>, WyrdError> {
        let mut conn = self.conn(tenant).await?;
        let row = gateway_batch_file(&mut conn, file_id)
            .await
            .map_err(unavailable)?;
        conn.commit().await.map_err(unavailable)?;
        Ok(row)
    }

    /// Reads one stored batch row, pending or created.
    ///
    /// # Errors
    /// Returns `ServiceUnavailable` when storage fails.
    async fn batch_row(
        &self,
        tenant: DataTenantId,
        batch_id: Uuid,
    ) -> Result<Option<GatewayBatchRow>, WyrdError> {
        let mut conn = self.conn(tenant).await?;
        let row = gateway_batch(&mut conn, batch_id)
            .await
            .map_err(unavailable)?;
        conn.commit().await.map_err(unavailable)?;
        Ok(row)
    }

    /// Stores a batch's provider id and latest provider object.
    ///
    /// # Errors
    /// Returns `ServiceUnavailable` when storage fails and `Internal` when the
    /// batch is gone or bound to another provider id.
    async fn record(
        &self,
        tenant: DataTenantId,
        batch_id: Uuid,
        upstream: &str,
        object: &Value,
    ) -> Result<(), WyrdError> {
        let mut conn = self.conn(tenant).await?;
        let recorded = record_gateway_batch(&mut conn, batch_id, upstream, object)
            .await
            .map_err(unavailable)?;
        conn.commit().await.map_err(unavailable)?;
        if recorded {
            Ok(())
        } else {
            Err(internal("a batch record does not match its provider id"))
        }
    }

    /// Releases a pending creation claim.
    ///
    /// # Errors
    /// Returns `ServiceUnavailable` when storage fails.
    async fn release(&self, tenant: DataTenantId, batch_id: Uuid) -> Result<(), WyrdError> {
        let mut conn = self.conn(tenant).await?;
        release_gateway_batch(&mut conn, batch_id)
            .await
            .map_err(unavailable)?;
        conn.commit().await.map_err(unavailable)
    }
}

#[cfg(test)]
mod tests {
    //! Public batch and file id parsing and provider id replacement.

    use serde_json::json;
    use uuid::Uuid;
    use wyrd_sql::row_types::gateway::GatewayBatchRow;

    use super::{FileId, batch_id, public_batch};

    /// Public ids parse only in their issued simple form, and a provider batch
    /// object leaves with every id public.
    ///
    /// # Panics
    ///
    /// Panics when an id parses wrongly or a provider id survives.
    #[test]
    fn public_ids_parse_strictly_and_replace_provider_ids() {
        let id = Uuid::now_v7();
        let simple = id.simple();
        assert_eq!(
            FileId::parse(&format!("file-{simple}")),
            Some(FileId::Uploaded(id))
        );
        assert_eq!(
            FileId::parse(&format!("file-{simple}-output")),
            Some(FileId::Output(id))
        );
        assert_eq!(
            FileId::parse(&format!("file-{simple}-error")),
            Some(FileId::Error(id))
        );
        assert_eq!(
            FileId::parse(&format!("file-{id}")),
            None,
            "hyphenated ids were never issued"
        );
        assert_eq!(FileId::parse("file-abc"), None);
        assert_eq!(batch_id(&format!("batch_{simple}")), Some(id));
        assert_eq!(batch_id(&format!("file-{simple}")), None);
        let row = GatewayBatchRow {
            batch_id: id,
            file_id: id,
            model: "acme/m".to_owned(),
            deployment: "d".to_owned(),
            upstream_batch_id: Some("b-up".to_owned()),
            batch: None,
        };
        let public = public_batch(
            &row,
            &json!({"id": "b-up", "input_file_id": "f-up", "output_file_id": "o-up", "error_file_id": null, "status": "completed"}),
        );
        assert_eq!(
            public,
            json!({
                "id": format!("batch_{simple}"),
                "input_file_id": format!("file-{simple}"),
                "output_file_id": format!("file-{simple}-output"),
                "error_file_id": null,
                "status": "completed",
            })
        );
    }
}
