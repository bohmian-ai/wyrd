//! Named publications of queued Verifier invocation attempts.
//!
//! [`Publications`] is the hosted backend a trusted caller inside one running
//! execution retains content through: a `/work` file or directory, or a bounded JSON value
//! under a logical name, scoped to its producer attempt. A `/work`
//! publication first decides and audits the accepted caller's File
//! permission, as every verification File operation does; a JSON value
//! creates no File. A call that already committed recovers its stored
//! receipt before touching scratch or storage.
//! Otherwise content is frozen under the workspace's writer lock, each file
//! stored as a verification File by the storage owner's validated
//! server-side put, its inventory fact recorded in the background, and only
//! then named by a manifest accepted into the
//! execution's [`RunState`] with each file's completed descriptor, charged
//! to the execution's cumulative publication budget, so a publication is
//! never visible with partial content, a writer that lost the state accepts
//! nothing, and readers locate its bytes without rediscovery. Every refusal is a [`ToolError`]
//! the calling Agent can read and correct.

use std::sync::Arc;

use serde_json::Value;
use skald_tool::ToolError;
use wyrd_spec::auth::Sha256Hex;
use wyrd_spec::error::WyrdError;
use wyrd_spec::storage::StoredObjectRef;
use wyrd_spec::verification::PublicationName;
use wyrd_spec::verification::execution::{
    Producer, Publication, PublicationContent, PublicationFile, PublicationKind,
    PublicationManifest,
};
use wyrd_storage::service::{StorageCaller, put_file};
use wyrd_storage::tenant_path;

use super::HostedLimits;
use super::tools::{invocation, structured};
use super::workspace::{Frozen, Workspace};
use crate::components::auth::Caller;
use crate::components::storage::inventory::ObjectInventory;
use crate::components::storage::routes::completed_file;
use crate::components::verification::service::VerificationControl;
use crate::state::AppState;
use crate::verification::state::{
    PublicationAcceptance, PublicationBudget, PublicationCall, PublicationLimit, RunState,
};

/// Audit operation of a `/work` publication's File decision.
const FILES: &str = "verification.publication.files";

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

/// The publication backend of one running execution.
pub struct Publications<'a> {
    /// Server state: storage, Postgres, and the object inventory.
    state: &'a AppState,
    /// The execution's accepted authority Files are completed as.
    caller: &'a Caller,
    /// The execution state every publication is accepted into.
    run: &'a RunState,
    /// The Deployment limits in force.
    limits: HostedLimits,
}

impl<'a> Publications<'a> {
    /// The backend of the execution whose state `run` writes, completing
    /// Files as `caller`.
    #[must_use]
    pub const fn new(
        state: &'a AppState,
        caller: &'a Caller,
        run: &'a RunState,
        limits: HostedLimits,
    ) -> Self {
        Self {
            state,
            caller,
            run,
            limits,
        }
    }

    /// Publish `value`, JSON `null` included, under `call`'s name.
    ///
    /// # Errors
    /// Returns a `publication_size` refusal for a value whose serialization
    /// exceeds the tool result limit, and [`Self::accept`]'s refusals.
    pub fn json(&self, call: &PublishCall, value: Value) -> Result<PublicationKind, ToolError> {
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
        self.accept(call, &arguments, PublicationContent::Json { value }, &[])
    }

    /// Publish the `/work` file or directory at `path` under `call`'s name:
    /// decide the accepted caller's File permission, as every other
    /// verification File operation does, then return the stored receipt
    /// when this call already committed it, otherwise freeze it, complete
    /// every file as a verification File, then commit its manifest. An
    /// interrupted or refused publication commits nothing.
    ///
    /// # Errors
    /// Returns the [`WyrdError::PermissionDeniedRbac`] refusal before any
    /// File work when the caller holds no `verifier:run` grant,
    /// [`Workspace::freeze`]'s refusals, a `publication_size` refusal for a
    /// file over the file limit, an invocation failure when a File cannot
    /// complete, and [`Self::accept`]'s refusals.
    pub async fn path(
        &self,
        call: &PublishCall,
        workspace: &Arc<Workspace>,
        path: &str,
    ) -> Result<PublicationKind, ToolError> {
        VerificationControl::new(self.state)
            .authorize_evidence(
                self.caller,
                FILES,
                &format!("publication:{}", call.name.as_str()),
            )
            .map_err(|error| structured(&error))?;
        let arguments = serde_json::json!({ "name": call.name, "path": path });
        let probe = PublicationContent::File {
            manifest: PublicationManifest {
                files: Vec::new(),
                directories: Vec::new(),
            },
        };
        if let Some(receipt) = self.receipt(call, &arguments, probe)? {
            return receipt;
        }
        let (files, directories, directory) = match workspace.freeze(path).await? {
            Frozen::File { name, bytes } => (vec![(name, bytes)], Vec::new(), false),
            Frozen::Directory { files, directories } => (files, directories, true),
        };
        let limit = self.limits.file_max_bytes;
        if let Some((relative, bytes)) = files.iter().find(|(_, bytes)| bytes.len() as u64 > limit)
        {
            return Err(over_limit(
                format!(
                    "{path}: {relative} is {} bytes, over the {limit}-byte file limit",
                    bytes.len()
                ),
                "path",
                format!("files of at most {limit} bytes"),
            ));
        }
        let mut retained = Vec::with_capacity(files.len());
        let mut objects = Vec::with_capacity(files.len());
        for (relative, bytes) in files {
            let sha256 = Sha256Hex::digest(&bytes);
            let size_bytes = bytes.len() as u64;
            objects.push(
                self.complete(sha256, bytes).await.map_err(|error| {
                    invocation(format!("{path} could not be retained: {error}"))
                })?,
            );
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
        self.accept(call, &arguments, content, &objects)
    }

    /// Store `bytes`, whose digest is `sha256`, as one verification File
    /// through the storage owner's validated server-side put, record it in
    /// the object inventory in the background, and return its verified
    /// descriptor.
    ///
    /// The inventory write never gates the publication: a failure is logged
    /// and the File's descriptor still names its bytes.
    ///
    /// # Errors
    /// Returns the storage failure or an inventory fact that cannot be built.
    async fn complete(
        &self,
        sha256: Sha256Hex,
        bytes: Vec<u8>,
    ) -> Result<StoredObjectRef, WyrdError> {
        let caller = StorageCaller {
            data_tenant_id: self.caller.data_tenant_id,
        };
        let file = put_file(&self.state.storage, &caller, &sha256, bytes).await?;
        let fact = completed_file(self.state, &file.upload_id, &file.stored)?;
        let (state, tenant, attribution) = (
            self.state.clone(),
            self.caller.data_tenant_id,
            self.caller.attribution(),
        );
        tokio::spawn(async move {
            if let Err(error) = ObjectInventory::new(&state)
                .record(tenant, attribution, &[fact])
                .await
            {
                tracing::warn!(%error, "a publication file's inventory fact was not recorded");
            }
        });
        Ok(file.stored)
    }

    /// The receipt `call` already holds, when it holds one: the accepted
    /// kind when this call published the name with these `arguments`, a
    /// `publication_name` refusal when another call holds it, or the lost
    /// state. `probe` stands in for content that is only identified, never
    /// stored.
    ///
    /// # Errors
    /// Returns an invocation failure when the arguments cannot serialize.
    fn receipt(
        &self,
        call: &PublishCall,
        arguments: &Value,
        probe: PublicationContent,
    ) -> Result<Option<Result<PublicationKind, ToolError>>, ToolError> {
        let digest = digest(arguments)?;
        let publication = self.publication(call, probe);
        let held = self.run.publication_receipt(PublicationCall {
            publication: &publication,
            call_id: &call.id,
            arguments: &digest,
            objects: &[],
            retained_bytes: 0,
        });
        Ok(held.map(|held| answer(call, held)))
    }

    /// Accept `content` under `call`'s producer and name with the digest of
    /// its canonical `arguments` and the completed descriptor of each of its
    /// manifest files into the execution's state, within its publication
    /// budget; the receipt answers from the live state, which the
    /// execution's background flush persists.
    ///
    /// # Errors
    /// Returns a `publication_name` refusal when another call already holds
    /// the name in this attempt, a `publication_count` or `publication_bytes`
    /// refusal when the execution's budget cannot hold it, and an invocation
    /// failure when the writer lost the state, its lease, or its deadline.
    fn accept(
        &self,
        call: &PublishCall,
        arguments: &Value,
        content: PublicationContent,
        objects: &[StoredObjectRef],
    ) -> Result<PublicationKind, ToolError> {
        let retained_bytes = match &content {
            PublicationContent::Json { value } => serde_json::to_vec(value)
                .map_err(|error| invocation(format!("the value cannot serialize: {error}")))?
                .len() as u64,
            PublicationContent::File { manifest } | PublicationContent::Directory { manifest } => {
                manifest.files.iter().map(|file| file.size_bytes).sum()
            }
        };
        let digest = digest(arguments)?;
        let publication = self.publication(call, content);
        let accepted = self.run.accept_publication(
            PublicationCall {
                publication: &publication,
                call_id: &call.id,
                arguments: &digest,
                objects,
                retained_bytes,
            },
            PublicationBudget {
                max_count: self.limits.publications_max,
                max_bytes: self.limits.files_max_bytes,
            },
        );
        if accepted == PublicationAcceptance::Accepted {
            return Ok(publication.content.kind());
        }
        answer(call, accepted)
    }

    /// `call`'s publication of `content`.
    fn publication(&self, call: &PublishCall, content: PublicationContent) -> Publication {
        Publication {
            name: call.name.clone(),
            producer: call.producer.clone(),
            content,
        }
    }
}

/// The digest of canonical `arguments`.
///
/// # Errors
/// Returns an invocation failure when they cannot serialize.
fn digest(arguments: &Value) -> Result<Sha256Hex, ToolError> {
    let canonical = serde_jcs::to_vec(arguments)
        .map_err(|error| invocation(format!("the arguments cannot serialize: {error}")))?;
    Ok(Sha256Hex::digest(&canonical))
}

/// A structured limit refusal of `field`, whose bound is `expected`.
fn over_limit(message: String, field: &str, expected: String) -> ToolError {
    let rule = match field {
        "count" => "publication_count",
        "bytes" => "publication_bytes",
        _ => "publication_size",
    };
    structured(&WyrdError::Validation {
        message,
        details: serde_json::json!({
            "rule": rule, "field": field, "expected": expected,
        }),
    })
}

/// What `call` answers for an acceptance that committed nothing new.
///
/// # Errors
/// Returns the refusal of a conflict, an exhausted budget, or a lost lease.
fn answer(call: &PublishCall, held: PublicationAcceptance) -> Result<PublicationKind, ToolError> {
    match held {
        PublicationAcceptance::Replayed(kind) => Ok(kind),
        PublicationAcceptance::Conflict => Err(structured(&WyrdError::Validation {
            message: format!(
                "the name {} is already published in this attempt, or this call already \
                 published another name or value; publish under another name in a new call",
                call.name.as_str()
            ),
            details: serde_json::json!({
                "rule": "publication_name", "field": "name", "value": call.name,
                "expected": "a name this attempt has not published",
            }),
        })),
        PublicationAcceptance::OverBudget(PublicationLimit::Count) => Err(over_limit(
            "the execution has accepted its most publications; nothing was published".to_owned(),
            "count",
            "fewer publications per execution".to_owned(),
        )),
        PublicationAcceptance::OverBudget(PublicationLimit::Bytes) => Err(over_limit(
            "the execution's publications would retain more than their byte limit; \
             nothing was published"
                .to_owned(),
            "bytes",
            "fewer retained bytes per execution".to_owned(),
        )),
        PublicationAcceptance::StaleLease | PublicationAcceptance::Accepted => Err(invocation(
            "the execution no longer holds its state, lease, or deadline; nothing was published"
                .to_owned(),
        )),
    }
}

/// The stored object holding accepted publication file `file` of `tenant`,
/// located by the completed File descriptor `object` its acceptance kept.
///
/// No inventory lookup or digest rediscovery is involved: the descriptor is
/// the authoritative location. It must name an evidence path of `tenant`
/// and the manifest file's exact digest and length; the reader still checks
/// the bytes against both.
///
/// # Errors
/// Returns [`WyrdError::Internal`] when the descriptor names another tenant,
/// a malformed path, or a different digest or length.
pub(crate) fn published_object(
    tenant: wyrd_spec::DataTenantId,
    file: &PublicationFile,
    object: &StoredObjectRef,
) -> Result<wyrd_storage::ValidatedPath, WyrdError> {
    use base64::Engine as _;
    let sha256 = base64::engine::general_purpose::STANDARD.encode(file.sha256.as_bytes());
    if object.sha256 != sha256 || object.size_bytes != file.size_bytes {
        return Err(WyrdError::internal(
            "a publication file's completed descriptor does not match its manifest",
        ));
    }
    tenant_path::validate_evidence(&object.storage_path, tenant).map_err(|error| {
        tracing::warn!(%error, "a publication file's completed descriptor is not tenant evidence");
        WyrdError::internal("a publication file's completed descriptor is not tenant evidence")
    })
}
