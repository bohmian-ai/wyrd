//! Typed, pod-local live-tail reads over writable and immutable memtable data.
//!
//! The Scribe reader owns no WAL or SQL access. Its local transport preserves
//! shallow Arrow batches; its private tonic transport authenticates with the
//! already-issued workload bearer and returns owned decoded frames.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use arrow::ipc::reader::StreamReader;
use async_trait::async_trait;
use chrono::Utc;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::vala::api as tail;
use wyrd_tonic::tonic::metadata::MetadataValue;
use wyrd_tonic::tonic::{Request, transport::Channel};
use wyrd_tonic::wyrd::v1::scribe_tail_service_client::ScribeTailServiceClient;

use crate::catalog::TenantTableBinding;
use crate::catalog::layout::TimePartition;
use crate::contracts::ScribeError;
use crate::scribe::memtable::{Memtable, ReadableBatchLimits};
use crate::scribe::routing::shard_for;
use crate::scribe::shards::ScribeShardRuntime;
use crate::scribe::stream_identity::StreamIdentity;
use crate::scribe::wal::WalLsn;

/// The only tail protocol revision understood by the Scribe v1 reader.
pub const TAIL_PROTOCOL_VERSION: u16 = 1;

/// Simultaneous descriptor backing reserved for each planned hot batch.
///
/// A shard result vector coexists with the runtime's merged hot vector; the
/// merged vector later coexists with its retained-fence replacement. The
/// larger of those two second vectors is charged with the hot source layout
/// before any descriptor vector is allocated.
const TAIL_BATCH_DESCRIPTOR_BYTES: usize = std::mem::size_of::<HotBatch>() * 2;
/// Ensures the charged second hot vector also covers its retained replacement.
const _: () = assert!(std::mem::size_of::<HotBatch>() >= std::mem::size_of::<RetainedBatch>());

/// Operation audience carried by a private Scribe-tail ticket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TailTicketAudience {
    /// Metadata-only active-stream discovery.
    List,
    /// One-shot exact-fence allocation.
    Acquire,
    /// Reusable retained-fence page access.
    Page,
    /// Idempotent retained-fence release.
    Release,
}

/// Transport-neutral claims signed by the server tail authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TailTicketClaims {
    /// Query identity owning the private operation.
    pub query_id: uuid::Uuid,
    /// Tenant bound by the authenticated query context.
    pub tenant_id: DataTenantId,
    /// Canonical tenant table name.
    pub canonical_table: String,
    /// Exact Scribe node and writer epoch selected by discovery.
    pub node_id: uuid::Uuid,
    /// Current writer epoch for stale-incarnation rejection.
    pub writer_epoch: u64,
    /// Absolute query deadline in UTC.
    pub deadline: chrono::DateTime<chrono::Utc>,
    /// Narrow operation audience.
    pub audience: TailTicketAudience,
    /// Single-use replay identity retained through expiry.
    pub nonce: Vec<u8>,
}

impl TailTicketClaims {
    /// Validates the signed claims against one operation's exact request tuple.
    ///
    /// # Errors
    /// Returns [`TailReadError::Authorization`] when query, tenant, table,
    /// stream, epoch, or deadline differs from the signed request.
    pub fn validate_binding(
        &self,
        query_id: uuid::Uuid,
        tenant_id: DataTenantId,
        canonical_table: &str,
        node_id: uuid::Uuid,
        writer_epoch: u64,
        deadline: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), TailReadError> {
        if self.query_id != query_id
            || self.tenant_id != tenant_id
            || self.canonical_table != canonical_table
            || self.node_id != node_id
            || self.writer_epoch != writer_epoch
            || self.deadline < deadline
        {
            return Err(TailReadError::Authorization {
                detail: "tail ticket binding is invalid".to_owned(),
            });
        }
        Ok(())
    }
}

/// Exact request tuple a signed tail ticket must authorize.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TailTicketBinding {
    /// Query identity selected by Oracle.
    pub query_id: uuid::Uuid,
    /// Tenant identity selected by the authenticated request.
    pub tenant_id: DataTenantId,
    /// Canonical table selected by the catalog cut.
    pub canonical_table: String,
    /// Scribe node selected by discovery.
    pub node_id: uuid::Uuid,
    /// Writer epoch selected by discovery.
    pub writer_epoch: u64,
    /// Request deadline the ticket must cover.
    pub deadline: chrono::DateTime<chrono::Utc>,
}

/// Reusable exact-fence capability returned by a successful acquire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TailFenceCapability {
    /// Opaque signed capability bytes.
    pub encoded: Vec<u8>,
    /// Exact retained fence identity bound by the signer.
    pub fence_id: tail::TailFenceId,
}

/// Narrow signer for private list/acquire tickets and returned fence capabilities.
pub trait TailTicketMinter: Send + Sync {
    /// Signs one operation-scoped claim set.
    ///
    /// # Errors
    /// Returns a closed authority error when claims cannot be encoded or signed.
    fn mint_tail_ticket(&self, claims: &TailTicketClaims) -> Result<Vec<u8>, TailReadError>;

    /// Signs a reusable exact-fence capability after allocation.
    ///
    /// # Errors
    /// Returns a closed authority error when the capability cannot be encoded
    /// or signed.
    fn mint_tail_capability(
        &self,
        _claims: &TailTicketClaims,
        _fence: &tail::TailReadFence,
    ) -> Result<Vec<u8>, TailReadError> {
        Err(TailReadError::Authorization {
            detail: "tail authority does not mint fence capabilities".to_owned(),
        })
    }
}

/// Durable audit collaborator required before returning a tail authorization
/// rejection.  The server maps these narrow reasons to its audit catalog.
#[async_trait]
pub trait TailSecurityAudit: Send + Sync {
    /// Records a failure whose tenant claim is not trusted.
    ///
    /// # Errors
    /// Returns [`TailReadError`] when the durable audit append is unavailable.
    async fn append_unverified_tail_rejection(&self, reason: &str) -> Result<(), TailReadError>;

    /// Records a failure after the signed tenant claim is verified.
    ///
    /// # Errors
    /// Returns [`TailReadError`] when the durable audit append is unavailable.
    async fn append_verified_tail_violation(
        &self,
        tenant_id: DataTenantId,
        reason: &str,
    ) -> Result<(), TailReadError>;
}

/// No-op audit used only by isolated transport tests.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopTailSecurityAudit;

#[async_trait]
impl TailSecurityAudit for NoopTailSecurityAudit {
    /// Accepts the isolated unverified rejection.
    ///
    /// # Errors
    /// This test-only sink never fails.
    async fn append_unverified_tail_rejection(&self, _reason: &str) -> Result<(), TailReadError> {
        Ok(())
    }

    /// Accepts the isolated tenant violation.
    ///
    /// # Errors
    /// This test-only sink never fails.
    async fn append_verified_tail_violation(
        &self,
        _tenant_id: DataTenantId,
        _reason: &str,
    ) -> Result<(), TailReadError> {
        Ok(())
    }
}

/// Narrow verifier for private tickets and exact-fence capabilities.
#[async_trait]
pub trait TailTicketVerifier: Send + Sync {
    /// Records a rejection whose signed tenant tuple is not trusted.
    ///
    /// # Errors
    /// Returns [`TailReadError`] when durable audit cannot append the rejection.
    async fn audit_unverified_rejection(&self, _reason: &str) -> Result<(), TailReadError> {
        Ok(())
    }

    /// Records a rejection after a signed tenant tuple has been decoded.
    ///
    /// # Errors
    /// Returns [`TailReadError`] when durable audit cannot append the rejection.
    async fn audit_verified_violation(
        &self,
        _tenant_id: DataTenantId,
        _reason: &str,
    ) -> Result<(), TailReadError> {
        Ok(())
    }

    /// Validates one decoded ticket against the exact operation tuple.
    ///
    /// # Errors
    /// Returns [`TailReadError::Authorization`] for any tuple mismatch or
    /// [`TailReadError`] when the rejection audit fails.
    async fn verify_tail_ticket_binding(
        &self,
        claims: &TailTicketClaims,
        binding: &TailTicketBinding,
    ) -> Result<(), TailReadError> {
        claims.validate_binding(
            binding.query_id,
            binding.tenant_id,
            &binding.canonical_table,
            binding.node_id,
            binding.writer_epoch,
            binding.deadline,
        )
    }

    /// Verifies signature, key, expiry, audience, and consumes the nonce before
    /// the caller checks operation-specific bindings.
    async fn verify_tail_ticket_unbound(
        &self,
        _encoded: &[u8],
        _audience: TailTicketAudience,
    ) -> Result<TailTicketClaims, TailReadError> {
        Err(TailReadError::Authorization {
            detail: "tail authority does not expose unbound claims".to_owned(),
        })
    }

    /// Decodes a capability's signed owner tuple before exact fence checks.
    async fn decode_tail_capability(
        &self,
        _encoded: &[u8],
        _audience: TailTicketAudience,
    ) -> Result<(uuid::Uuid, DataTenantId, String), TailReadError> {
        Err(TailReadError::Authorization {
            detail: "tail authority does not decode capabilities".to_owned(),
        })
    }

    /// Verifies claims before Scribe state is listed or mutated.
    ///
    /// # Errors
    /// Returns [`TailReadError`] for signature, audience, expiry, binding,
    /// epoch, replay, or audit failures.
    async fn verify_tail_ticket(
        &self,
        encoded: &[u8],
        expected: &TailTicketClaims,
    ) -> Result<(), TailReadError>;

    /// Verifies a reusable capability against an exact retained fence tuple.
    ///
    /// # Errors
    /// Returns [`TailReadError`] when the capability is expired, malformed,
    /// cross-query, cross-tenant, or bound to another fence.
    async fn verify_tail_capability(
        &self,
        encoded: &[u8],
        query_id: uuid::Uuid,
        tenant_id: DataTenantId,
        canonical_table: &str,
        fence: &tail::TailReadFence,
        audience: TailTicketAudience,
    ) -> Result<(), TailReadError>;
}

/// Maximum expired fences reclaimed while servicing one foreground operation.
const OPPORTUNISTIC_EXPIRY_LIMIT: usize = 64;

/// Bounds retained Scribe fences and each page returned from one immutable interval.
#[derive(Debug, Clone, Copy)]
pub struct TailFenceConfig {
    /// Default upper bound for a fence lifetime.
    pub ttl: Duration,
    /// Maximum number of simultaneously retained fences.
    pub max_fences: usize,
    /// Maximum Arrow payload ceiling shared by pending and retained fences.
    pub max_retained_bytes: usize,
    /// Maximum rows returned by one page irrespective of a caller's request.
    pub max_page_rows: u32,
    /// Maximum Arrow IPC bytes returned by one page irrespective of a caller's request.
    pub max_page_encoded_bytes: u32,
}

impl Default for TailFenceConfig {
    fn default() -> Self {
        Self {
            ttl: Duration::from_secs(30),
            max_fences: 256,
            max_retained_bytes: 512 * 1024 * 1024,
            max_page_rows: 4_096,
            max_page_encoded_bytes: 16 * 1024 * 1024,
        }
    }
}

/// A local page whose Arrow batches retain shallow ownership of Scribe arrays.
#[derive(Debug, Clone)]
pub struct LocalTailPage {
    /// Shallow row batches returned from the frozen interval.
    pub batches: Vec<Arc<arrow::record_batch::RecordBatch>>,
    /// The exact final row cursor when the interval has more rows.
    pub next: Option<tail::TailCursor>,
    /// Whether the interval is exhausted after this page.
    pub complete: bool,
}

/// One active partition scope returned by private Scribe discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveTailStream {
    /// Exact partition whose live rows remain in Scribe memory.
    pub time_partition: tail::TimePartitionWire,
    /// Exact Scribe stream incarnation serving that partition.
    pub stream: tail::TailStreamIdentity,
}

/// Fence metadata plus the Scribe-minted reusable capability returned by acquire.
#[derive(Debug, Clone)]
pub struct TailFenceLease {
    /// Exact retained interval metadata.
    pub fence: tail::TailReadFence,
    /// Signed capability bound to the retained fence tuple.
    pub capability: Vec<u8>,
}

/// Tail-read boundary with local shallow and remote owned-frame implementations.
#[async_trait]
pub trait TailReadTransport: Send + Sync {
    /// Lists active event-day scopes for one tenant/table without allocating a
    /// fence.
    async fn list_active_streams(
        &self,
        binding: tail::TenantTableBinding,
        _query_id: uuid::Uuid,
        _ticket: Vec<u8>,
    ) -> Result<Vec<ActiveTailStream>, TailReadError> {
        let _ = binding;
        Err(TailReadError::State {
            detail: "tail transport does not support active-stream discovery".to_owned(),
        })
    }

    /// Acquires metadata for one immutable tail interval.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError`] when the transport cannot acquire the requested
    /// fence or Scribe rejects its validation bounds.
    async fn acquire_fence(
        &self,
        request: tail::AcquireTailFenceRequest,
    ) -> Result<tail::TailReadFence, TailReadError>;

    /// Acquires and returns the Scribe-issued capability for the exact fence.
    async fn acquire_fence_with_capability(
        &self,
        request: tail::AcquireTailFenceRequest,
        ticket: Vec<u8>,
    ) -> Result<TailFenceLease, TailReadError> {
        Ok(TailFenceLease {
            fence: self.acquire_fence_with_ticket(request, ticket).await?,
            capability: Vec::new(),
        })
    }

    /// Authorized variant used by production query-scoped discovery.
    async fn acquire_fence_with_ticket(
        &self,
        request: tail::AcquireTailFenceRequest,
        _ticket: Vec<u8>,
    ) -> Result<tail::TailReadFence, TailReadError> {
        self.acquire_fence(request).await
    }

    /// Reads one locally consumable row-precise page.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError`] when the retained interval is unavailable or a
    /// requested bound cannot be satisfied.
    async fn read_page(
        &self,
        request: tail::TailPageRequest,
    ) -> Result<LocalTailPage, TailReadError>;

    /// Authorized page variant bound to one exact reusable capability.
    async fn read_page_with_capability(
        &self,
        request: tail::TailPageRequest,
        _capability: Vec<u8>,
    ) -> Result<LocalTailPage, TailReadError> {
        self.read_page(request).await
    }

    /// Idempotently releases a retained interval.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError::State`] when the local registry cannot be safely
    /// accessed.
    fn release_fence(&self, fence_id: tail::TailFenceId) -> Result<FenceRelease, TailReadError>;

    /// Awaits release completion when the transport has an asynchronous
    /// lifecycle (for example, an authenticated remote RPC).
    async fn release_fence_async(
        &self,
        fence_id: tail::TailFenceId,
    ) -> Result<FenceRelease, TailReadError> {
        self.release_fence(fence_id)
    }

    /// Authorized release variant retaining idempotent capability semantics.
    async fn release_fence_with_capability(
        &self,
        _query_id: uuid::Uuid,
        fence_id: tail::TailFenceId,
        _capability: Vec<u8>,
    ) -> Result<FenceRelease, TailReadError> {
        self.release_fence_async(fence_id).await
    }
}

/// In-process transport that preserves Scribe's shallow Arrow ownership.
#[derive(Clone)]
pub struct LocalTailReadTransport {
    /// The one Scribe-owned reader that retains interval state.
    reader: Arc<ScribeTailReader>,
    /// Optional verifier enforcing the same private ticket contract as tonic.
    authority: Option<Arc<dyn TailTicketVerifier>>,
    /// Server-owned signer used to return the capability after local retention.
    minter: Option<Arc<dyn TailTicketMinter>>,
}

impl std::fmt::Debug for LocalTailReadTransport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalTailReadTransport")
            .field("authority_configured", &self.authority.is_some())
            .finish_non_exhaustive()
    }
}

impl LocalTailReadTransport {
    /// Wraps the local Scribe reader without introducing an IPC encode/decode hop.
    #[must_use]
    pub fn new(reader: Arc<ScribeTailReader>) -> Self {
        Self {
            reader,
            authority: None,
            minter: None,
        }
    }

    /// Wraps the local reader with the server-owned tail verifier.
    #[must_use]
    pub fn with_authority(
        reader: Arc<ScribeTailReader>,
        authority: Arc<dyn TailTicketVerifier>,
    ) -> Self {
        Self {
            reader,
            authority: Some(authority),
            minter: None,
        }
    }

    /// Wraps the local reader with verifier and signer capabilities.
    #[must_use]
    pub fn with_authority_and_minter(
        reader: Arc<ScribeTailReader>,
        authority: Arc<dyn TailTicketVerifier>,
        minter: Arc<dyn TailTicketMinter>,
    ) -> Self {
        Self {
            reader,
            authority: Some(authority),
            minter: Some(minter),
        }
    }
}

#[async_trait]
impl TailReadTransport for LocalTailReadTransport {
    /// Delegates metadata-only active-stream discovery to Scribe.
    async fn list_active_streams(
        &self,
        binding: tail::TenantTableBinding,
        query_id: uuid::Uuid,
        ticket: Vec<u8>,
    ) -> Result<Vec<ActiveTailStream>, TailReadError> {
        if let Some(authority) = &self.authority {
            let claims = authority
                .verify_tail_ticket_unbound(&ticket, TailTicketAudience::List)
                .await?;
            let stream = self.reader.stream_identity();
            let canonical = canonical_table_name(&binding.namespace, &binding.table);
            authority
                .verify_tail_ticket_binding(
                    &claims,
                    &TailTicketBinding {
                        query_id,
                        tenant_id: binding.tenant_id,
                        canonical_table: canonical,
                        node_id: stream.node_id.as_uuid(),
                        writer_epoch: u64::try_from(stream.writer_epoch.as_i64()).unwrap_or(0),
                        deadline: claims.deadline,
                    },
                )
                .await?;
        }
        self.reader.list_active_streams(&binding).map(|streams| {
            streams
                .into_iter()
                .map(|(time_partition, stream)| ActiveTailStream {
                    time_partition,
                    stream,
                })
                .collect()
        })
    }

    /// Delegates metadata-only acquisition to the Scribe-owned reader.
    async fn acquire_fence(
        &self,
        request: tail::AcquireTailFenceRequest,
    ) -> Result<tail::TailReadFence, TailReadError> {
        self.reader.acquire_fence(request).await
    }

    /// Reads a local page only after exact capability verification.
    async fn read_page_with_capability(
        &self,
        request: tail::TailPageRequest,
        capability: Vec<u8>,
    ) -> Result<LocalTailPage, TailReadError> {
        if let Some(authority) = &self.authority {
            let (_cap_query_id, tenant, table) = authority
                .decode_tail_capability(&capability, TailTicketAudience::Page)
                .await?;
            let fence = match self.reader.fence_metadata(request.fence_id) {
                Ok(fence) => fence,
                Err(error) => {
                    authority.audit_verified_violation(tenant, "fence").await?;
                    return Err(error);
                }
            };
            authority
                .verify_tail_capability(
                    &capability,
                    request.query_id,
                    tenant,
                    &table,
                    &fence,
                    TailTicketAudience::Page,
                )
                .await?;
            return self.reader.read_page_for_tenant(tenant, request);
        }
        self.reader.read_page(&request)
    }

    /// Delegates acquisition after validating the explicit signed ticket.
    async fn acquire_fence_with_ticket(
        &self,
        request: tail::AcquireTailFenceRequest,
        ticket: Vec<u8>,
    ) -> Result<tail::TailReadFence, TailReadError> {
        if let Some(authority) = &self.authority {
            let claims = authority
                .verify_tail_ticket_unbound(&ticket, TailTicketAudience::Acquire)
                .await?;
            let stream = self.reader.stream_identity();
            let canonical =
                canonical_table_name(&request.binding.namespace, &request.binding.table);
            authority
                .verify_tail_ticket_binding(
                    &claims,
                    &TailTicketBinding {
                        query_id: request.query_id,
                        tenant_id: request.binding.tenant_id,
                        canonical_table: canonical,
                        node_id: stream.node_id.as_uuid(),
                        writer_epoch: u64::try_from(stream.writer_epoch.as_i64()).unwrap_or(0),
                        deadline: request.deadline,
                    },
                )
                .await?;
        }
        self.reader.acquire_fence(request).await
    }

    /// Acquires local metadata and returns a signer-produced capability.
    async fn acquire_fence_with_capability(
        &self,
        request: tail::AcquireTailFenceRequest,
        ticket: Vec<u8>,
    ) -> Result<TailFenceLease, TailReadError> {
        let claims = if let Some(authority) = &self.authority {
            let claims = authority
                .verify_tail_ticket_unbound(&ticket, TailTicketAudience::Acquire)
                .await?;
            let stream = self.reader.stream_identity();
            let canonical =
                canonical_table_name(&request.binding.namespace, &request.binding.table);
            authority
                .verify_tail_ticket_binding(
                    &claims,
                    &TailTicketBinding {
                        query_id: request.query_id,
                        tenant_id: request.binding.tenant_id,
                        canonical_table: canonical,
                        node_id: stream.node_id.as_uuid(),
                        writer_epoch: u64::try_from(stream.writer_epoch.as_i64()).unwrap_or(0),
                        deadline: request.deadline,
                    },
                )
                .await?;
            Some(claims)
        } else {
            None
        };
        let fence = self.reader.acquire_fence(request).await?;
        let capability = match (&self.minter, claims) {
            (Some(minter), Some(claims)) => minter.mint_tail_capability(&claims, &fence)?,
            _ => Vec::new(),
        };
        Ok(TailFenceLease { fence, capability })
    }

    /// Delegates local shallow page reads to the Scribe-owned reader.
    async fn read_page(
        &self,
        request: tail::TailPageRequest,
    ) -> Result<LocalTailPage, TailReadError> {
        self.reader.read_page(&request)
    }

    /// Delegates idempotent local release to the Scribe-owned reader.
    fn release_fence(&self, fence_id: tail::TailFenceId) -> Result<FenceRelease, TailReadError> {
        self.reader.release_fence(fence_id)
    }

    /// Releases a local fence only after exact capability verification.
    async fn release_fence_with_capability(
        &self,
        query_id: uuid::Uuid,
        fence_id: tail::TailFenceId,
        capability: Vec<u8>,
    ) -> Result<FenceRelease, TailReadError> {
        if let Some(authority) = &self.authority {
            let (_cap_query_id, tenant, table) = authority
                .decode_tail_capability(&capability, TailTicketAudience::Page)
                .await?;
            let fence = match self.reader.fence_metadata(fence_id) {
                Ok(fence) => fence,
                Err(error) => {
                    authority.audit_verified_violation(tenant, "fence").await?;
                    return Err(error);
                }
            };
            authority
                .verify_tail_capability(
                    &capability,
                    query_id,
                    tenant,
                    &table,
                    &fence,
                    TailTicketAudience::Page,
                )
                .await?;
            return self.reader.release_fence_for_tenant(tenant, fence_id);
        }
        self.reader.release_fence(fence_id)
    }
}

/// Private tonic tail client that turns owned IPC frames back into Arrow batches.
///
/// The adapter is deliberately separate from [`TailReadTransport`]: local fence
/// release is synchronous, while a remote release is an authenticated RPC. Both
/// expose the same typed domain requests and row-precise page shape.
#[derive(Debug, Clone)]
pub struct TonicTailReadTransport {
    /// Cloneable tonic client over one configured private Scribe endpoint.
    client: ScribeTailServiceClient<Channel>,
    /// Already-issued service-workload bearer sent on every private RPC.
    access_token: MetadataValue<wyrd_tonic::tonic::metadata::Ascii>,
}

/// Remote tail-page ceiling including protobuf framing overhead.
const TAIL_RPC_MAX_MESSAGE_BYTES: usize = 32 * 1024 * 1024 + 64 * 1024;

impl TonicTailReadTransport {
    /// Discovers active event-day scopes through the private Scribe RPC.
    ///
    /// # Errors
    /// Returns [`TailReadError::State`] for transport, conversion, or server
    /// status failures.
    pub async fn list_active_streams(
        &self,
        binding: tail::TenantTableBinding,
        query_id: uuid::Uuid,
        ticket: Vec<u8>,
    ) -> Result<Vec<ActiveTailStream>, TailReadError> {
        let mut client = self.client.clone();
        let response = client
            .list_active_streams(self.authenticated_request(
                wyrd_tonic::wyrd::v1::ListActiveStreamsRequest {
                    tail_ticket: ticket,
                    binding: Some(binding.into()),
                    query_id: query_id.as_bytes().to_vec(),
                },
            ))
            .await
            .map_err(|status| tonic_error(&status))?;
        response
            .into_inner()
            .streams
            .into_iter()
            .map(|stream| {
                let time_partition: tail::TimePartitionWire = stream
                    .time_partition
                    .ok_or(TailReadError::Binding)?
                    .try_into()
                    .map_err(|_| TailReadError::Binding)?;
                let stream = stream.stream.ok_or_else(|| TailReadError::State {
                    detail: "active tail stream omitted identity".to_owned(),
                })?;
                let node_id =
                    uuid::Uuid::parse_str(&stream.node_id).map_err(|_| TailReadError::State {
                        detail: "active tail stream node id is invalid".to_owned(),
                    })?;
                Ok(ActiveTailStream {
                    time_partition,
                    stream: tail::TailStreamIdentity {
                        node_id: tail::NodeId::new(node_id),
                        writer_epoch: stream.writer_epoch,
                    },
                })
            })
            .collect()
    }

    /// Creates a remote transport using the authenticated private Scribe channel.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError::State`] when the bearer cannot be represented as
    /// gRPC metadata.
    pub fn new(
        client: ScribeTailServiceClient<Channel>,
        bearer: &str,
    ) -> Result<Self, TailReadError> {
        let access_token =
            MetadataValue::try_from(format!("Bearer {bearer}").as_str()).map_err(|_| {
                TailReadError::State {
                    detail: "tail bearer cannot be represented as gRPC metadata".to_owned(),
                }
            })?;
        Ok(Self {
            client: client.max_decoding_message_size(TAIL_RPC_MAX_MESSAGE_BYTES),
            access_token,
        })
    }

    /// Acquires immutable remote fence metadata with the workload bearer attached.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError::State`] for transport/status/conversion failures.
    pub async fn acquire_fence(
        &self,
        request: tail::AcquireTailFenceRequest,
    ) -> Result<tail::TailReadFence, TailReadError> {
        self.acquire_fence_with_ticket(request, Vec::new()).await
    }

    /// Acquires immutable remote fence metadata with an explicit signed ticket.
    pub async fn acquire_fence_with_ticket(
        &self,
        request: tail::AcquireTailFenceRequest,
        ticket: Vec<u8>,
    ) -> Result<tail::TailReadFence, TailReadError> {
        let mut client = self.client.clone();
        let mut message: wyrd_tonic::wyrd::v1::AcquireTailFenceRequest = request.into();
        message.tail_ticket = ticket;
        let response = client
            .acquire_fence(self.authenticated_request(message))
            .await
            .map_err(|status| tonic_error(&status))?;
        response.into_inner().try_into().map_err(
            |error: wyrd_tonic::private_conversion::PrivateConversionError| TailReadError::State {
                detail: error.to_string(),
            },
        )
    }

    /// Acquires metadata and preserves the Scribe-issued reusable capability.
    pub async fn acquire_fence_with_capability(
        &self,
        request: tail::AcquireTailFenceRequest,
        ticket: Vec<u8>,
    ) -> Result<TailFenceLease, TailReadError> {
        let mut client = self.client.clone();
        let mut message: wyrd_tonic::wyrd::v1::AcquireTailFenceRequest = request.into();
        message.tail_ticket = ticket;
        let response = client
            .acquire_fence(self.authenticated_request(message))
            .await
            .map_err(|status| tonic_error(&status))?;
        let response = response.into_inner();
        let capability = response.capability.clone();
        let fence = response.try_into().map_err(
            |error: wyrd_tonic::private_conversion::PrivateConversionError| TailReadError::State {
                detail: error.to_string(),
            },
        )?;
        Ok(TailFenceLease { fence, capability })
    }

    /// Reads one remote page and decodes its owned Arrow IPC frames.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError::State`] for transport, conversion, or IPC decode
    /// failures.
    pub async fn read_page(
        &self,
        request: tail::TailPageRequest,
    ) -> Result<LocalTailPage, TailReadError> {
        self.read_page_with_capability(request, Vec::new()).await
    }

    /// Reads one remote page with an explicit reusable exact-fence capability.
    pub async fn read_page_with_capability(
        &self,
        request: tail::TailPageRequest,
        capability: Vec<u8>,
    ) -> Result<LocalTailPage, TailReadError> {
        let mut client = self.client.clone();
        let mut message: wyrd_tonic::wyrd::v1::TailPageRequest = request.into();
        message.tail_capability = capability;
        let response = client
            .read_fence_page(self.authenticated_request(message))
            .await
            .map_err(|status| tonic_error(&status))?;
        let page: tail::TailPage = response.into_inner().try_into().map_err(
            |error: wyrd_tonic::private_conversion::PrivateConversionError| TailReadError::State {
                detail: error.to_string(),
            },
        )?;
        let batches = page
            .batches
            .iter()
            .map(|bytes| decode_batch(bytes))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(LocalTailPage {
            batches,
            next: page.next,
            complete: page.complete,
        })
    }

    /// Releases a remote fence idempotently with the workload bearer attached.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError::State`] when the private RPC fails.
    pub async fn release_fence(&self, fence_id: tail::TailFenceId) -> Result<(), TailReadError> {
        self.release_fence_with_capability(uuid::Uuid::nil(), fence_id, Vec::new())
            .await
            .map(|_| ())
    }

    /// Releases a remote fence with an explicit reusable capability.
    pub async fn release_fence_with_capability(
        &self,
        query_id: uuid::Uuid,
        fence_id: tail::TailFenceId,
        capability: Vec<u8>,
    ) -> Result<FenceRelease, TailReadError> {
        let mut client = self.client.clone();
        let mut message: wyrd_tonic::wyrd::v1::ReleaseTailFenceRequest =
            tail::ReleaseTailFenceRequest { query_id, fence_id }.into();
        message.tail_capability = capability;
        client
            .release_fence(self.authenticated_request(message))
            .await
            .map_err(|status| tonic_error(&status))?;
        Ok(FenceRelease { released: true })
    }

    /// Adds the required private workload credential before a remote lookup.
    fn authenticated_request<T>(&self, message: T) -> Request<T> {
        let mut request = Request::new(message);
        request
            .metadata_mut()
            .insert("x-wyrd-access-token", self.access_token.clone());
        request
    }
}

#[async_trait]
impl TailReadTransport for TonicTailReadTransport {
    /// Discovers active streams through the authenticated private RPC.
    async fn list_active_streams(
        &self,
        binding: tail::TenantTableBinding,
        query_id: uuid::Uuid,
        ticket: Vec<u8>,
    ) -> Result<Vec<ActiveTailStream>, TailReadError> {
        TonicTailReadTransport::list_active_streams(self, binding, query_id, ticket).await
    }

    /// Acquire a remote immutable fence through the authenticated tonic client.
    async fn acquire_fence(
        &self,
        request: tail::AcquireTailFenceRequest,
    ) -> Result<tail::TailReadFence, TailReadError> {
        TonicTailReadTransport::acquire_fence(self, request).await
    }

    /// Acquires through the explicit signed ticket path.
    async fn acquire_fence_with_ticket(
        &self,
        request: tail::AcquireTailFenceRequest,
        ticket: Vec<u8>,
    ) -> Result<tail::TailReadFence, TailReadError> {
        TonicTailReadTransport::acquire_fence_with_ticket(self, request, ticket).await
    }

    /// Acquires the Scribe-issued capability without synthesizing one in Oracle.
    async fn acquire_fence_with_capability(
        &self,
        request: tail::AcquireTailFenceRequest,
        ticket: Vec<u8>,
    ) -> Result<TailFenceLease, TailReadError> {
        TonicTailReadTransport::acquire_fence_with_capability(self, request, ticket).await
    }

    /// Read and decode one remote owned-frame page.
    async fn read_page(
        &self,
        request: tail::TailPageRequest,
    ) -> Result<LocalTailPage, TailReadError> {
        TonicTailReadTransport::read_page(self, request).await
    }

    /// Reads through the explicit signed capability path.
    async fn read_page_with_capability(
        &self,
        request: tail::TailPageRequest,
        capability: Vec<u8>,
    ) -> Result<LocalTailPage, TailReadError> {
        TonicTailReadTransport::read_page_with_capability(self, request, capability).await
    }

    /// Declines synchronous remote release from a drop boundary.
    ///
    /// Lifecycle owners call [`Self::release_fence_async`] while they can await
    /// the authenticated RPC. Abandoned intervals rely on the bounded Scribe
    /// TTL instead of spawning detached cleanup from `Drop`.
    fn release_fence(&self, fence_id: tail::TailFenceId) -> Result<FenceRelease, TailReadError> {
        let _ = fence_id;
        Ok(FenceRelease { released: false })
    }

    /// Awaits the authenticated remote release RPC; no detached cleanup task is created.
    async fn release_fence_async(
        &self,
        fence_id: tail::TailFenceId,
    ) -> Result<FenceRelease, TailReadError> {
        TonicTailReadTransport::release_fence_with_capability(
            self,
            uuid::Uuid::nil(),
            fence_id,
            Vec::new(),
        )
        .await
    }

    /// Releases through the explicit signed capability path.
    async fn release_fence_with_capability(
        &self,
        query_id: uuid::Uuid,
        fence_id: tail::TailFenceId,
        capability: Vec<u8>,
    ) -> Result<FenceRelease, TailReadError> {
        TonicTailReadTransport::release_fence_with_capability(self, query_id, fence_id, capability)
            .await
    }
}

/// Converts an authenticated remote status into the local reader error shape.
fn tonic_error(status: &wyrd_tonic::tonic::Status) -> TailReadError {
    TailReadError::State {
        detail: status.to_string(),
    }
}

/// Decodes exactly one owned Arrow IPC batch received from the private service.
///
/// # Errors
///
/// Returns [`TailReadError::State`] for malformed IPC, an empty frame, or a
/// frame that carries more than the protocol's one batch.
fn decode_batch(bytes: &[u8]) -> Result<Arc<arrow::record_batch::RecordBatch>, TailReadError> {
    let mut reader =
        StreamReader::try_new(Cursor::new(bytes), None).map_err(|error| TailReadError::State {
            detail: format!("tail IPC decode failed: {error}"),
        })?;
    let batch = reader
        .next()
        .transpose()
        .map_err(|error| TailReadError::State {
            detail: format!("tail IPC read failed: {error}"),
        })?
        .ok_or_else(|| TailReadError::State {
            detail: "tail IPC frame contains no batch".to_owned(),
        })?;
    if reader.next().is_some() {
        return Err(TailReadError::State {
            detail: "tail IPC frame contains more than one batch".to_owned(),
        });
    }
    Ok(Arc::new(batch))
}

/// Idempotent outcome of releasing a tail fence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FenceRelease {
    /// Whether this call removed a retained fence and freed its capacity.
    pub released: bool,
}

/// Summary of one bounded expiry pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpiryReport {
    /// Number of expired fences removed during this pass.
    pub released: usize,
    /// Number of fences still retained after this pass.
    pub retained: usize,
}

/// Errors local to tail retention and paging before tonic maps them to statuses.
#[derive(Debug, thiserror::Error)]
pub enum TailReadError {
    /// A private ticket or capability failed cryptographic or tuple validation.
    #[error("tail authorization failed: {detail}")]
    Authorization { detail: String },
    /// The requested tail protocol is not served by this Scribe.
    #[error("unsupported tail protocol version {version}")]
    UnsupportedProtocol { version: u16 },
    /// The query deadline cannot retain a live interval.
    #[error("tail fence deadline has elapsed")]
    DeadlineElapsed,
    /// The provided binding cannot map to this Scribe's logical table owner.
    #[error("invalid tail binding")]
    Binding,
    /// The authenticated tenant does not own the retained fence binding.
    #[error("authenticated tenant cannot access this tail fence")]
    AccessDenied,
    /// A cursor names a different writer epoch than the retained stream.
    #[error("tail cursor writer epoch does not match the stream")]
    WriterEpochMismatch,
    /// A cursor is outside the immutable fence interval.
    #[error("tail cursor is outside the fence interval")]
    CursorOutOfRange,
    /// The observed live schema does not match the planned schema fingerprint.
    #[error("tail schema fingerprint does not match")]
    SchemaMismatch,
    /// The bounded fence registry cannot retain another interval.
    #[error("tail fence capacity is exhausted")]
    Capacity,
    /// A single row cannot be represented inside the configured response ceiling.
    #[error("a single tail row exceeds the encoded page ceiling")]
    OversizeRow,
    /// Arrow IPC encoding failed while calculating a page bound.
    #[error("tail Arrow IPC encoding failed: {detail}")]
    Encode { detail: String },
    /// Scribe state could not produce a consistent shallow snapshot.
    #[error("tail Scribe state failed: {detail}")]
    State { detail: String },
}

/// Concrete Scribe-owned reader that freezes one bounded live interval at a time.
#[derive(Debug)]
pub struct ScribeTailReader {
    /// Existing Scribe state reader used only to take a shallow point-in-time snapshot.
    source: Arc<FetchLiveTailService>,
    /// Retention and page ceilings enforced before caller-provided values.
    config: TailFenceConfig,
    /// Mutable retained fences and aggregate shallow-memory accounting.
    fences: Mutex<FenceRegistry>,
}

/// Converts the private wire binding into the established local table owner.
///
/// # Errors
///
/// Returns [`TailReadError::Binding`] when the namespace, table, or tenant cannot
/// form one valid local tenant/table binding.
fn binding_from_wire(
    binding: &tail::TenantTableBinding,
) -> Result<TenantTableBinding, TailReadError> {
    let namespace = crate::namespaces::BifrostNamespace::from_domain_namespace(&binding.namespace)
        .ok_or(TailReadError::Binding)?;
    TenantTableBinding::resolve((
        binding.tenant_id,
        crate::catalog::TableRef::new(namespace, &binding.table),
    ))
    .map_err(|_| TailReadError::Binding)
}

/// Builds the domain-qualified table identity used by signed private tickets.
fn canonical_table_name(namespace: &str, table: &str) -> String {
    if namespace.starts_with("vala.") {
        format!("{namespace}.{table}")
    } else {
        format!("vala.{namespace}.{table}")
    }
}

/// Converts the validated wire partition into the local memtable partition key.
///
/// Both representations already guarantee an exact unit boundary, so the
/// conversion is infallible and exists only to keep the transport type out of
/// the memtable layer.
fn partition_from_wire(time_partition: tail::TimePartitionWire) -> TimePartition {
    TimePartition::from_wire(time_partition)
}

/// Verifies that retained batches use the schema selected during planning.
///
/// An empty interval has no data schema to compare, so it is safely retained as
/// empty; the binding's catalog owner has already validated the requested schema
/// before it asks Scribe for the live cut.
///
/// # Errors
///
/// Returns [`TailReadError::SchemaMismatch`] when any shallow batch differs from
/// the required planning fingerprint.
fn validate_schema(
    batches: &[RetainedBatch],
    expected: &tail::SchemaFingerprint,
) -> Result<(), TailReadError> {
    for batch in batches {
        let actual = hex::encode(
            crate::contracts::projected_source_schema_fingerprint(batch.rows.schema().as_ref()).0,
        );
        if actual != expected.as_str() {
            return Err(TailReadError::SchemaMismatch);
        }
    }
    Ok(())
}

/// Returns the inclusive live edge of the retained snapshot.
///
/// # Errors
///
/// Returns [`TailReadError::State`] when persisted row identity is invalid, or
/// a cursor comparison error when the retained writer epoch is inconsistent.
fn inclusive_cursor(
    exclusive: &tail::TailCursor,
    writer_epoch: u64,
    batches: &[RetainedBatch],
) -> Result<tail::TailCursor, TailReadError> {
    let mut inclusive = exclusive.clone();
    for batch in batches {
        let ordinals =
            crate::schema::managed_columns::row_ordinals(batch.rows.as_ref()).map_err(|error| {
                TailReadError::State {
                    detail: format!("retained row identity invariant failed: {error}"),
                }
            })?;
        if let Some(row) = batch.rows.num_rows().checked_sub(1) {
            let ordinal = u32::try_from(ordinals.value(row)).map_err(|_| TailReadError::State {
                detail: "retained row ordinal is negative".to_owned(),
            })?;
            let candidate = tail::TailCursor {
                writer_epoch,
                wal_lsn: batch.lsn.as_u64(),
                batch_id: batch.batch_id,
                row_ordinal: ordinal,
            };
            if cursor_cmp(&candidate, &inclusive)? == std::cmp::Ordering::Greater {
                inclusive = candidate;
            }
        }
    }
    Ok(inclusive)
}

/// Returns the append identity a fence cursor orders one hot batch by.
///
/// An in-memory batch is exactly one append, so its own identity is the answer.
/// A staged batch is not: its member merged many appends, and the cursor must
/// name the identity of the last row it actually returns, which the managed
/// `wyrd_batch_id` column carries per row.
///
/// # Errors
///
/// Returns [`TailReadError::State`] when a staged batch carries no usable
/// managed batch identity, because a cursor derived from anything else would
/// not be resumable.
fn cursor_batch_id(batch: &HotBatch) -> Result<uuid::Uuid, TailReadError> {
    match batch.origin {
        HotBatchSource::Append { batch_id } => Ok(uuid::Uuid::from_bytes(batch_id)),
        HotBatchSource::StagedMember { member, .. } => {
            last_row_batch_id(&batch.rows).ok_or(TailReadError::State {
                detail: format!(
                    "staged member {}-{} returned rows without a managed batch identity",
                    member.shard(),
                    member.generation()
                ),
            })
        }
    }
}

/// Reads the managed batch identity of a batch's last row, when it has one.
fn last_row_batch_id(rows: &arrow::record_batch::RecordBatch) -> Option<uuid::Uuid> {
    let row = rows.num_rows().checked_sub(1)?;
    let index = rows
        .schema()
        .index_of(wyrd_spec::vala::managed_columns::WYRD_BATCH_ID)
        .ok()?;
    let values = rows
        .column(index)
        .as_any()
        .downcast_ref::<arrow::array::FixedSizeBinaryArray>()?;
    if arrow::array::Array::is_null(values, row) {
        return None;
    }
    uuid::Uuid::from_slice(values.value(row)).ok()
}

/// Validates a continuation against the retained writer epoch and exact fence bounds.
///
/// # Errors
///
/// Returns [`TailReadError::WriterEpochMismatch`] for a different writer epoch
/// and [`TailReadError::CursorOutOfRange`] outside `(exclusive, inclusive]`.
fn validate_after(
    after: Option<&tail::TailCursor>,
    fence: &tail::TailReadFence,
) -> Result<(), TailReadError> {
    let Some(after) = after else {
        return Ok(());
    };
    if after.writer_epoch != fence.stream.writer_epoch {
        return Err(TailReadError::WriterEpochMismatch);
    }
    if cursor_cmp(after, &fence.exclusive_sealed)? == std::cmp::Ordering::Less
        || cursor_cmp(after, &fence.inclusive_live)? == std::cmp::Ordering::Greater
    {
        return Err(TailReadError::CursorOutOfRange);
    }
    Ok(())
}

/// Orders two cursors only after proving their writer epochs agree.
///
/// # Errors
///
/// Returns [`TailReadError::WriterEpochMismatch`] when the cursors name
/// different writer epochs.
fn cursor_cmp(
    left: &tail::TailCursor,
    right: &tail::TailCursor,
) -> Result<std::cmp::Ordering, TailReadError> {
    if left.writer_epoch != right.writer_epoch {
        return Err(TailReadError::WriterEpochMismatch);
    }
    Ok((left.wal_lsn, left.batch_id, left.row_ordinal).cmp(&(
        right.wal_lsn,
        right.batch_id,
        right.row_ordinal,
    )))
}

/// Counts one shallow row's exact canonical IPC stream before page admission.
///
/// # Errors
///
/// Returns [`TailReadError::Encode`] when the row is outside the canonical
/// scalar IPC subset or its exact stream length cannot be represented.
fn encoded_record_batch_bytes(
    batch: &arrow::record_batch::RecordBatch,
) -> Result<usize, TailReadError> {
    crate::scribe::fixed_ipc::FixedIpcPlan::count(batch)
        .map(|plan| plan.encoded_bytes())
        .map_err(|error| TailReadError::Encode {
            detail: error.to_string(),
        })
}

/// Encodes one admitted tail batch directly into one exact-capacity IPC owner.
///
/// The same fixed plan used by local page admission is recomputed from the
/// shallow batch and consumed by the transport adapter. No growable counting
/// or output buffer is created, and encoder divergence fails closed.
///
/// # Errors
///
/// Returns [`TailReadError::Encode`] when the batch is outside the canonical
/// scalar IPC subset, exact size planning fails, or materialized buffers no
/// longer match the immutable plan.
pub fn encode_tail_batch_exact(
    batch: &arrow::record_batch::RecordBatch,
) -> Result<Vec<u8>, TailReadError> {
    let plan = crate::scribe::fixed_ipc::FixedIpcPlan::count(batch).map_err(|error| {
        TailReadError::Encode {
            detail: error.to_string(),
        }
    })?;
    plan.encode(batch).map_err(|error| TailReadError::Encode {
        detail: error.to_string(),
    })
}

/// Bounded owner of pending and retained live-tail fences.
///
/// A pending slot excludes both its configured/current-root payload ceiling
/// and simultaneous descriptor capacity before materialization. Successful
/// acquisition atomically moves that root-backed owner into `retained`;
/// cancellation returns the pending slot and owner once. `retained_bytes` and
/// `pending_payload_bytes` are payload admission facts only—the lease stored by
/// each pending or retained fence is the authoritative owner through shrink,
/// cancellation, release, expiry, or registry shutdown.
#[derive(Debug)]
struct FenceRegistry {
    /// Fences currently retaining shallow Scribe Arrow arrays.
    retained: HashMap<tail::TailFenceId, RetainedFence>,
    /// Released ownership tombstones retained until the original fence expiry.
    /// A tombstone makes duplicate release idempotent without reopening state.
    released: HashMap<tail::TailFenceId, (Instant, tail::TailReadFence)>,
    /// Aggregate Arrow payload bytes tracked for retained-fence admission.
    retained_bytes: usize,
    /// Fence acquisitions that own a slot while materialization is in flight.
    pending_fences: usize,
    /// Aggregate payload-ceiling portion reserved by pending acquisitions.
    pending_payload_bytes: usize,
    /// Fixed scalar lifecycle observations emitted by this enforcing registry.
    lifecycle: TailOwnershipSnapshot,
    /// Whether terminal teardown has closed every future fence acquisition.
    terminal: bool,
}

impl FenceRegistry {
    /// Closes admission and settles every retained root-backed fence owner.
    ///
    /// Pending guards borrow their [`ScribeTailReader`], so final reader
    /// destruction cannot reach this operation until their cancellation or
    /// transfer has settled. An explicit terminal close enforces the same
    /// invariant before releasing any retained owner.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError::State`] without releasing retained owners when
    /// pending acquisition facts have not settled or checked lifecycle
    /// arithmetic diverges. Admission remains terminally closed so cancellation
    /// can settle those pending guards before an idempotent retry.
    fn drain_terminal(&mut self) -> Result<TailOwnershipSnapshot, TailReadError> {
        self.terminal = true;
        if self.pending_fences != 0 || self.pending_payload_bytes != 0 {
            return Err(TailReadError::State {
                detail: "tail terminal drain observed an unsettled pending acquisition".to_owned(),
            });
        }
        let release_count =
            u64::try_from(self.retained.len()).map_err(|error| TailReadError::State {
                detail: format!("tail terminal release count does not fit u64: {error}"),
            })?;
        let released_bytes = self.retained.values().try_fold(0usize, |bytes, retained| {
            bytes
                .checked_add(retained.owner.bytes())
                .ok_or_else(|| TailReadError::State {
                    detail: "tail terminal release byte count overflow".to_owned(),
                })
        })?;
        let active_reservations = self
            .lifecycle
            .active_reservations
            .checked_sub(self.retained.len())
            .ok_or_else(|| TailReadError::State {
                detail: "tail terminal active reservation count underflow".to_owned(),
            })?;
        let active_reserved_bytes = self
            .lifecycle
            .active_reserved_bytes
            .checked_sub(released_bytes)
            .ok_or_else(|| TailReadError::State {
                detail: "tail terminal active reservation bytes underflow".to_owned(),
            })?;
        let releases = self
            .lifecycle
            .releases
            .checked_add(release_count)
            .ok_or_else(|| TailReadError::State {
                detail: "tail terminal release count overflow".to_owned(),
            })?;
        let total_released_bytes = self
            .lifecycle
            .released_bytes
            .checked_add(released_bytes)
            .ok_or_else(|| TailReadError::State {
                detail: "tail terminal cumulative release bytes overflow".to_owned(),
            })?;

        let retained = std::mem::take(&mut self.retained);
        self.retained_bytes = 0;
        self.released.clear();
        self.lifecycle.releases = releases;
        self.lifecycle.released_bytes = total_released_bytes;
        self.lifecycle.active_reservations = active_reservations;
        self.lifecycle.active_reserved_bytes = active_reserved_bytes;
        drop(retained);
        Ok(self.lifecycle)
    }
}

/// Fixed-size live-tail ownership facts from plan through terminal settlement.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TailOwnershipSnapshot {
    /// Acquisitions whose complete pre-material root ceiling was planned.
    pub plans: u64,
    /// Planned payload ceilings plus simultaneous descriptor capacity.
    pub planned_bytes: usize,
    /// Planned root-backed owners admitted before materialization.
    pub reservations: u64,
    /// Initial payload ceilings plus descriptor capacity owned by reservations.
    pub reserved_bytes: usize,
    /// Pending snapshots completed and shrank to their exact retained live set.
    pub materializations: u64,
    /// Retained Arrow payload and descriptor backing represented by snapshots.
    pub materialized_bytes: usize,
    /// Page batches transferred to a local or remote transport.
    pub transfers: u64,
    /// Exact canonical IPC bytes represented by transferred pages.
    pub transferred_bytes: usize,
    /// Terminal owner settlements from pending cancellation, explicit release,
    /// expiry, shutdown, or other registry teardown.
    pub releases: u64,
    /// Root bytes returned by shrink, cancellation, or terminal fence settlement.
    pub released_bytes: usize,
    /// Pending acquisitions and retained fences with live root-backed owners.
    pub active_reservations: usize,
    /// Complete live bytes across pending ceilings and retained snapshots.
    pub active_reserved_bytes: usize,
}

/// One retained fence together with its sole root-backed capacity owner.
#[derive(Debug)]
struct RetainedFence {
    /// Immutable metadata returned by this fence acquisition.
    fence: tail::TailReadFence,
    /// Monotonic local expiration used independently of wall-clock conversion.
    expires_at: Instant,
    /// Shallow append batches frozen at acquisition.
    batches: Vec<RetainedBatch>,
    /// Arrow payload bytes tracked for aggregate retained-fence admission.
    retained_bytes: usize,
    /// Sole owner of the retained Arrow payload and descriptor backing.
    owner: crate::resources::ScribeMemoryLease,
}

/// Cancellation-safe pre-material fence slot and root-backed capacity owner.
///
/// The reservation is created under the fence registry lock before any shard
/// snapshot descriptor or Arrow handle is materialized. Dropping it before
/// transfer returns the pending slot and complete payload-plus-descriptor owner
/// exactly once; successful transfer moves that owner into [`RetainedFence`].
struct PendingFence<'a> {
    /// Reader whose registry owns this pending slot.
    reader: &'a ScribeTailReader,
    /// Root-backed payload-plus-descriptor ceiling protecting materialization.
    owner: Option<crate::resources::ScribeMemoryLease>,
    /// Configured/current-root payload portion excluded from other acquisitions.
    payload_limit: usize,
    /// Whether exact payload and descriptor facts shrank the owner to its live set.
    materialized: bool,
}

#[derive(Debug)]
struct RetainedBatch {
    /// WAL position shared by every row in this admitted append.
    lsn: WalLsn,
    /// Immutable idempotency batch identity shared by every row in this append.
    batch_id: uuid::Uuid,
    /// Shared Arrow arrays retained without copying row payloads.
    rows: Arc<arrow::record_batch::RecordBatch>,
}

impl PendingFence<'_> {
    /// Records a completed snapshot and returns unused ceiling bytes by shrinking.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError::Capacity`] when the materialized payload exceeds
    /// the pre-admitted payload ceiling, or [`TailReadError::State`] when exact
    /// descriptor arithmetic, root shrink, or registry accounting fails.
    fn materialized(
        &mut self,
        payload_bytes: usize,
        descriptor_bytes: usize,
    ) -> Result<(), TailReadError> {
        if payload_bytes > self.payload_limit {
            return Err(TailReadError::Capacity);
        }
        let live_bytes =
            payload_bytes
                .checked_add(descriptor_bytes)
                .ok_or_else(|| TailReadError::State {
                    detail: "tail fence live owner byte count overflow".to_owned(),
                })?;
        let owner = self.owner.as_mut().ok_or_else(|| TailReadError::State {
            detail: "tail fence reservation lost its root owner".to_owned(),
        })?;
        let released =
            owner
                .bytes()
                .checked_sub(live_bytes)
                .ok_or_else(|| TailReadError::State {
                    detail: "tail fence materialization exceeded its root owner".to_owned(),
                })?;
        owner
            .shrink_to(live_bytes)
            .map_err(|error| TailReadError::State {
                detail: format!("tail fence owner shrink failed: {error}"),
            })?;
        let mut registry = self
            .reader
            .fences
            .lock()
            .map_err(|error| TailReadError::State {
                detail: format!("tail fence registry lock poisoned: {error}"),
            })?;
        registry.lifecycle.materializations = registry.lifecycle.materializations.saturating_add(1);
        registry.lifecycle.materialized_bytes = registry
            .lifecycle
            .materialized_bytes
            .saturating_add(live_bytes);
        registry.lifecycle.released_bytes =
            registry.lifecycle.released_bytes.saturating_add(released);
        registry.lifecycle.active_reserved_bytes = registry
            .lifecycle
            .active_reserved_bytes
            .saturating_sub(released);
        self.materialized = true;
        Ok(())
    }

    /// Transfers this pending slot and its root owner into the retained registry.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError::State`] when materialization did not complete,
    /// terminal shutdown closed the registry, the registry lock is poisoned,
    /// or pending accounting diverges.
    fn retain(
        mut self,
        fence: tail::TailReadFence,
        batches: Vec<RetainedBatch>,
        ttl: Duration,
        payload_bytes: usize,
    ) -> Result<tail::TailReadFence, TailReadError> {
        if !self.materialized {
            return Err(TailReadError::State {
                detail: "tail fence cannot retain an unmaterialized reservation".to_owned(),
            });
        }
        let mut registry = self
            .reader
            .fences
            .lock()
            .map_err(|error| TailReadError::State {
                detail: format!("tail fence registry lock poisoned: {error}"),
            })?;
        if registry.terminal {
            return Err(TailReadError::State {
                detail: "tail fence registry closed during materialization".to_owned(),
            });
        }
        let pending_fences =
            registry
                .pending_fences
                .checked_sub(1)
                .ok_or_else(|| TailReadError::State {
                    detail: "tail pending fence count underflow".to_owned(),
                })?;
        let pending_payload_bytes = registry
            .pending_payload_bytes
            .checked_sub(self.payload_limit)
            .ok_or_else(|| TailReadError::State {
                detail: "tail pending payload bytes underflow".to_owned(),
            })?;
        let retained_bytes = registry
            .retained_bytes
            .checked_add(payload_bytes)
            .ok_or_else(|| TailReadError::State {
                detail: "tail retained payload byte count overflow".to_owned(),
            })?;
        if registry.retained.contains_key(&fence.fence_id) {
            return Err(TailReadError::State {
                detail: "tail fence identity collision".to_owned(),
            });
        }
        let owner = self.owner.take().ok_or_else(|| TailReadError::State {
            detail: "tail fence reservation lost its root owner".to_owned(),
        })?;
        registry.pending_fences = pending_fences;
        registry.pending_payload_bytes = pending_payload_bytes;
        registry.retained_bytes = retained_bytes;
        registry.retained.insert(
            fence.fence_id,
            RetainedFence {
                fence: fence.clone(),
                expires_at: Instant::now() + ttl,
                batches,
                retained_bytes: payload_bytes,
                owner,
            },
        );
        metrics::counter!("bifrost_tail_fences_total", "outcome" => "acquired").increment(1);
        Ok(fence)
    }
}

impl Drop for PendingFence<'_> {
    /// Records cancellation and returns an untransferred pending owner exactly once.
    fn drop(&mut self) {
        let Some(owner) = self.owner.take() else {
            return;
        };
        let owner_bytes = owner.bytes();
        if let Ok(mut registry) = self.reader.fences.lock() {
            registry.pending_fences = registry.pending_fences.saturating_sub(1);
            registry.pending_payload_bytes = registry
                .pending_payload_bytes
                .saturating_sub(self.payload_limit);
            registry.lifecycle.releases = registry.lifecycle.releases.saturating_add(1);
            registry.lifecycle.released_bytes = registry
                .lifecycle
                .released_bytes
                .saturating_add(owner_bytes);
            registry.lifecycle.active_reservations =
                registry.lifecycle.active_reservations.saturating_sub(1);
            registry.lifecycle.active_reserved_bytes = registry
                .lifecycle
                .active_reserved_bytes
                .saturating_sub(owner_bytes);
        }
        drop(owner);
    }
}

impl ScribeTailReader {
    /// Creates one bounded reader over the supplied pod-local Scribe source.
    #[must_use]
    pub fn new(source: Arc<FetchLiveTailService>, config: TailFenceConfig) -> Self {
        let max_fences = config.max_fences.max(1);
        let tombstone_capacity = max_fences.saturating_mul(4);
        Self {
            source,
            config: TailFenceConfig {
                ttl: config.ttl.min(Duration::from_secs(30)),
                max_fences,
                max_retained_bytes: config.max_retained_bytes.max(1),
                max_page_rows: config.max_page_rows.max(1),
                max_page_encoded_bytes: config.max_page_encoded_bytes.max(1),
            },
            fences: Mutex::new(FenceRegistry {
                retained: HashMap::with_capacity(max_fences),
                released: HashMap::with_capacity(tombstone_capacity),
                retained_bytes: 0,
                pending_fences: 0,
                pending_payload_bytes: 0,
                lifecycle: TailOwnershipSnapshot::default(),
                terminal: false,
            }),
        }
    }

    /// Closes this reader and settles its retained root-backed fence owners.
    ///
    /// This is the same terminal operation invoked by [`Drop`]. It remains
    /// crate-private so shutdown does not add a public or wire lifecycle API.
    /// A successful second invocation is an idempotent snapshot read.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError::State`] when a pending acquisition has not
    /// settled or exact lifecycle arithmetic diverges. A poisoned lock is
    /// recovered only for terminal owner settlement, and the registry remains
    /// closed to new acquisitions on failure.
    pub(crate) fn drain_terminal(&self) -> Result<TailOwnershipSnapshot, TailReadError> {
        let mut registry = self.fences.lock().unwrap_or_else(|poisoned| {
            tracing::error!("recovering poisoned tail registry for terminal owner drain");
            poisoned.into_inner()
        });
        registry.drain_terminal()
    }

    /// Returns the exact local stream incarnation used for ticket validation.
    #[must_use]
    pub fn stream_identity(&self) -> StreamIdentity {
        self.source.stream()
    }

    /// Return the exact number of live fences retained by this Scribe reader.
    ///
    /// This test-support inspection reads the production fence registry after
    /// bounded expiry reclamation; it does not infer ownership from metrics.
    ///
    /// # Errors
    /// Returns [`TailReadError::State`] when the fence registry lock is poisoned.
    #[cfg(any(test, feature = "test-support"))]
    pub fn active_fence_count_for_test(&self) -> Result<u64, TailReadError> {
        let mut registry = self.fences.lock().map_err(|error| TailReadError::State {
            detail: format!("tail fence registry lock poisoned: {error}"),
        })?;
        Self::reclaim_expired(&mut registry, Instant::now(), OPPORTUNISTIC_EXPIRY_LIMIT);
        u64::try_from(registry.retained.len()).map_err(|error| TailReadError::State {
            detail: format!("active tail fence count does not fit u64: {error}"),
        })
    }

    /// Returns bounded live-tail lifecycle facts after opportunistic expiry.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError::State`] when the fence registry lock is poisoned.
    #[cfg(any(test, feature = "test-support"))]
    pub fn ownership_snapshot_for_test(&self) -> Result<TailOwnershipSnapshot, TailReadError> {
        let mut registry = self.fences.lock().map_err(|error| TailReadError::State {
            detail: format!("tail fence registry lock poisoned: {error}"),
        })?;
        Self::reclaim_expired(&mut registry, Instant::now(), OPPORTUNISTIC_EXPIRY_LIMIT);
        Ok(registry.lifecycle)
    }

    /// Returns retained immutable metadata for capability verification.
    ///
    /// # Errors
    /// Returns [`TailReadError::CursorOutOfRange`] when the fence is unknown or
    /// has already expired.
    pub fn fence_metadata(
        &self,
        fence_id: tail::TailFenceId,
    ) -> Result<tail::TailReadFence, TailReadError> {
        let mut registry = self.fences.lock().map_err(|error| TailReadError::State {
            detail: format!("tail fence registry lock poisoned: {error}"),
        })?;
        Self::reclaim_expired(&mut registry, Instant::now(), OPPORTUNISTIC_EXPIRY_LIMIT);
        registry
            .retained
            .get(&fence_id)
            .map(|retained| retained.fence.clone())
            .or_else(|| {
                registry
                    .released
                    .get(&fence_id)
                    .map(|(_, fence)| fence.clone())
            })
            .ok_or(TailReadError::CursorOutOfRange)
    }

    /// Lists active event-day scopes for one authenticated tenant/table.
    ///
    /// This is intentionally metadata-only.  It does not allocate a fence or
    /// expose row state; callers must acquire an exact fence for every returned
    /// day before reading data.
    ///
    /// # Errors
    /// Returns [`TailReadError::Binding`] for an invalid wire binding and
    /// [`TailReadError::State`] when the Scribe snapshot cannot be read.
    pub fn list_active_streams(
        &self,
        binding: &tail::TenantTableBinding,
    ) -> Result<Vec<(tail::TimePartitionWire, tail::TailStreamIdentity)>, TailReadError> {
        let binding = binding_from_wire(binding)?;
        let keys = self
            .source
            .active_seal_keys_for_tenant(binding.tenant)
            .map_err(|error| TailReadError::State {
                detail: error.to_string(),
            })?;
        tracing::debug!(
            tenant = %binding.tenant,
            table = %binding.table_ref,
            active_key_count = keys.len(),
            active_keys = ?keys,
            "listed active Scribe tail keys"
        );
        let stream = self.source.stream();
        let writer_epoch =
            u64::try_from(stream.writer_epoch.as_i64()).map_err(|_| TailReadError::State {
                detail: "Scribe writer epoch is negative".to_owned(),
            })?;
        let mut partitions = keys
            .into_iter()
            .filter(|key| key.table == binding.table_ref)
            .map(|key| key.partition)
            .collect::<Vec<_>>();
        partitions.sort();
        partitions.dedup();
        Ok(partitions
            .into_iter()
            .map(TimePartition::to_wire)
            .map(|partition| {
                (
                    partition,
                    tail::TailStreamIdentity {
                        node_id: tail::NodeId::new(stream.node_id.as_uuid()),
                        writer_epoch,
                    },
                )
            })
            .collect())
    }

    /// Acquires metadata for one exact `(exclusive, inclusive]` shallow interval.
    ///
    /// A pending registry slot and root-backed payload-plus-descriptor ceiling
    /// are acquired before the shard request can allocate snapshot descriptors
    /// or shallow Arrow handles. Cancellation settles through the pending
    /// owner's `Drop`, while success shrinks and transfers the same owner into
    /// the retained fence.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError`] when the binding, protocol, deadline, stream,
    /// schema, or registry capacity is invalid, or when Scribe cannot snapshot
    /// its live rows.
    pub async fn acquire_fence(
        &self,
        request: tail::AcquireTailFenceRequest,
    ) -> Result<tail::TailReadFence, TailReadError> {
        if request.tail_protocol_version != TAIL_PROTOCOL_VERSION {
            return Err(TailReadError::UnsupportedProtocol {
                version: request.tail_protocol_version,
            });
        }
        let now = Utc::now();
        let remaining = request.deadline.signed_duration_since(now);
        let ttl = remaining
            .to_std()
            .map_err(|_| TailReadError::DeadlineElapsed)?;
        let ttl = ttl.min(self.config.ttl);
        if ttl.is_zero() {
            return Err(TailReadError::DeadlineElapsed);
        }
        let binding = binding_from_wire(&request.binding)?;
        let time_partition = partition_from_wire(request.time_partition);
        let stream = self.source.stream();
        let stream_epoch =
            u64::try_from(stream.writer_epoch.as_i64()).map_err(|_| TailReadError::State {
                detail: "Scribe writer epoch is negative".to_owned(),
            })?;
        if request.exclusive_sealed.writer_epoch != stream_epoch {
            return Err(TailReadError::WriterEpochMismatch);
        }
        let mut pending = self.reserve_pending_fence()?;
        let hot = self
            .source
            .fetch_hot_batches(FetchLiveTailRequest {
                binding: binding.clone(),
                target_stream: stream,
                start_partition: time_partition,
                end_partition: time_partition,
                after_lsn: WalLsn::ZERO,
                persisted_lsn_ranges: Vec::new(),
                required_columns: Vec::new(),
                predicates: Vec::new(),
                max_batches: self.config.max_page_rows as usize,
                max_retained_bytes: pending.payload_limit,
            })
            .await
            .map_err(|error| TailReadError::State {
                detail: error.to_string(),
            })?;
        let mut batches = hot
            .into_iter()
            .map(|batch| {
                Ok(RetainedBatch {
                    lsn: batch.wal_lsn,
                    batch_id: cursor_batch_id(&batch)?,
                    rows: Arc::new(batch.rows),
                })
            })
            .collect::<Result<Vec<_>, TailReadError>>()?;
        batches.sort_by_key(|batch| (batch.lsn, batch.batch_id));
        validate_schema(&batches, &request.schema_fingerprint)?;
        let inclusive_live = inclusive_cursor(&request.exclusive_sealed, stream_epoch, &batches)?;
        let retained_bytes = batches.iter().try_fold(0_usize, |total, batch| {
            total
                .checked_add(batch.rows.get_array_memory_size())
                .ok_or_else(|| TailReadError::State {
                    detail: "tail fence retained byte count overflow".to_owned(),
                })
        })?;
        let descriptor_bytes = batches
            .capacity()
            .checked_mul(std::mem::size_of::<RetainedBatch>())
            .ok_or_else(|| TailReadError::State {
                detail: "tail retained descriptor byte count overflow".to_owned(),
            })?;
        pending.materialized(retained_bytes, descriptor_bytes)?;
        let expires_at =
            now + chrono::Duration::from_std(ttl).map_err(|_| TailReadError::DeadlineElapsed)?;
        let fence = tail::TailReadFence {
            fence_id: tail::TailFenceId::new(uuid::Uuid::now_v7()),
            binding: request.binding,
            time_partition: request.time_partition,
            stream: tail::TailStreamIdentity {
                node_id: tail::NodeId::new(stream.node_id.as_uuid()),
                writer_epoch: stream_epoch,
            },
            exclusive_sealed: request.exclusive_sealed,
            inclusive_live,
            schema_fingerprint: request.schema_fingerprint,
            tail_protocol_version: TAIL_PROTOCOL_VERSION,
            expires_at,
        };
        pending.retain(fence, batches, ttl, retained_bytes)
    }

    /// Reserves one slot and complete pre-material root-backed ceiling.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError::Capacity`] before materialization when the fixed
    /// fence/aggregate payload ceiling is exhausted, terminal shutdown closed
    /// admission, or the Scribe root refuses the current-root/configured payload
    /// plus simultaneous descriptor ceiling. Returns [`TailReadError::State`]
    /// when checked arithmetic or the registry lock fails.
    fn reserve_pending_fence(&self) -> Result<PendingFence<'_>, TailReadError> {
        let mut registry = self.fences.lock().map_err(|error| TailReadError::State {
            detail: format!("tail fence registry lock poisoned: {error}"),
        })?;
        Self::reclaim_expired(&mut registry, Instant::now(), OPPORTUNISTIC_EXPIRY_LIMIT);
        if registry.terminal {
            return Err(TailReadError::Capacity);
        }
        if registry
            .retained
            .len()
            .checked_add(registry.pending_fences)
            .is_none_or(|count| count >= self.config.max_fences)
        {
            return Err(TailReadError::Capacity);
        }
        let committed_payload = registry
            .retained_bytes
            .checked_add(registry.pending_payload_bytes)
            .ok_or(TailReadError::Capacity)?;
        let configured_payload_limit = self
            .config
            .max_retained_bytes
            .checked_sub(committed_payload)
            .filter(|bytes| *bytes > 0)
            .ok_or(TailReadError::Capacity)?;
        let descriptor_bytes = (self.config.max_page_rows as usize)
            .checked_mul(TAIL_BATCH_DESCRIPTOR_BYTES)
            .ok_or_else(|| TailReadError::State {
                detail: "tail descriptor reservation byte count overflow".to_owned(),
            })?;
        let available_bytes = self.source.available_fence_bytes()?;
        let available_payload = available_bytes
            .checked_sub(descriptor_bytes)
            .filter(|bytes| *bytes > 0)
            .ok_or(TailReadError::Capacity)?;
        let payload_limit = configured_payload_limit.min(available_payload);
        let owner_bytes =
            payload_limit
                .checked_add(descriptor_bytes)
                .ok_or_else(|| TailReadError::State {
                    detail: "tail fence owner byte count overflow".to_owned(),
                })?;
        let pending_fences =
            registry
                .pending_fences
                .checked_add(1)
                .ok_or_else(|| TailReadError::State {
                    detail: "tail pending fence count overflow".to_owned(),
                })?;
        let pending_payload_bytes = registry
            .pending_payload_bytes
            .checked_add(payload_limit)
            .ok_or_else(|| TailReadError::State {
                detail: "tail pending payload byte count overflow".to_owned(),
            })?;
        let owner = self
            .source
            .reserve_fence_owner(owner_bytes)
            .map_err(|error| match error {
                ScribeError::IngestBusy { .. } => TailReadError::Capacity,
                other => TailReadError::State {
                    detail: other.to_string(),
                },
            })?;
        registry.lifecycle.plans = registry.lifecycle.plans.saturating_add(1);
        registry.lifecycle.planned_bytes =
            registry.lifecycle.planned_bytes.saturating_add(owner_bytes);
        registry.lifecycle.reservations = registry.lifecycle.reservations.saturating_add(1);
        registry.lifecycle.reserved_bytes = registry
            .lifecycle
            .reserved_bytes
            .saturating_add(owner_bytes);
        registry.lifecycle.active_reservations =
            registry.lifecycle.active_reservations.saturating_add(1);
        registry.lifecycle.active_reserved_bytes = registry
            .lifecycle
            .active_reserved_bytes
            .saturating_add(owner_bytes);
        registry.pending_fences = pending_fences;
        registry.pending_payload_bytes = pending_payload_bytes;
        Ok(PendingFence {
            reader: self,
            owner: Some(owner),
            payload_limit,
            materialized: false,
        })
    }

    /// Reads one row-precise shallow page from a retained fence.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError`] when the fence expired, the continuation is
    /// outside its exact interval, or an individual row cannot meet the encoded
    /// byte bound.
    pub fn read_page(
        &self,
        request: &tail::TailPageRequest,
    ) -> Result<LocalTailPage, TailReadError> {
        self.read_page_inner(None, request.clone())
    }

    /// Reads one page only when the authenticated tenant owns the retained binding.
    ///
    /// The tenant check and shallow-batch lookup occur under the same registry
    /// lock, so an unauthorized caller cannot observe data between validation
    /// and paging.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError::AccessDenied`] before reading retained data when
    /// `tenant` differs from the fence binding. Other failures match
    /// [`Self::read_page`].
    pub fn read_page_for_tenant(
        &self,
        tenant: DataTenantId,
        request: tail::TailPageRequest,
    ) -> Result<LocalTailPage, TailReadError> {
        self.read_page_inner(Some(tenant), request)
    }

    /// Implements local and authenticated paging under one atomic registry lookup.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError`] for poisoned state, an unknown or expired
    /// fence, unauthorized tenant, invalid continuation, encoding failure, or
    /// an unsatisfied one-row byte ceiling.
    fn read_page_inner(
        &self,
        tenant: Option<DataTenantId>,
        request: tail::TailPageRequest,
    ) -> Result<LocalTailPage, TailReadError> {
        let tail::TailPageRequest {
            query_id: _,
            fence_id,
            after,
            max_rows,
            max_encoded_bytes,
        } = request;
        let mut registry = self.fences.lock().map_err(|error| TailReadError::State {
            detail: format!("tail fence registry lock poisoned: {error}"),
        })?;
        Self::reclaim_expired(&mut registry, Instant::now(), OPPORTUNISTIC_EXPIRY_LIMIT);
        let retained = registry
            .retained
            .get(&fence_id)
            .ok_or(TailReadError::CursorOutOfRange)?;
        if tenant.is_some_and(|tenant| retained.fence.binding.tenant_id != tenant) {
            return Err(TailReadError::AccessDenied);
        }
        validate_after(after.as_ref(), &retained.fence)?;
        let row_limit = max_rows.min(self.config.max_page_rows).max(1) as usize;
        let byte_limit = max_encoded_bytes
            .min(self.config.max_page_encoded_bytes)
            .max(1) as usize;
        let after = after.as_ref().unwrap_or(&retained.fence.exclusive_sealed);
        let mut rows = Vec::with_capacity(row_limit);
        let mut encoded_bytes = 0_usize;
        let mut next = None;
        for batch in &retained.batches {
            let ordinals = crate::schema::managed_columns::row_ordinals(batch.rows.as_ref())
                .map_err(|error| TailReadError::State {
                    detail: format!("retained row identity invariant failed: {error}"),
                })?;
            for row_index in 0..batch.rows.num_rows() {
                let cursor = tail::TailCursor {
                    writer_epoch: retained.fence.stream.writer_epoch,
                    wal_lsn: batch.lsn.as_u64(),
                    batch_id: batch.batch_id,
                    row_ordinal: u32::try_from(ordinals.value(row_index)).map_err(|_| {
                        TailReadError::State {
                            detail: "retained row ordinal is negative".to_owned(),
                        }
                    })?,
                };
                if cursor_cmp(&cursor, after)? != std::cmp::Ordering::Greater {
                    continue;
                }
                if cursor_cmp(&cursor, &retained.fence.inclusive_live)? != std::cmp::Ordering::Less
                    && cursor != retained.fence.inclusive_live
                {
                    continue;
                }
                let row = Arc::new(batch.rows.slice(row_index, 1));
                let row_bytes = encoded_record_batch_bytes(row.as_ref())?;
                if rows.is_empty() && row_bytes > byte_limit {
                    return Err(TailReadError::OversizeRow);
                }
                if rows.len() == row_limit || encoded_bytes.saturating_add(row_bytes) > byte_limit {
                    registry.lifecycle.transfers = registry
                        .lifecycle
                        .transfers
                        .saturating_add(u64::try_from(rows.len()).unwrap_or(u64::MAX));
                    registry.lifecycle.transferred_bytes = registry
                        .lifecycle
                        .transferred_bytes
                        .saturating_add(encoded_bytes);
                    return Ok(LocalTailPage {
                        batches: rows,
                        next,
                        complete: false,
                    });
                }
                encoded_bytes = encoded_bytes.saturating_add(row_bytes);
                next = Some(cursor);
                rows.push(row);
            }
        }
        registry.lifecycle.transfers = registry
            .lifecycle
            .transfers
            .saturating_add(u64::try_from(rows.len()).unwrap_or(u64::MAX));
        registry.lifecycle.transferred_bytes = registry
            .lifecycle
            .transferred_bytes
            .saturating_add(encoded_bytes);
        Ok(LocalTailPage {
            batches: rows,
            next,
            complete: true,
        })
    }

    /// Settles one retained owner exactly once; repeat calls are successful no-ops.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError::State`] only when the local registry lock is poisoned.
    pub fn release_fence(
        &self,
        fence_id: tail::TailFenceId,
    ) -> Result<FenceRelease, TailReadError> {
        self.release_fence_inner(None, fence_id)
    }

    /// Releases a fence only when the authenticated tenant owns its binding.
    ///
    /// The ownership check precedes removal under the same registry lock. A
    /// denied release therefore leaves retained bytes and capacity unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError::AccessDenied`] for a cross-tenant fence and
    /// [`TailReadError::State`] when the registry lock is poisoned.
    pub fn release_fence_for_tenant(
        &self,
        tenant: DataTenantId,
        fence_id: tail::TailFenceId,
    ) -> Result<FenceRelease, TailReadError> {
        self.release_fence_inner(Some(tenant), fence_id)
    }

    /// Settles a retained payload-plus-descriptor owner under one registry mutation.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError::AccessDenied`] before removal for a mismatched
    /// tenant or [`TailReadError::State`] when the registry lock is poisoned.
    fn release_fence_inner(
        &self,
        tenant: Option<DataTenantId>,
        fence_id: tail::TailFenceId,
    ) -> Result<FenceRelease, TailReadError> {
        let mut registry = self.fences.lock().map_err(|error| TailReadError::State {
            detail: format!("tail fence registry lock poisoned: {error}"),
        })?;
        Self::reclaim_expired(&mut registry, Instant::now(), OPPORTUNISTIC_EXPIRY_LIMIT);
        if registry.retained.get(&fence_id).is_some_and(|retained| {
            tenant.is_some_and(|tenant| retained.fence.binding.tenant_id != tenant)
        }) {
            return Err(TailReadError::AccessDenied);
        }
        let Some(_retained) = registry.retained.get(&fence_id) else {
            if registry.released.contains_key(&fence_id) {
                return Ok(FenceRelease { released: true });
            }
            return Ok(FenceRelease { released: false });
        };
        let retained = registry
            .retained
            .remove(&fence_id)
            .expect("retained fence remains present while registry lock is held");
        let owner_bytes = retained.owner.bytes();
        registry.retained_bytes = registry
            .retained_bytes
            .saturating_sub(retained.retained_bytes);
        registry.lifecycle.releases = registry.lifecycle.releases.saturating_add(1);
        registry.lifecycle.released_bytes = registry
            .lifecycle
            .released_bytes
            .saturating_add(owner_bytes);
        registry.lifecycle.active_reservations =
            registry.lifecycle.active_reservations.saturating_sub(1);
        registry.lifecycle.active_reserved_bytes = registry
            .lifecycle
            .active_reserved_bytes
            .saturating_sub(owner_bytes);
        // Tombstones no longer bound live admission, so cap the map purely as a
        // memory backstop at `max_fences * 4`. At capacity, evict the entry with
        // the earliest `expires_at` (the soonest to be purged anyway) via a linear
        // scan under the already-held registry lock; the cap keeps the scan cheap.
        // Reaching this cap requires release churn above 4x the live-fence budget
        // within one TTL, whose only consequence — a later duplicate release of an
        // evicted fence sees `released: false` — matches the post-TTL semantics
        // every caller already tolerates.
        let tombstone_cap = self.config.max_fences.saturating_mul(4);
        if tombstone_cap > 0
            && registry.released.len() >= tombstone_cap
            && let Some(evict_id) = registry
                .released
                .iter()
                .min_by_key(|(_, (expires_at, _))| *expires_at)
                .map(|(fence_id, _)| *fence_id)
        {
            registry.released.remove(&evict_id);
        }
        registry
            .released
            .insert(fence_id, (retained.expires_at, retained.fence));
        metrics::counter!("bifrost_tail_fences_total", "outcome" => "released").increment(1);
        Ok(FenceRelease { released: true })
    }

    /// Settles at most `max` expired owners and reports remaining retained fences.
    #[must_use]
    pub fn expire_due(&self, now: Instant, max: usize) -> ExpiryReport {
        let Ok(mut registry) = self.fences.lock() else {
            return ExpiryReport {
                released: 0,
                retained: 0,
            };
        };
        let released = Self::reclaim_expired(&mut registry, now, max);
        ExpiryReport {
            released,
            retained: registry.retained.len(),
        }
    }

    /// Reclaims bounded expired entries and their payload-plus-descriptor owners.
    fn reclaim_expired(registry: &mut FenceRegistry, now: Instant, max: usize) -> usize {
        let expired = registry
            .retained
            .iter()
            .filter_map(|(fence_id, retained)| (retained.expires_at <= now).then_some(*fence_id))
            .take(max)
            .collect::<Vec<_>>();
        for fence_id in &expired {
            if let Some(retained) = registry.retained.remove(fence_id) {
                let owner_bytes = retained.owner.bytes();
                registry.retained_bytes = registry
                    .retained_bytes
                    .saturating_sub(retained.retained_bytes);
                registry.lifecycle.releases = registry.lifecycle.releases.saturating_add(1);
                registry.lifecycle.released_bytes = registry
                    .lifecycle
                    .released_bytes
                    .saturating_add(owner_bytes);
                registry.lifecycle.active_reservations =
                    registry.lifecycle.active_reservations.saturating_sub(1);
                registry.lifecycle.active_reserved_bytes = registry
                    .lifecycle
                    .active_reserved_bytes
                    .saturating_sub(owner_bytes);
            }
        }
        let expired_tombstones = registry
            .released
            .iter()
            .filter_map(|(fence_id, (expiry, _))| (*expiry <= now).then_some(*fence_id))
            .take(max)
            .collect::<Vec<_>>();
        for fence_id in expired_tombstones {
            registry.released.remove(&fence_id);
        }
        if !expired.is_empty() {
            metrics::counter!("bifrost_tail_fences_total", "outcome" => "expired")
                .increment(expired.len() as u64);
        }
        expired.len()
    }
}

impl Drop for ScribeTailReader {
    /// Runs the enforcing registry's terminal owner drain before destruction.
    fn drop(&mut self) {
        if let Err(error) = self.drain_terminal() {
            tracing::error!(%error, "Scribe tail terminal owner drain failed");
        }
    }
}

/// Exact, bounded hot-read request handed from Oracle to Scribe.
#[derive(Debug, Clone)]
pub struct FetchLiveTailRequest {
    /// Authenticated tenant/table binding resolved by Oracle.
    pub binding: TenantTableBinding,
    /// The writer stream the caller believes it is talking to.
    pub target_stream: StreamIdentity,
    /// Inclusive first partition day governed by the query.
    pub start_partition: TimePartition,
    /// Inclusive last partition day governed by the query.
    pub end_partition: TimePartition,
    /// Emit only records with `LSN > after_lsn`.
    pub after_lsn: WalLsn,
    /// Manifest-pinned inclusive WAL ranges already owned by persisted files.
    ///
    /// Each range suppresses only its cohort member, so an independently
    /// published later member cannot hide an earlier hot member.
    pub persisted_lsn_ranges: Vec<(WalLsn, WalLsn)>,
    /// Columns required by Oracle filters, ordering, tripwire, and projection.
    pub required_columns: Vec<String>,
    /// Signed closed predicates the assignment authorized for this scan.
    ///
    /// Scribe applies this conjunction to the assembled snapshot before
    /// returning it, so a selective query ships only matching rows back to
    /// the follower instead of the whole live tail.
    pub predicates: Vec<wyrd_spec::vala::assignment_authority::ScanPredicate>,
    /// Maximum shallow Arrow batches materialized by the snapshot.
    pub max_batches: usize,
    /// Maximum source-derived Arrow bytes retained by the snapshot.
    pub max_retained_bytes: usize,
}

impl FetchLiveTailRequest {
    /// Return any consistent shard index for attribution purposes.
    ///
    /// Under batch-spread routing the live-tail data for one (tenant, table)
    /// may be spread across multiple shard lanes, so this value is used only
    /// for approximate attribution and diagnostics — not for dispatch. Use
    /// `ScribeShardRuntime::snapshot` for the
    /// fan-out that merges results across all shards.
    #[must_use]
    pub fn shard_id(&self) -> usize {
        // Use a stable zero-UUID as the batch_id placeholder so the
        // attribution-only shard is deterministic across calls on the same
        // request. The fan-out dispatch in ScribeShardRuntime::snapshot is the
        // authoritative multi-shard read path.
        shard_for(
            self.binding.tenant,
            &self.binding.table_ref,
            uuid::Uuid::nil(),
        )
    }
}

/// One shallow, structural hot snapshot returned by a shard owner.
#[derive(Debug, Clone)]
pub struct HotBatch {
    /// Exact partition day owning the batch.
    pub partition_day: TimePartition,
    /// Highest WAL LSN the batch's rows cover.
    pub wal_lsn: WalLsn,
    /// Where the rows came from, and under whose identity.
    pub origin: HotBatchSource,
    /// Arrow rows projected to the request's required columns.
    pub rows: arrow::record_batch::RecordBatch,
}

/// Source identity of one live-tail batch.
///
/// A batch is served either from the append that is still in memory or from the
/// durable staged member that replaced it. Both identities are real durable
/// facts; neither is derivable from the other, so the batch carries whichever
/// one actually produced its rows rather than a single field that would have to
/// be invented for the other case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotBatchSource {
    /// Rows still held by the append that wrote them.
    Append {
        /// Idempotency identity of the append.
        batch_id: [u8; 16],
    },
    /// Rows served from a durable staged member.
    StagedMember {
        /// Staged member identity holding the rows.
        member: crate::scribe::assembly::StagedMemberId,
        /// Inclusive WAL bounds the member covers, as its record names them.
        ///
        /// A staged batch has no single append behind it, so the bounds are the
        /// only exact answer to which WAL a reader's cut has to account for.
        wal: (WalLsn, WalLsn),
    },
}

/// Pod-local live-tail service over the Scribe memtable.
#[derive(Debug)]
pub struct FetchLiveTailService {
    /// Existing Scribe root capability used for authoritative fence ownership.
    resources: crate::resources::ScribeResources,
    /// Exact pod-local WAL stream incarnation served by this source.
    stream: StreamIdentity,
    /// Direct in-process memtable used only by narrow fixtures.
    memtable: Option<Arc<Memtable>>,
    /// Pod-wide authority registry naming which staged members serve rows.
    ///
    /// `None` for the direct-memtable fixtures, which have no staged boundary:
    /// their generations never leave memory.
    hot_sources: Option<Arc<crate::scribe::hot_source::ScribeHotSourceRegistry>>,
    /// Production shard runtime that owns live generation state.
    shards: Option<Arc<ScribeShardRuntime>>,
}

impl FetchLiveTailService {
    /// Construct a reader over a direct in-process memtable.
    ///
    /// The direct-memtable branch backs the narrow in-process adapter used by
    /// unit tests; production readers submit shard snapshots through
    /// `FetchLiveTailService::with_runtime`.
    #[must_use]
    pub fn new(
        stream: StreamIdentity,
        memtable: Arc<Memtable>,
        resources: crate::resources::ScribeResources,
    ) -> Self {
        Self {
            resources,
            stream,
            hot_sources: None,
            memtable: Some(memtable),
            shards: None,
        }
    }

    /// Construct a production reader that submits snapshots to the owning
    /// shard command queue instead of traversing Scribe state directly.
    #[must_use]
    pub(crate) fn with_runtime(
        stream: StreamIdentity,
        shards: Arc<ScribeShardRuntime>,
        resources: crate::resources::ScribeResources,
        hot_sources: Arc<crate::scribe::hot_source::ScribeHotSourceRegistry>,
    ) -> Self {
        Self {
            resources,
            stream,
            hot_sources: Some(hot_sources),
            memtable: None,
            shards: Some(shards),
        }
    }

    /// Stream identity this service serves.
    #[must_use]
    pub fn stream(&self) -> StreamIdentity {
        self.stream
    }

    /// Acquires one root-backed fence owner before snapshot materialization.
    ///
    /// # Errors
    ///
    /// Returns the stable Scribe capacity/internal error when the existing
    /// process root cannot admit the complete payload-plus-descriptor ceiling.
    fn reserve_fence_owner(
        &self,
        bytes: usize,
    ) -> Result<crate::resources::ScribeMemoryLease, ScribeError> {
        self.resources
            .try_reserve_maintenance(crate::scribe::memory::MemoryCategory::Immutable, bytes)
    }

    /// Returns the capacity the existing Scribe root can assign to a fence now.
    ///
    /// The value includes unused protected-floor bytes and currently free
    /// elastic bytes. It is only a planning ceiling: the subsequent root
    /// reservation remains authoritative if another owner races this snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`TailReadError::State`] when the shared root cannot provide a
    /// trustworthy snapshot or the free-capacity arithmetic overflows.
    fn available_fence_bytes(&self) -> Result<usize, TailReadError> {
        let snapshot = self
            .resources
            .snapshot()
            .map_err(|error| TailReadError::State {
                detail: format!("tail fence root snapshot failed: {error}"),
            })?;
        let free_floor = snapshot
            .plan
            .scribe_floor_bytes
            .saturating_sub(snapshot.scribe_memory_used_bytes);
        let free_elastic = snapshot
            .plan
            .elastic_memory_bytes
            .checked_sub(snapshot.elastic_memory_used_bytes)
            .ok_or_else(|| TailReadError::State {
                detail: "tail fence root elastic ownership exceeds its plan".to_owned(),
            })?;
        free_floor
            .checked_add(free_elastic)
            .ok_or_else(|| TailReadError::State {
                detail: "tail fence free-capacity arithmetic overflow".to_owned(),
            })
    }

    /// Lists the tenant-owned table/day scopes that still have live Scribe
    /// state.  The list is a point-in-time discovery cut; retirement is driven
    /// by the durable file-list commit and therefore naturally removes a key
    /// from subsequent cuts.
    ///
    /// # Errors
    /// Returns [`ScribeError`] when a shard inspection snapshot or test
    /// memtable lock cannot be read.
    pub fn active_seal_keys_for_tenant(
        &self,
        tenant: DataTenantId,
    ) -> Result<Vec<crate::scribe::seal_key::SealKey>, ScribeError> {
        if let Some(memtable) = &self.memtable {
            return memtable.seal_keys_for_tenant(tenant);
        }
        self.shards
            .as_ref()
            .ok_or_else(|| ScribeError::Internal {
                detail: "tail source has no shard runtime".to_owned(),
            })?
            .active_seal_keys_for_tenant(tenant)
    }

    /// Return the canonical pod-local shard for a live-tail scope.
    #[must_use]
    pub fn shard_id(&self, request: &FetchLiveTailRequest) -> usize {
        request.shard_id()
    }

    /// Return direct local Arrow handles for Oracle's `MemoryExec` path.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when stream/range validation fails, the bounded
    /// snapshot exceeds its count or retained-byte ceiling, or the owning
    /// memtable/shard cannot produce the requested projection.
    pub async fn fetch_hot_batches(
        &self,
        request: FetchLiveTailRequest,
    ) -> Result<Vec<HotBatch>, ScribeError> {
        if request.target_stream != self.stream {
            return Err(ScribeError::StreamMismatch {
                requested: request.target_stream,
                actual: self.stream,
            });
        }
        if request.start_partition > request.end_partition {
            return Err(ScribeError::Internal {
                detail: "live-tail start day is after end day".to_owned(),
            });
        }
        let predicates = request.predicates.clone();
        let staged = self.staged_batches(&request)?;
        let assembled = if let Some(shards) = &self.shards {
            let mut assembled = shards.snapshot(request).await?;
            assembled.extend(staged);
            assembled
        } else {
            self.memtable
                .as_ref()
                .ok_or_else(|| ScribeError::Internal {
                    detail: "direct tail memtable is not configured".to_owned(),
                })?
                .readable_batches_for_range(
                    request.binding.tenant,
                    &request.binding.table_ref,
                    request.start_partition,
                    request.end_partition,
                    &request.required_columns,
                    ReadableBatchLimits {
                        max_batches: request.max_batches,
                        max_retained_bytes: request.max_retained_bytes,
                    },
                )?
                .into_iter()
                .map(|readable| HotBatch {
                    partition_day: readable.partition_day,
                    wal_lsn: readable.meta.wal_lsn_max,
                    origin: HotBatchSource::Append {
                        batch_id: readable.meta.batch_id,
                    },
                    rows: readable.batch,
                })
                .collect()
        };
        Self::retain_signed_rows(assembled, &predicates)
    }

    /// Returns the rows this request's staged members still serve.
    ///
    /// A generation whose Arrow was released after staging is invisible to the
    /// shard snapshot, so without this a live-tail reader would see a gap
    /// between staging and publication. The read is bounded by the same
    /// projection, count, and retained-byte limits the memtable path obeys, and
    /// a member whose complete WAL range the pinned cut already owns is skipped
    /// because the published object serves those rows.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the registry is unavailable or a
    /// staged run cannot be opened, projected, or decoded.
    fn staged_batches(&self, request: &FetchLiveTailRequest) -> Result<Vec<HotBatch>, ScribeError> {
        let Some(hot_sources) = &self.hot_sources else {
            return Ok(Vec::new());
        };
        let sources = hot_sources
            .staged_sources(
                request.binding.tenant,
                &request.binding.table_ref,
                request.start_partition,
                request.end_partition,
            )
            .map_err(|error| ScribeError::Internal {
                detail: format!("resolve the staged members serving a live-tail read: {error}"),
            })?;
        if sources.sources().is_empty() {
            return Ok(Vec::new());
        }
        crate::scribe::staged_tail::StagedTailReader::default().read(
            sources.sources(),
            &crate::scribe::staged_tail::StagedTailRead {
                required_columns: &request.required_columns,
                limits: crate::scribe::memtable::ReadableBatchLimits {
                    max_batches: request.max_batches,
                    max_retained_bytes: request.max_retained_bytes,
                },
                persisted_cursor: request.after_lsn,
                persisted_ranges: &request.persisted_lsn_ranges,
            },
        )
    }

    /// Applies the assignment's signed predicate conjunction to an assembled
    /// snapshot, dropping batches that retain no rows.
    ///
    /// This is the live tail's equivalent of the `FilterExec` the leader
    /// keeps above a persisted provider: the caller receives exactly the
    /// rows the signed closure authorizes, so nothing further is shipped
    /// into follower attempt encoding.
    ///
    /// # Errors
    /// Returns [`ScribeError::Internal`] when a signed predicate cannot be
    /// compiled against, or evaluated over, the snapshot's own schema.
    fn retain_signed_rows(
        batches: Vec<HotBatch>,
        predicates: &[wyrd_spec::vala::assignment_authority::ScanPredicate],
    ) -> Result<Vec<HotBatch>, ScribeError> {
        if predicates.is_empty() {
            return Ok(batches);
        }
        let mut retained = Vec::with_capacity(batches.len());
        for batch in batches {
            let HotBatch {
                partition_day,
                wal_lsn,
                origin,
                rows,
            } = batch;
            let filter =
                crate::oracle::exec::ScanPredicateFilter::compile(&rows.schema(), predicates)
                    .map_err(|error| ScribeError::Internal {
                        detail: format!(
                            "live-tail predicate is invalid for this snapshot: {error}"
                        ),
                    })?;
            let rows = filter.retain(rows).map_err(|error| ScribeError::Internal {
                detail: format!("live-tail predicate evaluation failed: {error}"),
            })?;
            if rows.num_rows() > 0 {
                retained.push(HotBatch {
                    partition_day,
                    wal_lsn,
                    origin,
                    rows,
                });
            }
        }
        Ok(retained)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    use super::{
        FetchLiveTailService, LocalTailReadTransport, RetainedBatch, ScribeTailReader,
        TailFenceConfig, TailReadError, TailReadTransport, TailTicketAudience, TailTicketBinding,
        TailTicketClaims, TailTicketVerifier, cursor_cmp,
    };
    use crate::scribe::memtable::Memtable;
    use crate::scribe::stream_identity::{NodeId, StreamIdentity, WriterEpoch};
    use crate::scribe::wal::WalLsn;
    use arrow::array::{Int64Array, RecordBatch};
    use arrow::datatypes::{DataType, Field, Schema};
    use wyrd_spec::DataTenantId;
    use wyrd_spec::vala::api as tail;
    use wyrd_spec::vala::api::{
        AcquireTailFenceRequest, SchemaFingerprint, TailCursor, TailPageRequest, TenantTableBinding,
    };

    /// Builds one production-shaped Scribe root for direct tail-reader tests.
    fn tail_resources() -> crate::resources::ScribeResources {
        crate::scribe::embedded_scribe_resources(&crate::scribe::AdmissionConfig::default())
    }

    /// Builds one valid empty-interval request for opportunistic registry tests.
    fn empty_fence_request(tenant: DataTenantId) -> AcquireTailFenceRequest {
        AcquireTailFenceRequest {
            query_id: uuid::Uuid::nil(),
            binding: TenantTableBinding {
                tenant_id: tenant,
                namespace: "bifrost".to_owned(),
                table: "events".to_owned(),
            },
            time_partition: crate::test_support::day_partition(2026, 7, 14).to_wire(),
            exclusive_sealed: TailCursor {
                writer_epoch: 1,
                wal_lsn: 0,
                batch_id: uuid::Uuid::nil(),
                row_ordinal: 0,
            },
            deadline: chrono::Utc::now() + chrono::Duration::seconds(5),
            schema_fingerprint: SchemaFingerprint::new("empty-schema")
                .expect("fixture fingerprint"),
            tail_protocol_version: 1,
        }
    }

    /// Test verifier that records query-binding failures before allocation.
    struct BindingProbe {
        /// Claims returned by the synthetic ticket decoder.
        claims: TailTicketClaims,
        /// Number of binding failures observed by the audit hook.
        audited: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl TailTicketVerifier for BindingProbe {
        async fn verify_tail_ticket_unbound(
            &self,
            _encoded: &[u8],
            _audience: TailTicketAudience,
        ) -> Result<TailTicketClaims, TailReadError> {
            Ok(self.claims.clone())
        }

        async fn verify_tail_ticket_binding(
            &self,
            claims: &TailTicketClaims,
            binding: &TailTicketBinding,
        ) -> Result<(), TailReadError> {
            if claims
                .validate_binding(
                    binding.query_id,
                    binding.tenant_id,
                    &binding.canonical_table,
                    binding.node_id,
                    binding.writer_epoch,
                    binding.deadline,
                )
                .is_err()
            {
                self.audited.fetch_add(1, Ordering::SeqCst);
                return Err(TailReadError::Authorization {
                    detail: "query binding mismatch".to_owned(),
                });
            }
            Ok(())
        }

        async fn verify_tail_ticket(
            &self,
            _encoded: &[u8],
            _expected: &TailTicketClaims,
        ) -> Result<(), TailReadError> {
            Ok(())
        }

        async fn verify_tail_capability(
            &self,
            _encoded: &[u8],
            _query_id: uuid::Uuid,
            _tenant_id: DataTenantId,
            _canonical_table: &str,
            _fence: &tail::TailReadFence,
            _audience: TailTicketAudience,
        ) -> Result<(), TailReadError> {
            Ok(())
        }
    }

    /// Builds one direct live-tail service over a memtable holding two
    /// appended batches of `value` rows, so a selective fetch can be compared
    /// against the unfiltered one.
    ///
    /// # Panics
    /// Panics when the fixture batches, seal key, or memtable inserts violate
    /// their construction invariants.
    fn selective_tail_fixture(
        tenant: DataTenantId,
        day: crate::catalog::layout::TimePartition,
        stream: StreamIdentity,
    ) -> (FetchLiveTailService, super::TenantTableBinding) {
        use crate::catalog::TableRef;
        use crate::scribe::seal_key::SealKey;
        use crate::scribe::wal::ScribeAppendMeta;

        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]));
        let batch = |values: Vec<i64>| {
            RecordBatch::try_new(
                Arc::clone(&schema),
                vec![Arc::new(Int64Array::from(values))],
            )
            .expect("valid fixture batch")
        };
        let table = TableRef::new(crate::namespaces::BifrostNamespace::Bifrost, "events");
        let key = SealKey::new(tenant, table.clone(), day);
        let memtable = Arc::new(Memtable::new());
        let event = || wyrd_spec::vala::api::AuditEvent {
            request_id: wyrd_spec::request_id::RequestId::now_v7(),
            trace_id: None,
            operation: "test".to_owned(),
            resource: "vala.bifrost.events".to_owned(),
            card_ref: None,
            principal_id: wyrd_spec::auth::PrincipalId::new(uuid::Uuid::now_v7()),
            principal_kind: wyrd_spec::auth::PrincipalKindTag::User,
            auth_method: wyrd_spec::vala::api::AuthMethod::Jwt,
            permission: "test".to_owned(),
            decision: wyrd_spec::vala::api::AuditDecision::Allow,
            result: wyrd_spec::vala::api::AuditResult::Success,
            payload_summary: "test".to_owned(),
            detail: None,
        };
        let meta = |lsn: u64, rows: usize| ScribeAppendMeta {
            batch_id: *uuid::Uuid::now_v7().as_bytes(),
            schema_fingerprint: [0; 32],
            data_digest: [0; 32],
            data_len: 0,
            payload_digest: [0; 32],
            payload_len: 0,
            slice_index: 0,
            slice_count: 1,
            rows_accepted: rows,
            wal_lsn_min: WalLsn::new(lsn),
            wal_lsn_max: WalLsn::new(lsn),
            seal_key: key.to_string(),
        };
        memtable
            .insert(&key, event(), meta(1, 3), batch(vec![1, 2, 3]))
            .expect("first fixture batch inserts");
        memtable
            .insert(&key, event(), meta(2, 2), batch(vec![4, 5]))
            .expect("second fixture batch inserts");
        let binding =
            super::TenantTableBinding::resolve((tenant, table)).expect("fixture binding resolves");
        (
            FetchLiveTailService::new(stream, memtable, tail_resources()),
            binding,
        )
    }

    /// A signed predicate is applied inside Scribe, so a selective live-tail
    /// fetch returns the same rows the leader would have kept but ships
    /// strictly fewer of them into follower attempt encoding.
    ///
    /// # Panics
    /// Panics when the fixture cannot be built or either snapshot fails.
    #[tokio::test]
    async fn selective_live_tail_fetch_returns_only_signed_rows() {
        use wyrd_spec::vala::assignment_authority::{ScanLiteral, ScanPredicate};

        let tenant = DataTenantId::new_v7();
        let day = crate::test_support::day_partition(2026, 7, 14);
        let stream = StreamIdentity::new(NodeId::generate(), WriterEpoch::new(1));
        let (service, binding) = selective_tail_fixture(tenant, day, stream);
        let request = |predicates: Vec<ScanPredicate>| super::FetchLiveTailRequest {
            binding: binding.clone(),
            target_stream: stream,
            start_partition: day,
            end_partition: day,
            after_lsn: WalLsn::ZERO,
            persisted_lsn_ranges: Vec::new(),
            required_columns: vec!["value".to_owned()],
            predicates,
            max_batches: 64,
            max_retained_bytes: 64 * 1024 * 1024,
        };

        let unfiltered = service
            .fetch_hot_batches(request(Vec::new()))
            .await
            .expect("unfiltered snapshot");
        let unfiltered_rows: usize = unfiltered.iter().map(|batch| batch.rows.num_rows()).sum();
        assert_eq!(unfiltered.len(), 2);
        assert_eq!(unfiltered_rows, 5);

        let selective = service
            .fetch_hot_batches(request(vec![ScanPredicate::Eq(
                "value".to_owned(),
                ScanLiteral::I64(2),
            )]))
            .await
            .expect("selective snapshot");
        // The batch holding no matching row is dropped entirely rather than
        // returned empty, and the surviving batch carries only row `2`.
        assert_eq!(selective.len(), 1);
        let selective_rows: usize = selective.iter().map(|batch| batch.rows.num_rows()).sum();
        assert_eq!(selective_rows, 1);
        assert!(selective_rows < unfiltered_rows);
        let retained = selective[0]
            .rows
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .expect("Int64 value column")
            .value(0);
        assert_eq!(retained, 2);
    }

    /// Local acquire rejects query-B against a query-A ticket before allocation.
    #[tokio::test]
    async fn local_acquire_query_binding_rejects_without_allocation() {
        let tenant = DataTenantId::new_v7();
        let stream = StreamIdentity::new(NodeId::generate(), WriterEpoch::new(1));
        let reader = Arc::new(ScribeTailReader::new(
            Arc::new(FetchLiveTailService::new(
                stream,
                Arc::new(Memtable::new()),
                tail_resources(),
            )),
            TailFenceConfig::default(),
        ));
        let claims = TailTicketClaims {
            query_id: uuid::Uuid::now_v7(),
            tenant_id: tenant,
            canonical_table: "vala.bifrost.events".to_owned(),
            node_id: stream.node_id.as_uuid(),
            writer_epoch: 1,
            deadline: chrono::Utc::now() + chrono::Duration::seconds(5),
            audience: TailTicketAudience::Acquire,
            nonce: vec![7; 16],
        };
        let audited = Arc::new(AtomicUsize::new(0));
        let transport = LocalTailReadTransport::with_authority(
            Arc::clone(&reader),
            Arc::new(BindingProbe {
                claims,
                audited: Arc::clone(&audited),
            }),
        );
        let mut request = empty_fence_request(tenant);
        request.query_id = uuid::Uuid::now_v7();
        assert!(
            transport
                .acquire_fence_with_ticket(request, Vec::new())
                .await
                .is_err()
        );
        assert_eq!(audited.load(Ordering::SeqCst), 1);
        assert_eq!(reader.expire_due(Instant::now(), 64).retained, 0);
    }

    /// Rejects cursor ordering across two independent writer epochs.
    #[test]
    fn cursor_rejects_cross_epoch_comparison() {
        let first = TailCursor {
            writer_epoch: 1,
            wal_lsn: 4,
            batch_id: uuid::Uuid::nil(),
            row_ordinal: 0,
        };
        let second = TailCursor {
            writer_epoch: 2,
            wal_lsn: 1,
            batch_id: uuid::Uuid::nil(),
            row_ordinal: 0,
        };
        assert!(cursor_cmp(&first, &second).is_err());
    }

    /// Rejects a signed ticket tuple when the caller changes the tenant binding.
    #[test]
    fn ticket_binding_rejects_cross_tenant_request() {
        let signed_tenant = DataTenantId::new_v7();
        let requested_tenant = DataTenantId::new_v7();
        let claims = TailTicketClaims {
            query_id: uuid::Uuid::new_v4(),
            tenant_id: signed_tenant,
            canonical_table: "vala.bifrost.events".to_owned(),
            node_id: uuid::Uuid::new_v4(),
            writer_epoch: 7,
            deadline: chrono::Utc::now() + chrono::Duration::seconds(5),
            audience: TailTicketAudience::Acquire,
            nonce: vec![1; 16],
        };
        assert!(matches!(
            claims.validate_binding(
                claims.query_id,
                requested_tenant,
                &claims.canonical_table,
                claims.node_id,
                claims.writer_epoch,
                claims.deadline,
            ),
            Err(TailReadError::Authorization { .. })
        ));
    }

    /// Reclaims abandoned expired capacity during a later production acquisition.
    #[tokio::test]
    async fn opportunistic_acquire_reclaims_expired_capacity() {
        let tenant = DataTenantId::new_v7();
        let stream = StreamIdentity::new(NodeId::generate(), WriterEpoch::new(1));
        let reader = ScribeTailReader::new(
            Arc::new(FetchLiveTailService::new(
                stream,
                Arc::new(Memtable::new()),
                tail_resources(),
            )),
            TailFenceConfig {
                max_fences: 1,
                ..TailFenceConfig::default()
            },
        );
        let first = reader
            .acquire_fence(empty_fence_request(tenant))
            .await
            .expect("first fence consumes capacity");
        {
            let mut registry = reader.fences.lock().expect("registry lock");
            registry
                .retained
                .get_mut(&first.fence_id)
                .expect("first fence retained")
                .expires_at = Instant::now()
                .checked_sub(Duration::from_secs(1))
                .expect("one second is within the monotonic clock range");
        }

        let second = reader
            .acquire_fence(empty_fence_request(tenant))
            .await
            .expect("foreground acquisition reclaims expired capacity");
        let registry = reader.fences.lock().expect("registry lock");
        assert_eq!(registry.retained.len(), 1);
        assert!(!registry.retained.contains_key(&first.fence_id));
        assert!(registry.retained.contains_key(&second.fence_id));
    }

    /// Tail lifecycle inspection reconciles one admitted fence through release.
    #[tokio::test]
    async fn tail_lifecycle_reconciles_plan_reservation_and_release() {
        let tenant = DataTenantId::new_v7();
        let stream = StreamIdentity::new(NodeId::generate(), WriterEpoch::new(1));
        let reader = ScribeTailReader::new(
            Arc::new(FetchLiveTailService::new(
                stream,
                Arc::new(Memtable::new()),
                tail_resources(),
            )),
            TailFenceConfig::default(),
        );
        let fence = reader
            .acquire_fence(empty_fence_request(tenant))
            .await
            .expect("empty fence admission");
        let active = reader
            .ownership_snapshot_for_test()
            .expect("active ownership snapshot");
        assert_eq!(active.plans, 1);
        assert_eq!(active.reservations, 1);
        assert_eq!(active.materializations, 1);
        assert_eq!(active.releases, 0);
        assert_eq!(active.active_reservations, 1);
        assert_eq!(
            active.active_reserved_bytes, active.materialized_bytes,
            "the retained descriptor backing remains root-owned even for an empty fence"
        );
        assert!(active.active_reserved_bytes > 0);

        reader
            .release_fence(fence.fence_id)
            .expect("explicit fence release");
        let released = reader
            .ownership_snapshot_for_test()
            .expect("released ownership snapshot");
        assert_eq!(released.releases, 1);
        assert_eq!(released.active_reservations, 0);
        assert_eq!(released.active_reserved_bytes, 0);
        assert_eq!(released.planned_bytes, released.reserved_bytes);
        assert_eq!(released.reserved_bytes, released.released_bytes);
    }

    /// Nonempty retained owner and its exact pre-acquire root baseline.
    struct RetainedTerminalFixture {
        /// Reader whose production terminal path owns the retained fence.
        reader: ScribeTailReader,
        /// Authoritative root used to prove exact terminal release.
        resources: crate::resources::ScribeResources,
        /// Root bytes in use before the fence reservation.
        baseline_bytes: usize,
        /// Lifecycle facts immediately before terminal settlement.
        active: super::TailOwnershipSnapshot,
    }

    /// Acquires one nonempty retained fence under its production root owner.
    fn retained_terminal_fixture() -> RetainedTerminalFixture {
        let tenant = DataTenantId::new_v7();
        let stream = StreamIdentity::new(NodeId::generate(), WriterEpoch::new(1));
        let resources = tail_resources();
        let baseline_bytes = resources
            .snapshot()
            .expect("baseline root snapshot")
            .scribe_memory_used_bytes;
        let reader = ScribeTailReader::new(
            Arc::new(FetchLiveTailService::new(
                stream,
                Arc::new(Memtable::new()),
                resources.clone(),
            )),
            TailFenceConfig::default(),
        );
        let mut pending = reader
            .reserve_pending_fence()
            .expect("terminal fixture reserves one root-backed owner");
        let batch = Arc::new(
            RecordBatch::try_new(
                Arc::new(Schema::new(vec![Field::new(
                    "value",
                    DataType::Int64,
                    false,
                )])),
                vec![Arc::new(Int64Array::from(vec![7_i64]))],
            )
            .expect("terminal fixture batch"),
        );
        let payload_bytes = batch.get_array_memory_size();
        let batches = vec![RetainedBatch {
            lsn: WalLsn::new(1),
            batch_id: uuid::Uuid::now_v7(),
            rows: batch,
        }];
        let descriptor_bytes = batches
            .capacity()
            .checked_mul(std::mem::size_of::<RetainedBatch>())
            .expect("one retained descriptor fits usize");
        pending
            .materialized(payload_bytes, descriptor_bytes)
            .expect("terminal fixture materializes under its root owner");
        let request = empty_fence_request(tenant);
        let fence = tail::TailReadFence {
            fence_id: tail::TailFenceId::new(uuid::Uuid::now_v7()),
            binding: request.binding,
            time_partition: request.time_partition,
            stream: tail::TailStreamIdentity {
                node_id: tail::NodeId::new(stream.node_id.as_uuid()),
                writer_epoch: 1,
            },
            exclusive_sealed: request.exclusive_sealed,
            inclusive_live: TailCursor {
                writer_epoch: 1,
                wal_lsn: 1,
                batch_id: batches[0].batch_id,
                row_ordinal: 0,
            },
            schema_fingerprint: request.schema_fingerprint,
            tail_protocol_version: 1,
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(5),
        };
        pending
            .retain(fence, batches, Duration::from_secs(5), payload_bytes)
            .expect("terminal fixture retains one nonempty fence");
        let active = reader
            .ownership_snapshot_for_test()
            .expect("active terminal fixture snapshot");
        assert_eq!(active.releases, 0);
        assert_eq!(active.active_reservations, 1);
        assert!(active.active_reserved_bytes > 0);
        assert!(
            resources
                .snapshot()
                .expect("active root snapshot")
                .scribe_memory_used_bytes
                > baseline_bytes
        );
        RetainedTerminalFixture {
            reader,
            resources,
            baseline_bytes,
            active,
        }
    }

    /// Terminal drain settles one nonempty retained owner and is idempotent.
    #[tokio::test]
    async fn terminal_drain_settles_retained_owner_once() {
        let fixture = retained_terminal_fixture();
        let drained = fixture
            .reader
            .drain_terminal()
            .expect("production terminal drain settles the retained owner");
        assert_eq!(drained.releases, fixture.active.releases + 1);
        assert_eq!(
            drained.released_bytes,
            fixture.active.released_bytes + fixture.active.active_reserved_bytes
        );
        assert_eq!(drained.active_reservations, 0);
        assert_eq!(drained.active_reserved_bytes, 0);
        assert_eq!(
            fixture
                .resources
                .snapshot()
                .expect("terminal root snapshot")
                .scribe_memory_used_bytes,
            fixture.baseline_bytes
        );
        assert_eq!(
            fixture
                .reader
                .drain_terminal()
                .expect("second terminal drain is idempotent"),
            drained
        );
        assert!(matches!(
            fixture
                .reader
                .acquire_fence(empty_fence_request(DataTenantId::new_v7()))
                .await,
            Err(TailReadError::Capacity)
        ));
    }

    /// Terminal close waits for a pending guard to settle its own root owner.
    #[test]
    fn terminal_drain_preserves_pending_owner_until_cancellation() {
        let stream = StreamIdentity::new(NodeId::generate(), WriterEpoch::new(1));
        let resources = tail_resources();
        let baseline = resources.snapshot().expect("baseline root snapshot");
        let reader = ScribeTailReader::new(
            Arc::new(FetchLiveTailService::new(
                stream,
                Arc::new(Memtable::new()),
                resources.clone(),
            )),
            TailFenceConfig::default(),
        );
        let pending = reader
            .reserve_pending_fence()
            .expect("pending terminal fixture owns one reservation");
        let active = reader
            .ownership_snapshot_for_test()
            .expect("pending terminal snapshot");
        assert_eq!(active.active_reservations, 1);
        assert!(reader.drain_terminal().is_err());
        assert_eq!(
            reader
                .ownership_snapshot_for_test()
                .expect("terminal close preserves pending owner"),
            active
        );

        drop(pending);
        let cancelled = reader
            .ownership_snapshot_for_test()
            .expect("pending cancellation settles terminal owner");
        assert_eq!(cancelled.releases, 1);
        assert_eq!(cancelled.active_reservations, 0);
        assert_eq!(cancelled.active_reserved_bytes, 0);
        assert_eq!(cancelled.reserved_bytes, cancelled.released_bytes);
        assert_eq!(
            reader
                .drain_terminal()
                .expect("terminal retry after cancellation is idempotent"),
            cancelled
        );
        assert_eq!(
            resources
                .snapshot()
                .expect("cancelled terminal root snapshot")
                .scribe_memory_used_bytes,
            baseline.scribe_memory_used_bytes
        );
    }

    /// Refuses a concurrent fence before materialization and settles cancellation once.
    #[tokio::test]
    async fn pending_fence_bounds_concurrency_and_cancellation() {
        let tenant = DataTenantId::new_v7();
        let stream = StreamIdentity::new(NodeId::generate(), WriterEpoch::new(1));
        let resources = tail_resources();
        let reader = ScribeTailReader::new(
            Arc::new(FetchLiveTailService::new(
                stream,
                Arc::new(Memtable::new()),
                resources.clone(),
            )),
            TailFenceConfig {
                max_fences: 1,
                ..TailFenceConfig::default()
            },
        );
        let baseline = resources.snapshot().expect("baseline root snapshot");
        let pending = reader
            .reserve_pending_fence()
            .expect("first acquisition owns the only fence slot");
        assert!(matches!(
            reader.acquire_fence(empty_fence_request(tenant)).await,
            Err(TailReadError::Capacity)
        ));
        let before_materialization = reader
            .ownership_snapshot_for_test()
            .expect("pending ownership snapshot");
        assert_eq!(before_materialization.reservations, 1);
        assert_eq!(before_materialization.materializations, 0);
        assert_eq!(before_materialization.active_reservations, 1);
        assert!(before_materialization.active_reserved_bytes > 0);
        drop(pending);
        let cancelled_before = reader
            .ownership_snapshot_for_test()
            .expect("pre-material cancellation snapshot");
        assert_eq!(cancelled_before.releases, 1);
        assert_eq!(cancelled_before.materializations, 0);
        assert_eq!(cancelled_before.active_reservations, 0);

        let mut pending = reader
            .reserve_pending_fence()
            .expect("a settled cancellation returns the slot");

        pending
            .materialized(0, 0)
            .expect("empty materialization shrinks to no live bytes");
        drop(pending);
        let cancelled = reader
            .ownership_snapshot_for_test()
            .expect("cancelled ownership snapshot");
        assert_eq!(cancelled.reservations, 2);
        assert_eq!(cancelled.materializations, 1);
        assert_eq!(cancelled.releases, 2);
        assert_eq!(cancelled.active_reservations, 0);
        assert_eq!(cancelled.active_reserved_bytes, 0);
        assert_eq!(cancelled.reserved_bytes, cancelled.released_bytes);
        assert_eq!(
            resources
                .snapshot()
                .expect("settled root snapshot")
                .scribe_memory_used_bytes,
            baseline.scribe_memory_used_bytes
        );
    }

    /// Bounds the release-tombstone map by memory only, never by live admission.
    ///
    /// Tombstones no longer consume ownership capacity, so accumulating them never
    /// blocks acquisition; the map is instead held at `max_fences * 4` by evicting
    /// the earliest-expiry entry, and expired tombstones remain purged on reclaim.
    #[tokio::test]
    async fn release_tombstones_respect_configured_capacity() {
        let tenant = DataTenantId::new_v7();
        let stream = StreamIdentity::new(NodeId::generate(), WriterEpoch::new(1));
        let reader = ScribeTailReader::new(
            Arc::new(FetchLiveTailService::new(
                stream,
                Arc::new(Memtable::new()),
                tail_resources(),
            )),
            TailFenceConfig {
                max_fences: 2,
                ..TailFenceConfig::default()
            },
        );

        // Fill the tombstone map to exactly its cap. Each acquire succeeds even as
        // tombstones accumulate, proving live admission counts retained fences only.
        let cap = 2 * 4;
        let mut tombstones = Vec::new();
        for _ in 0..cap {
            let fence = reader
                .acquire_fence(empty_fence_request(tenant))
                .await
                .expect("tombstones never block live admission");
            assert!(
                reader
                    .release_fence(fence.fence_id)
                    .expect("release creates tombstone")
                    .released
            );
            tombstones.push(fence.fence_id);
        }

        // Impose a strict expiry order so eviction is deterministic: the target is
        // the unique earliest-expiry tombstone, and every tombstone stays in the
        // future so the opportunistic reclaim spares them.
        let target = tombstones[3];
        {
            let mut registry = reader.fences.lock().expect("registry lock");
            assert_eq!(registry.released.len(), cap);
            let base = Instant::now();
            for (offset, fence_id) in tombstones.iter().enumerate() {
                let (expiry, _) = registry
                    .released
                    .get_mut(fence_id)
                    .expect("tombstone present");
                let secs = if *fence_id == target {
                    1
                } else {
                    100 + offset as u64
                };
                *expiry = base + Duration::from_secs(secs);
            }
        }

        // One more acquire/release overflows the cap; the earliest-expiry tombstone
        // (the target) is evicted and the map stays at its cap.
        let overflow = reader
            .acquire_fence(empty_fence_request(tenant))
            .await
            .expect("tombstone cap does not block live admission");
        assert!(
            reader
                .release_fence(overflow.fence_id)
                .expect("overflow release")
                .released
        );
        {
            let registry = reader.fences.lock().expect("registry lock");
            assert_eq!(registry.released.len(), cap, "tombstone map stays capped");
            assert!(
                !registry.released.contains_key(&target),
                "earliest-expiry tombstone is evicted first"
            );
            assert!(registry.released.contains_key(&overflow.fence_id));
            assert!(registry.retained.is_empty());
            assert_eq!(registry.retained_bytes, 0);
        }

        // A duplicate release of a surviving tombstone stays idempotent; the evicted
        // fence — now in neither map — reports released: false, matching the
        // post-TTL semantics every caller already tolerates.
        assert!(
            reader
                .release_fence(tombstones[4])
                .expect("surviving tombstone duplicate release")
                .released
        );
        assert!(
            !reader
                .release_fence(target)
                .expect("evicted fence release")
                .released
        );

        // Expired tombstones are still purged on the next opportunistic reclaim.
        {
            let mut registry = reader.fences.lock().expect("registry lock");
            for (expiry, _) in registry.released.values_mut() {
                *expiry = Instant::now()
                    .checked_sub(Duration::from_secs(1))
                    .expect("one second is within monotonic range");
            }
        }
        reader
            .acquire_fence(empty_fence_request(tenant))
            .await
            .expect("expired tombstones permit a new fence");
        {
            let registry = reader.fences.lock().expect("registry lock");
            assert!(registry.released.is_empty(), "expired tombstones purged");
            assert_eq!(registry.retained.len(), 1);
        }
    }

    /// Concurrent duplicate page reads observe the same immutable fence snapshot.
    #[tokio::test]
    async fn concurrent_duplicate_pages_are_equal() {
        let tenant = DataTenantId::new_v7();
        let stream = StreamIdentity::new(NodeId::generate(), WriterEpoch::new(1));
        let reader = Arc::new(ScribeTailReader::new(
            Arc::new(FetchLiveTailService::new(
                stream,
                Arc::new(Memtable::new()),
                tail_resources(),
            )),
            TailFenceConfig::default(),
        ));
        let fence = reader
            .acquire_fence(empty_fence_request(tenant))
            .await
            .expect("fence acquires");
        let request = TailPageRequest {
            query_id: uuid::Uuid::nil(),
            fence_id: fence.fence_id,
            after: None,
            max_rows: 16,
            max_encoded_bytes: 4096,
        };
        let left_reader = Arc::clone(&reader);
        let right_reader = Arc::clone(&reader);
        let left_request = request.clone();
        let right_request = request;
        let (left, right) = tokio::join!(
            tokio::task::spawn_blocking(move || left_reader.read_page(&left_request)),
            tokio::task::spawn_blocking(move || right_reader.read_page(&right_request)),
        );
        let left = left.expect("left task").expect("left page");
        let right = right.expect("right task").expect("right page");
        assert_eq!(left.complete, right.complete);
        assert_eq!(left.next, right.next);
        assert_eq!(left.batches, right.batches);
    }
}
