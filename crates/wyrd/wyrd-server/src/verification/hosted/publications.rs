//! Named publications of queued Verifier invocation attempts.
//!
//! [`Publications`] is the hosted backend a trusted caller inside one leased
//! execution retains content through: a `/work` file or directory, or a bounded JSON value
//! under a logical name, scoped to its producer attempt. Content is frozen
//! under the workspace's writer lock, each file completed as an ordinary verification
//! File through the shared storage lifecycle, and only then named by a
//! manifest committed under the execution's lease, so a publication is never
//! visible with partial content and a stale lease commits nothing. Every
//! refusal is a [`ToolError`] the calling Agent can read and correct.

use serde_json::Value;
use skald_tool::ToolError;
use wyrd_spec::auth::Sha256Hex;
use wyrd_spec::error::{WyrdError, storage::WyrdStorageError};
use wyrd_spec::verification::PublicationName;
use wyrd_spec::verification::execution::{
    Producer, Publication, PublicationContent, PublicationFile, PublicationKind,
    PublicationManifest,
};
use wyrd_sql::queries::verifier_executions::{
    ExecutionLease, PublicationAcceptance, PublicationCall, VerifierExecutionQueue,
};
use wyrd_storage::service::{StorageCaller, file_upload_init, reconcile_upload};
use wyrd_storage::tenant_path;

use super::HostedLimits;
use super::tools::{invocation, structured};
use super::workspace::{Frozen, Workspace};
use crate::components::auth::Caller;
use crate::components::storage::inventory::ObjectInventory;
use crate::components::storage::routes::completed_file;
use crate::state::AppState;

/// One call asking to publish, as its producer attempt makes it.
#[derive(Debug, Clone)]
pub struct PublishCall {
    /// The call's identity, stable across its replays.
    pub id: String,
    /// The invocation attempt publishing.
    pub producer: Producer,
    /// The logical name to publish under.
    pub name: PublicationName,
}

/// The publication backend of one leased execution.
pub struct Publications<'a> {
    /// Server state: storage, Postgres, and the object inventory.
    state: &'a AppState,
    /// The execution's accepted authority Files are completed as.
    caller: &'a Caller,
    /// The lease every publication commits under.
    lease: ExecutionLease,
    /// The Deployment limits in force.
    limits: HostedLimits,
}

impl<'a> Publications<'a> {
    /// The backend of the execution `lease` holds, completing Files as
    /// `caller`.
    #[must_use]
    pub const fn new(
        state: &'a AppState,
        caller: &'a Caller,
        lease: ExecutionLease,
        limits: HostedLimits,
    ) -> Self {
        Self {
            state,
            caller,
            lease,
            limits,
        }
    }

    /// Publish `value`, JSON `null` included, under `call`'s name.
    ///
    /// # Errors
    /// Returns a `publication_size` refusal for a value whose serialization
    /// exceeds the tool result limit, and [`Self::accept`]'s refusals.
    pub async fn json(
        &self,
        call: &PublishCall,
        value: Value,
    ) -> Result<PublicationKind, ToolError> {
        // ponytail: JSON publications share the tool result bound, since an
        // Agent supplies them as tool arguments; larger JSON publishes as a file.
        let limit = self.limits.tool_result_max_bytes;
        let size = serde_json::to_vec(&value)
            .map_err(|error| invocation(format!("the value cannot serialize: {error}")))?
            .len();
        if size > limit {
            return Err(structured(&WyrdError::Validation {
                message: format!(
                    "the value is {size} bytes, over the {limit}-byte publication limit; \
                     write it to a /work file and publish its path"
                ),
                details: serde_json::json!({
                    "rule": "publication_size", "field": "value",
                    "value": size, "expected": format!("at most {limit} bytes"),
                }),
            }));
        }
        let arguments = serde_json::json!({ "name": call.name, "value": value });
        self.accept(call, &arguments, PublicationContent::Json { value })
            .await
    }

    /// Publish the `/work` file or directory at `path` under `call`'s name:
    /// freeze it, complete every file as a verification File, then commit
    /// its manifest. An interrupted or refused publication commits nothing.
    ///
    /// # Errors
    /// Returns [`Workspace::freeze`]'s refusals, an invocation failure when
    /// a File cannot complete, and [`Self::accept`]'s refusals.
    pub async fn path(
        &self,
        call: &PublishCall,
        workspace: &Workspace,
        path: &str,
    ) -> Result<PublicationKind, ToolError> {
        let (files, directories, directory) = match workspace.freeze(path).await? {
            Frozen::File { name, bytes } => (vec![(name, bytes)], Vec::new(), false),
            Frozen::Directory { files, directories } => (files, directories, true),
        };
        let mut retained = Vec::with_capacity(files.len());
        for (relative, bytes) in files {
            let sha256 = Sha256Hex::digest(&bytes);
            let size_bytes = bytes.len() as u64;
            self.complete(sha256, bytes)
                .await
                .map_err(|error| invocation(format!("{path} could not be retained: {error}")))?;
            retained.push(PublicationFile {
                path: relative,
                sha256,
                size_bytes,
            });
        }
        let manifest = PublicationManifest {
            files: retained,
            directories,
        };
        let content = if directory {
            PublicationContent::Directory { manifest }
        } else {
            PublicationContent::File { manifest }
        };
        let arguments = serde_json::json!({ "name": call.name, "path": path });
        self.accept(call, &arguments, content).await
    }

    /// Complete `bytes`, whose digest is `sha256`, as one verification File
    /// through the shared storage lifecycle and record it in the object
    /// inventory.
    ///
    /// # Errors
    /// Returns the storage, verification, or inventory failure.
    async fn complete(&self, sha256: Sha256Hex, bytes: Vec<u8>) -> Result<(), WyrdError> {
        let storage = StorageCaller {
            data_tenant_id: self.caller.data_tenant_id,
        };
        let postgres = self.state.postgres.wyrd();
        let init = file_upload_init(
            &self.state.storage,
            postgres,
            &storage,
            &sha256,
            bytes.len() as u64,
        )
        .await?;
        let path = tenant_path::evidence(self.caller.data_tenant_id, &sha256, &init.upload_id);
        self.state
            .storage
            .put_object(&path, bytes)
            .await
            .map_err(|error| WyrdError::from(WyrdStorageError::from(error)))?;
        let completed = reconcile_upload(
            &self.state.storage,
            postgres,
            &storage,
            init.upload_id.clone(),
        )
        .await?;
        let fact = completed_file(self.state, &init.upload_id, &completed.stored)?;
        ObjectInventory::new(self.state)
            .record(
                self.caller.data_tenant_id,
                self.caller.attribution(),
                &[fact],
            )
            .await
    }

    /// Commit `content` under `call`'s producer and name with the digest of
    /// its canonical `arguments`, under the execution's lease.
    ///
    /// # Errors
    /// Returns a `publication_name` refusal when another call already holds
    /// the name in this attempt, and an invocation failure when the lease is
    /// no longer held or the commit fails.
    async fn accept(
        &self,
        call: &PublishCall,
        arguments: &Value,
        content: PublicationContent,
    ) -> Result<PublicationKind, ToolError> {
        let kind = content.kind();
        let canonical = serde_jcs::to_vec(arguments)
            .map_err(|error| invocation(format!("the arguments cannot serialize: {error}")))?;
        let publication = Publication {
            name: call.name.clone(),
            producer: call.producer.clone(),
            content,
        };
        let unavailable = |error: &dyn std::fmt::Display| {
            invocation(format!("the publication could not commit: {error}"))
        };
        let mut conn = self
            .state
            .postgres
            .tenant_conn(self.caller.data_tenant_id)
            .await
            .map_err(|error| unavailable(&error))?;
        let accepted = VerifierExecutionQueue::default()
            .accept_publication(
                &mut conn,
                self.lease,
                PublicationCall {
                    publication: &publication,
                    call_id: &call.id,
                    arguments: &Sha256Hex::digest(&canonical),
                },
            )
            .await
            .map_err(|error| unavailable(&error))?;
        match accepted {
            PublicationAcceptance::Accepted => {
                conn.commit().await.map_err(|error| unavailable(&error))?;
                Ok(kind)
            }
            PublicationAcceptance::Replayed => Ok(kind),
            PublicationAcceptance::Conflict => Err(structured(&WyrdError::Validation {
                message: format!(
                    "the name {} is already published in this attempt; publish under another name",
                    call.name.as_str()
                ),
                details: serde_json::json!({
                    "rule": "publication_name", "field": "name", "value": call.name,
                    "expected": "a name this attempt has not published",
                }),
            })),
            PublicationAcceptance::StaleLease => Err(invocation(
                "the execution no longer holds its lease; nothing was published".to_owned(),
            )),
        }
    }
}
