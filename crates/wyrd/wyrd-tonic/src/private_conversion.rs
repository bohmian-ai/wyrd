//! Validated protobuf conversions for private Scribe and Oracle peer contracts.

use std::str::FromStr;

use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api as domain;
use wyrd_spec::vala::assignment_authority;
use wyrd_spec::DataTenantId;

use crate::wyrd::v1 as proto;

/// Hard protocol ceiling for opaque signed claims.
const MAX_CLAIMS_BYTES: usize = 16 * 1024;
/// Exact v1 peer signature width.
const SIGNATURE_BYTES: usize = 64;
/// Hard protocol ceiling for an ASCII signing-key identifier.
const MAX_KEY_ID_BYTES: usize = 64;

/// Decodes one required [`proto::TimePartition`] into its validated domain value.
///
/// The protobuf enum's zero tag means "unspecified" and is rejected rather than
/// defaulted, and a start instant that is not the exact UTC boundary of its
/// granularity is rejected before any caller can act on the message.
///
/// # Errors
/// Returns [`PrivateConversionError::Missing`] when the nested message is
/// absent, [`PrivateConversionError::RequiredEnum`] for tag `0` or an unknown
/// tag, and [`PrivateConversionError::Invalid`] for a noncanonical or
/// unrepresentable start.
fn time_partition(
    value: Option<proto::TimePartition>,
    field: &'static str,
) -> Result<domain::TimePartitionWire, PrivateConversionError> {
    let value = value.ok_or(PrivateConversionError::Missing(field))?;
    let granularity = match proto::TimeGranularity::try_from(value.granularity) {
        Ok(proto::TimeGranularity::Hour) => domain::TimeGranularityWire::Hour,
        Ok(proto::TimeGranularity::Day) => domain::TimeGranularityWire::Day,
        Ok(proto::TimeGranularity::Unspecified) | Err(_) => {
            return Err(PrivateConversionError::RequiredEnum(field));
        }
    };
    let start = chrono::DateTime::from_timestamp_micros(value.start_unix_micros)
        .ok_or(PrivateConversionError::Invalid { field })?;
    domain::TimePartitionWire::new(granularity, start)
        .map_err(|_| PrivateConversionError::Invalid { field })
}

/// Encodes one validated partition value into its protobuf message.
fn time_partition_proto(value: domain::TimePartitionWire) -> proto::TimePartition {
    proto::TimePartition {
        granularity: match value.granularity() {
            domain::TimeGranularityWire::Hour => proto::TimeGranularity::Hour as i32,
            domain::TimeGranularityWire::Day => proto::TimeGranularity::Day as i32,
        },
        start_unix_micros: value.start_unix_micros(),
    }
}

/// Error returned before malformed private input reaches a runtime owner.
#[derive(Debug, thiserror::Error)]
pub enum PrivateConversionError {
    /// A required nested message or oneof was absent.
    #[error("required protobuf field `{0}` is missing")]
    Missing(&'static str),
    /// A UUID byte field was not exactly 16 bytes or a UUID string was invalid.
    #[error("protobuf UUID field `{0}` is malformed")]
    InvalidUuid(&'static str),
    /// A required enum used its protobuf zero or unknown value.
    #[error("required protobuf enum `{0}` is unspecified or unknown")]
    RequiredEnum(&'static str),
    /// A bounded field exceeded its protocol maximum.
    #[error("protobuf field `{field}` exceeds its protocol bound")]
    TooLarge {
        /// Field that exceeded its bound.
        field: &'static str,
    },
    /// A required field violated a closed protocol invariant.
    #[error("protobuf field `{field}` is invalid")]
    Invalid {
        /// Field that violated its invariant.
        field: &'static str,
    },
}

impl TryFrom<proto::TimePartition> for domain::TimePartitionWire {
    type Error = PrivateConversionError;

    /// Decodes one partition identity and revalidates its exact boundary.
    ///
    /// The private wire carries granularity and start micros separately, so the
    /// decoded pair is re-checked against the canonical boundary rule before any
    /// runtime owner observes it.
    ///
    /// # Errors
    ///
    /// Returns [`PrivateConversionError::RequiredEnum`] when the granularity is
    /// unspecified or unknown, and [`PrivateConversionError::Invalid`] when the
    /// start instant is unrepresentable or is not the exact start of its unit.
    fn try_from(value: proto::TimePartition) -> Result<Self, Self::Error> {
        time_partition(Some(value), "time_partition")
    }
}

impl From<domain::TimePartitionWire> for proto::TimePartition {
    /// Encodes one already-validated partition identity onto the private wire.
    fn from(value: domain::TimePartitionWire) -> Self {
        time_partition_proto(value)
    }
}

impl TryFrom<proto::TailCursor> for domain::TailCursor {
    type Error = PrivateConversionError;

    /// Decodes a tail cursor and validates its batch UUID.
    ///
    /// # Errors
    /// Returns [`PrivateConversionError`] when `batch_id` is not a UUID.
    fn try_from(value: proto::TailCursor) -> Result<Self, Self::Error> {
        Ok(Self {
            writer_epoch: value.writer_epoch,
            wal_lsn: value.wal_lsn,
            batch_id: uuid_bytes(&value.batch_id, "batch_id")?,
            row_ordinal: value.row_ordinal,
        })
    }
}

impl From<domain::TailCursor> for proto::TailCursor {
    /// Encodes a validated tail cursor for the private peer wire.
    fn from(value: domain::TailCursor) -> Self {
        Self {
            writer_epoch: value.writer_epoch,
            wal_lsn: value.wal_lsn,
            batch_id: value.batch_id.as_bytes().to_vec(),
            row_ordinal: value.row_ordinal,
        }
    }
}

impl TryFrom<proto::TenantTableBinding> for domain::TenantTableBinding {
    type Error = PrivateConversionError;

    /// Decodes an authenticated tenant/table binding.
    ///
    /// # Errors
    /// Returns [`PrivateConversionError`] for an invalid tenant UUID or empty
    /// namespace or table name.
    fn try_from(value: proto::TenantTableBinding) -> Result<Self, Self::Error> {
        nonempty(&value.namespace, "namespace")?;
        nonempty(&value.table, "table")?;
        Ok(Self {
            tenant_id: wyrd_spec::DataTenantId::from_str(&value.tenant_id)
                .map_err(|_| PrivateConversionError::InvalidUuid("tenant_id"))?,
            namespace: value.namespace,
            table: value.table,
        })
    }
}

impl From<domain::TenantTableBinding> for proto::TenantTableBinding {
    /// Encodes an authenticated tenant/table binding for the private wire.
    fn from(value: domain::TenantTableBinding) -> Self {
        Self {
            tenant_id: value.tenant_id.to_string(),
            namespace: value.namespace,
            table: value.table,
        }
    }
}

impl TryFrom<proto::TailStreamIdentity> for domain::TailStreamIdentity {
    type Error = PrivateConversionError;

    /// Decodes the node and writer epoch identifying one live-tail stream.
    ///
    /// # Errors
    /// Returns [`PrivateConversionError`] when the node identifier is not a UUID.
    fn try_from(value: proto::TailStreamIdentity) -> Result<Self, Self::Error> {
        Ok(Self {
            node_id: domain::NodeId::new(uuid_string(&value.node_id, "node_id")?),
            writer_epoch: value.writer_epoch,
        })
    }
}

impl From<domain::TailStreamIdentity> for proto::TailStreamIdentity {
    /// Encodes one typed live-tail stream identity.
    fn from(value: domain::TailStreamIdentity) -> Self {
        Self {
            node_id: value.node_id.as_uuid().to_string(),
            writer_epoch: value.writer_epoch,
        }
    }
}

impl TryFrom<proto::AcquireTailFenceRequest> for domain::AcquireTailFenceRequest {
    type Error = PrivateConversionError;

    /// Decodes and validates all bounds of a tail-fence acquisition request.
    ///
    /// # Errors
    /// Returns [`PrivateConversionError`] for missing nested values, invalid
    /// identifiers, dates, timestamps, fingerprints, or protocol versions.
    fn try_from(value: proto::AcquireTailFenceRequest) -> Result<Self, Self::Error> {
        let version = u16::try_from(value.tail_protocol_version).map_err(|_| {
            PrivateConversionError::Invalid {
                field: "tail_protocol_version",
            }
        })?;
        protocol_v1(version, "tail_protocol_version")?;
        Ok(Self {
            query_id: uuid_bytes(&value.query_id, "query_id")?,
            binding: value
                .binding
                .ok_or(PrivateConversionError::Missing("binding"))?
                .try_into()?,
            time_partition: time_partition(value.time_partition, "time_partition")?,
            exclusive_sealed: value
                .exclusive_sealed
                .ok_or(PrivateConversionError::Missing("exclusive_sealed"))?
                .try_into()?,
            deadline: datetime(value.deadline_unix_ms, "deadline_unix_ms")?,
            schema_fingerprint: domain::SchemaFingerprint::new(value.schema_fingerprint).map_err(
                |_| PrivateConversionError::Invalid {
                    field: "schema_fingerprint",
                },
            )?,
            tail_protocol_version: version,
        })
    }
}

impl From<domain::AcquireTailFenceRequest> for proto::AcquireTailFenceRequest {
    /// Encodes a validated tail-fence acquisition request.
    fn from(value: domain::AcquireTailFenceRequest) -> Self {
        Self {
            query_id: value.query_id.as_bytes().to_vec(),
            binding: Some(value.binding.into()),
            time_partition: Some(time_partition_proto(value.time_partition)),
            exclusive_sealed: Some(value.exclusive_sealed.into()),
            deadline_unix_ms: unix_millis(value.deadline),
            schema_fingerprint: value.schema_fingerprint.as_str().to_owned(),
            tail_protocol_version: u32::from(value.tail_protocol_version),
            tail_ticket: Vec::new(),
        }
    }
}

impl TryFrom<proto::TailReadFence> for domain::TailReadFence {
    type Error = PrivateConversionError;

    /// Decodes a read fence and verifies its stream interval and protocol.
    ///
    /// # Errors
    /// Returns [`PrivateConversionError`] for malformed or missing fields, an
    /// unsupported protocol, or cursors inconsistent with the fenced stream.
    fn try_from(value: proto::TailReadFence) -> Result<Self, Self::Error> {
        let version = u16::try_from(value.tail_protocol_version).map_err(|_| {
            PrivateConversionError::Invalid {
                field: "tail_protocol_version",
            }
        })?;
        protocol_v1(version, "tail_protocol_version")?;
        let stream: domain::TailStreamIdentity = value
            .stream
            .ok_or(PrivateConversionError::Missing("stream"))?
            .try_into()?;
        let exclusive_sealed: domain::TailCursor = value
            .exclusive_sealed
            .ok_or(PrivateConversionError::Missing("exclusive_sealed"))?
            .try_into()?;
        let inclusive_live: domain::TailCursor = value
            .inclusive_live
            .ok_or(PrivateConversionError::Missing("inclusive_live"))?
            .try_into()?;
        if exclusive_sealed.writer_epoch != stream.writer_epoch
            || inclusive_live.writer_epoch != stream.writer_epoch
            || inclusive_live.wal_lsn < exclusive_sealed.wal_lsn
        {
            return Err(PrivateConversionError::Invalid {
                field: "tail_interval",
            });
        }
        Ok(Self {
            fence_id: domain::TailFenceId::new(uuid_bytes(&value.fence_id, "fence_id")?),
            binding: value
                .binding
                .ok_or(PrivateConversionError::Missing("binding"))?
                .try_into()?,
            time_partition: time_partition(value.time_partition, "time_partition")?,
            stream,
            exclusive_sealed,
            inclusive_live,
            schema_fingerprint: domain::SchemaFingerprint::new(value.schema_fingerprint).map_err(
                |_| PrivateConversionError::Invalid {
                    field: "schema_fingerprint",
                },
            )?,
            tail_protocol_version: version,
            expires_at: datetime(value.expires_at_unix_ms, "expires_at_unix_ms")?,
        })
    }
}

impl From<domain::TailReadFence> for proto::TailReadFence {
    /// Encodes a validated immutable tail-read fence.
    fn from(value: domain::TailReadFence) -> Self {
        Self {
            fence_id: value.fence_id.as_uuid().as_bytes().to_vec(),
            binding: Some(value.binding.into()),
            time_partition: Some(time_partition_proto(value.time_partition)),
            stream: Some(value.stream.into()),
            exclusive_sealed: Some(value.exclusive_sealed.into()),
            inclusive_live: Some(value.inclusive_live.into()),
            schema_fingerprint: value.schema_fingerprint.as_str().to_owned(),
            tail_protocol_version: u32::from(value.tail_protocol_version),
            expires_at_unix_ms: unix_millis(value.expires_at),
            capability: Vec::new(),
        }
    }
}

impl TryFrom<proto::TailPageRequest> for domain::TailPageRequest {
    type Error = PrivateConversionError;

    /// Decodes one bounded page request against an acquired fence.
    ///
    /// # Errors
    /// Returns [`PrivateConversionError`] for a malformed fence or cursor UUID,
    /// or for non-positive row and encoded-byte limits.
    fn try_from(value: proto::TailPageRequest) -> Result<Self, Self::Error> {
        positive(value.max_rows, "max_rows")?;
        positive(value.max_encoded_bytes, "max_encoded_bytes")?;
        Ok(Self {
            query_id: uuid_bytes(&value.query_id, "query_id")?,
            fence_id: domain::TailFenceId::new(uuid_bytes(&value.fence_id, "fence_id")?),
            after: value
                .after_cursor
                .map(|cursor| match cursor {
                    proto::tail_page_request::AfterCursor::After(value) => value.try_into(),
                })
                .transpose()?,
            max_rows: value.max_rows,
            max_encoded_bytes: value.max_encoded_bytes,
        })
    }
}

impl From<domain::TailPageRequest> for proto::TailPageRequest {
    /// Encodes one validated bounded tail-page request.
    fn from(value: domain::TailPageRequest) -> Self {
        Self {
            query_id: value.query_id.as_bytes().to_vec(),
            fence_id: value.fence_id.as_uuid().as_bytes().to_vec(),
            after_cursor: value
                .after
                .map(Into::into)
                .map(proto::tail_page_request::AfterCursor::After),
            max_rows: value.max_rows,
            max_encoded_bytes: value.max_encoded_bytes,
            tail_capability: Vec::new(),
        }
    }
}

impl TryFrom<proto::TailPage> for domain::TailPage {
    type Error = PrivateConversionError;

    /// Decodes a tail page and its optional continuation cursor.
    ///
    /// # Errors
    /// Returns [`PrivateConversionError`] when the continuation cursor carries
    /// a malformed batch UUID.
    fn try_from(value: proto::TailPage) -> Result<Self, Self::Error> {
        Ok(Self {
            batches: value.arrow_ipc_batches,
            next: value
                .next_cursor
                .map(|cursor| match cursor {
                    proto::tail_page::NextCursor::Next(value) => value.try_into(),
                })
                .transpose()?,
            complete: value.complete,
        })
    }
}

impl From<domain::TailPage> for proto::TailPage {
    /// Encodes tail batches and their optional continuation cursor.
    fn from(value: domain::TailPage) -> Self {
        Self {
            arrow_ipc_batches: value.batches,
            next_cursor: value
                .next
                .map(Into::into)
                .map(proto::tail_page::NextCursor::Next),
            complete: value.complete,
        }
    }
}

impl TryFrom<proto::ReleaseTailFenceRequest> for domain::ReleaseTailFenceRequest {
    type Error = PrivateConversionError;

    /// Decodes the exact fence identity to release.
    ///
    /// # Errors
    /// Returns [`PrivateConversionError`] when the fence identifier is not a UUID.
    fn try_from(value: proto::ReleaseTailFenceRequest) -> Result<Self, Self::Error> {
        Ok(Self {
            query_id: uuid_bytes(&value.query_id, "query_id")?,
            fence_id: domain::TailFenceId::new(uuid_bytes(&value.fence_id, "fence_id")?),
        })
    }
}

impl From<domain::ReleaseTailFenceRequest> for proto::ReleaseTailFenceRequest {
    /// Encodes the exact fence identity to release.
    fn from(value: domain::ReleaseTailFenceRequest) -> Self {
        Self {
            query_id: value.query_id.as_bytes().to_vec(),
            fence_id: value.fence_id.as_uuid().as_bytes().to_vec(),
            tail_capability: Vec::new(),
        }
    }
}

impl TryFrom<proto::OracleRoleFence> for domain::OracleRoleFence {
    type Error = PrivateConversionError;

    /// Decodes one exact private participant role fence.
    ///
    /// # Errors
    /// Returns [`PrivateConversionError`] for malformed node identities, an
    /// unspecified role, or a zero fencing token.
    fn try_from(value: proto::OracleRoleFence) -> Result<Self, Self::Error> {
        if value.fencing_token == 0 {
            return Err(PrivateConversionError::Invalid {
                field: "fencing_token",
            });
        }
        Ok(Self {
            node_id: domain::NodeId::new(uuid_string(&value.node_id, "node_id")?),
            role: cluster_role(value.role)?,
            fencing_token: value.fencing_token,
        })
    }
}

impl From<domain::OracleRoleFence> for proto::OracleRoleFence {
    /// Encodes one exact private participant role fence.
    fn from(value: domain::OracleRoleFence) -> Self {
        Self {
            node_id: value.node_id.as_uuid().to_string(),
            role: match value.role {
                domain::ClusterRole::Scribe => proto::ClusterRole::Scribe as i32,
                domain::ClusterRole::Oracle => proto::ClusterRole::Oracle as i32,
            },
            fencing_token: value.fencing_token,
        }
    }
}

impl TryFrom<proto::ListOracleLifecyclesRequest> for domain::ListOracleLifecyclesRequest {
    type Error = PrivateConversionError;

    /// Decodes an owner-local tenant lifecycle listing.
    ///
    /// # Errors
    /// Returns a conversion error for a malformed tenant identity.
    fn try_from(value: proto::ListOracleLifecyclesRequest) -> Result<Self, Self::Error> {
        Ok(Self {
            tenant_id: DataTenantId::from_str(&value.tenant_id)
                .map_err(|_| PrivateConversionError::InvalidUuid("tenant_id"))?,
        })
    }
}

impl From<domain::ListOracleLifecyclesRequest> for proto::ListOracleLifecyclesRequest {
    /// Encodes an owner-local tenant lifecycle listing.
    fn from(value: domain::ListOracleLifecyclesRequest) -> Self {
        Self {
            tenant_id: value.tenant_id.to_string(),
        }
    }
}

impl TryFrom<proto::GetOracleLifecycleRequest> for domain::OracleLifecycleLookupRequest {
    type Error = PrivateConversionError;

    /// Decodes an owner-local lifecycle get lookup.
    ///
    /// # Errors
    /// Returns a conversion error for malformed tenant or request identities.
    fn try_from(value: proto::GetOracleLifecycleRequest) -> Result<Self, Self::Error> {
        decode_lifecycle_lookup(&value.tenant_id, &value.request_id)
    }
}

impl From<domain::OracleLifecycleLookupRequest> for proto::GetOracleLifecycleRequest {
    /// Encodes an owner-local lifecycle get lookup.
    fn from(value: domain::OracleLifecycleLookupRequest) -> Self {
        Self {
            tenant_id: value.tenant_id.to_string(),
            request_id: value.request_id.to_string(),
        }
    }
}

impl TryFrom<proto::ListOracleLifecyclesResponse> for domain::ListOracleLifecyclesResponse {
    type Error = PrivateConversionError;

    /// Decodes every owner-local lifecycle summary in stable response order.
    ///
    /// # Errors
    /// Returns a conversion error when any nested running-query summary is malformed.
    fn try_from(value: proto::ListOracleLifecyclesResponse) -> Result<Self, Self::Error> {
        let queries = value
            .queries
            .into_iter()
            .map(|query| {
                domain::RunningQuerySummary::try_from(query).map_err(|_| {
                    PrivateConversionError::Invalid {
                        field: "running_query_summary",
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { queries })
    }
}

impl From<domain::ListOracleLifecyclesResponse> for proto::ListOracleLifecyclesResponse {
    /// Encodes owner-local lifecycle summaries without changing their order.
    fn from(value: domain::ListOracleLifecyclesResponse) -> Self {
        Self {
            queries: value.queries.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<proto::GetOracleLifecycleResponse> for domain::GetOracleLifecycleResponse {
    type Error = PrivateConversionError;

    /// Decodes the required owner-local lifecycle summary.
    ///
    /// # Errors
    /// Returns a conversion error when the summary is absent or malformed.
    fn try_from(value: proto::GetOracleLifecycleResponse) -> Result<Self, Self::Error> {
        let query = value
            .query
            .ok_or(PrivateConversionError::Missing("query"))?;
        Ok(Self {
            query: domain::RunningQuerySummary::try_from(query).map_err(|_| {
                PrivateConversionError::Invalid {
                    field: "running_query_summary",
                }
            })?,
        })
    }
}

impl From<domain::GetOracleLifecycleResponse> for proto::GetOracleLifecycleResponse {
    /// Encodes the required owner-local lifecycle summary.
    fn from(value: domain::GetOracleLifecycleResponse) -> Self {
        Self {
            query: Some(value.query.into()),
        }
    }
}

impl TryFrom<proto::CancelOracleLifecycleRequest> for domain::CancelOracleLifecycleRequest {
    type Error = PrivateConversionError;

    /// Decodes a tenant-qualified owner-local cancellation request.
    ///
    /// # Errors
    /// Returns a conversion error for malformed tenant or request identities.
    fn try_from(value: proto::CancelOracleLifecycleRequest) -> Result<Self, Self::Error> {
        Ok(Self {
            tenant_id: DataTenantId::from_str(&value.tenant_id)
                .map_err(|_| PrivateConversionError::InvalidUuid("tenant_id"))?,
            request_id: RequestId::parse(&value.request_id).map_err(|_| {
                PrivateConversionError::Invalid {
                    field: "request_id",
                }
            })?,
        })
    }
}

impl From<domain::CancelOracleLifecycleRequest> for proto::CancelOracleLifecycleRequest {
    /// Encodes a tenant-qualified owner-local cancellation request.
    fn from(value: domain::CancelOracleLifecycleRequest) -> Self {
        Self {
            tenant_id: value.tenant_id.to_string(),
            request_id: value.request_id.to_string(),
        }
    }
}

impl From<domain::CancelOracleLifecycleResponse> for proto::CancelOracleLifecycleResponse {
    /// Encodes the idempotent owner-local cancellation acknowledgement.
    fn from(value: domain::CancelOracleLifecycleResponse) -> Self {
        Self {
            request_id: value.request_id.to_string(),
            cancellation_started: value.cancellation_started,
        }
    }
}

impl TryFrom<proto::CancelOracleLifecycleResponse> for domain::CancelOracleLifecycleResponse {
    type Error = PrivateConversionError;

    /// Decodes the idempotent owner-local cancellation acknowledgement.
    ///
    /// # Errors
    /// Returns a conversion error for a malformed request identity.
    fn try_from(value: proto::CancelOracleLifecycleResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            request_id: RequestId::parse(&value.request_id).map_err(|_| {
                PrivateConversionError::Invalid {
                    field: "request_id",
                }
            })?,
            cancellation_started: value.cancellation_started,
        })
    }
}

impl TryFrom<proto::ReserveNodeSlotsRequest> for domain::ReserveNodeSlotsRequest {
    type Error = PrivateConversionError;

    /// Decodes a fenced Oracle capacity-reservation request.
    ///
    /// # Errors
    /// Returns [`PrivateConversionError`] for malformed identifiers, an
    /// unknown class, zero capacity, or an invalid expiry timestamp.
    fn try_from(value: proto::ReserveNodeSlotsRequest) -> Result<Self, Self::Error> {
        positive(value.slot_units, "slot_units")?;
        Ok(Self {
            query_id: domain::QueryId::new(uuid_bytes(&value.query_id, "query_id")?),
            leader_node_id: domain::NodeId::new(uuid_string(
                &value.leader_node_id,
                "leader_node_id",
            )?),
            leader_fencing_token: value.leader_fencing_token,
            query_class: query_class(value.query_class)?,
            slot_units: value.slot_units,
            expires_at: datetime(value.expires_at_unix_ms, "expires_at_unix_ms")?,
        })
    }
}

impl From<domain::ReserveNodeSlotsRequest> for proto::ReserveNodeSlotsRequest {
    /// Encodes a validated Oracle capacity-reservation request.
    fn from(value: domain::ReserveNodeSlotsRequest) -> Self {
        Self {
            query_id: value.query_id.as_uuid().as_bytes().to_vec(),
            leader_node_id: value.leader_node_id.as_uuid().to_string(),
            leader_fencing_token: value.leader_fencing_token,
            query_class: match value.query_class {
                domain::QueryClass::Interactive => proto::QueryClass::Interactive as i32,
                domain::QueryClass::Analytical => proto::QueryClass::Analytical as i32,
            },
            slot_units: value.slot_units,
            expires_at_unix_ms: unix_millis(value.expires_at),
        }
    }
}

impl TryFrom<proto::ReserveNodeSlotsResponse> for domain::ReserveNodeSlotsResponse {
    type Error = PrivateConversionError;

    /// Decodes the closed pending-or-rejected reservation outcome.
    ///
    /// # Errors
    /// Returns [`PrivateConversionError`] when the outcome is absent, a pending
    /// reservation is malformed, or a rejection has no positive retry delay.
    fn try_from(value: proto::ReserveNodeSlotsResponse) -> Result<Self, Self::Error> {
        match value
            .outcome
            .ok_or(PrivateConversionError::Missing("outcome"))?
        {
            proto::reserve_node_slots_response::Outcome::Pending(value) => {
                Ok(Self::Pending(domain::PendingNodeReservation {
                    reservation_id: domain::ReservationId::new(uuid_bytes(
                        &value.reservation_id,
                        "reservation_id",
                    )?),
                    expires_at: datetime(value.expires_at_unix_ms, "expires_at_unix_ms")?,
                }))
            }
            proto::reserve_node_slots_response::Outcome::Rejected(value) => {
                positive(value.retry_after_ms, "retry_after_ms")?;
                Ok(Self::Rejected(domain::ReservationRejected {
                    retry_after_ms: value.retry_after_ms,
                }))
            }
        }
    }
}

impl From<domain::ReserveNodeSlotsResponse> for proto::ReserveNodeSlotsResponse {
    /// Encodes the closed admission outcome into its protobuf oneof.
    fn from(value: domain::ReserveNodeSlotsResponse) -> Self {
        let outcome = match value {
            domain::ReserveNodeSlotsResponse::Pending(value) => {
                proto::reserve_node_slots_response::Outcome::Pending(
                    proto::PendingNodeReservation {
                        reservation_id: value.reservation_id.as_uuid().as_bytes().to_vec(),
                        expires_at_unix_ms: unix_millis(value.expires_at),
                    },
                )
            }
            domain::ReserveNodeSlotsResponse::Rejected(value) => {
                proto::reserve_node_slots_response::Outcome::Rejected(proto::ReservationRejected {
                    retry_after_ms: value.retry_after_ms,
                })
            }
        };
        Self {
            outcome: Some(outcome),
        }
    }
}

impl TryFrom<proto::ReleaseNodeSlotsRequest> for domain::ReleaseNodeSlotsRequest {
    type Error = PrivateConversionError;

    /// Decodes the reservation, query, and fenced leader identity for release.
    ///
    /// # Errors
    /// Returns [`PrivateConversionError`] when any supplied identifier is not
    /// a valid UUID.
    fn try_from(value: proto::ReleaseNodeSlotsRequest) -> Result<Self, Self::Error> {
        Ok(Self {
            reservation_id: domain::ReservationId::new(uuid_bytes(
                &value.reservation_id,
                "reservation_id",
            )?),
            query_id: domain::QueryId::new(uuid_bytes(&value.query_id, "query_id")?),
            leader_node_id: domain::NodeId::new(uuid_string(
                &value.leader_node_id,
                "leader_node_id",
            )?),
            leader_fencing_token: value.leader_fencing_token,
        })
    }
}

impl From<domain::ReleaseNodeSlotsRequest> for proto::ReleaseNodeSlotsRequest {
    /// Encodes the complete fenced identity of a reservation release.
    fn from(value: domain::ReleaseNodeSlotsRequest) -> Self {
        Self {
            reservation_id: value.reservation_id.as_uuid().as_bytes().to_vec(),
            query_id: value.query_id.as_uuid().as_bytes().to_vec(),
            leader_node_id: value.leader_node_id.as_uuid().to_string(),
            leader_fencing_token: value.leader_fencing_token,
        }
    }
}

impl TryFrom<proto::SignedPeerTicket> for domain::SignedPeerTicket {
    type Error = PrivateConversionError;

    /// Decodes a signed peer ticket under fixed key, claim, and signature bounds.
    ///
    /// # Errors
    /// Returns [`PrivateConversionError`] when the key identifier, claims, or
    /// signature violates the private protocol's size or encoding rules.
    fn try_from(value: proto::SignedPeerTicket) -> Result<Self, Self::Error> {
        if value.key_id.is_empty()
            || value.key_id.len() > MAX_KEY_ID_BYTES
            || !value.key_id.is_ascii()
        {
            return Err(PrivateConversionError::Invalid { field: "key_id" });
        }
        bounded(&value.claims_bytes, MAX_CLAIMS_BYTES, "claims_bytes")?;
        if value.signature.len() != SIGNATURE_BYTES {
            return Err(PrivateConversionError::Invalid { field: "signature" });
        }
        Ok(Self {
            key_id: value.key_id,
            claims_bytes: value.claims_bytes,
            signature: value.signature,
        })
    }
}

impl From<domain::SignedPeerTicket> for proto::SignedPeerTicket {
    /// Encodes an already validated opaque signed peer ticket.
    fn from(value: domain::SignedPeerTicket) -> Self {
        Self {
            key_id: value.key_id,
            claims_bytes: value.claims_bytes,
            signature: value.signature,
        }
    }
}

impl TryFrom<proto::ScanLiteral> for assignment_authority::ScanLiteral {
    type Error = PrivateConversionError;

    /// Decodes one closed scalar literal from its protobuf oneof.
    ///
    /// # Errors
    /// Returns [`PrivateConversionError::Missing`] when the oneof carries no
    /// variant.
    fn try_from(value: proto::ScanLiteral) -> Result<Self, Self::Error> {
        use crate::wyrd::v1::scan_literal::Value;
        match value
            .value
            .ok_or(PrivateConversionError::Missing("scan_literal.value"))?
        {
            Value::BoolValue(inner) => Ok(Self::Bool(inner)),
            Value::I64Value(inner) => Ok(Self::I64(inner)),
            Value::U64Value(inner) => Ok(Self::U64(inner)),
            Value::F64BitsValue(inner) => Ok(Self::F64Bits(inner)),
            Value::Utf8Value(inner) => Ok(Self::Utf8(inner)),
            Value::TimestampMicrosValue(inner) => Ok(Self::TimestampMicros(inner)),
        }
    }
}

impl From<assignment_authority::ScanLiteral> for proto::ScanLiteral {
    /// Encodes one closed scalar literal into its protobuf oneof.
    fn from(value: assignment_authority::ScanLiteral) -> Self {
        use crate::wyrd::v1::scan_literal::Value;
        let value = match value {
            assignment_authority::ScanLiteral::Bool(inner) => Value::BoolValue(inner),
            assignment_authority::ScanLiteral::I64(inner) => Value::I64Value(inner),
            assignment_authority::ScanLiteral::U64(inner) => Value::U64Value(inner),
            assignment_authority::ScanLiteral::F64Bits(inner) => Value::F64BitsValue(inner),
            assignment_authority::ScanLiteral::Utf8(inner) => Value::Utf8Value(inner),
            assignment_authority::ScanLiteral::TimestampMicros(inner) => {
                Value::TimestampMicrosValue(inner)
            }
        };
        Self { value: Some(value) }
    }
}

impl TryFrom<proto::ScanPredicate> for assignment_authority::ScanPredicate {
    type Error = PrivateConversionError;

    /// Decodes one closed leaf predicate, validating the op/literal-presence
    /// shape the wire enum requires (comparisons carry a literal, null
    /// checks do not).
    ///
    /// # Errors
    /// Returns [`PrivateConversionError`] when the operator is unspecified or
    /// unknown, the column is empty, or the literal is present/absent in
    /// violation of the operator's closed shape.
    fn try_from(value: proto::ScanPredicate) -> Result<Self, Self::Error> {
        nonempty(&value.column, "scan_predicate.column")?;
        let op = proto::ScanPredicateOp::try_from(value.op)
            .map_err(|_| PrivateConversionError::RequiredEnum("scan_predicate.op"))?;
        let literal = value.literal;
        match op {
            proto::ScanPredicateOp::Unspecified => {
                Err(PrivateConversionError::RequiredEnum("scan_predicate.op"))
            }
            proto::ScanPredicateOp::IsNull => {
                if literal.is_some() {
                    return Err(PrivateConversionError::Invalid {
                        field: "scan_predicate.literal",
                    });
                }
                Ok(Self::IsNull(value.column))
            }
            proto::ScanPredicateOp::IsNotNull => {
                if literal.is_some() {
                    return Err(PrivateConversionError::Invalid {
                        field: "scan_predicate.literal",
                    });
                }
                Ok(Self::IsNotNull(value.column))
            }
            comparison => {
                let literal: assignment_authority::ScanLiteral = literal
                    .ok_or(PrivateConversionError::Missing("scan_predicate.literal"))?
                    .try_into()?;
                Ok(match comparison {
                    proto::ScanPredicateOp::Eq => Self::Eq(value.column, literal),
                    proto::ScanPredicateOp::NotEq => Self::NotEq(value.column, literal),
                    proto::ScanPredicateOp::Lt => Self::Lt(value.column, literal),
                    proto::ScanPredicateOp::LtEq => Self::LtEq(value.column, literal),
                    proto::ScanPredicateOp::Gt => Self::Gt(value.column, literal),
                    proto::ScanPredicateOp::GtEq => Self::GtEq(value.column, literal),
                    proto::ScanPredicateOp::Unspecified
                    | proto::ScanPredicateOp::IsNull
                    | proto::ScanPredicateOp::IsNotNull => unreachable!(
                        "comparison arm excludes IsNull/IsNotNull/Unspecified by construction"
                    ),
                })
            }
        }
    }
}

impl From<assignment_authority::ScanPredicate> for proto::ScanPredicate {
    /// Encodes one closed leaf predicate into its protobuf op/column/literal
    /// shape.
    fn from(value: assignment_authority::ScanPredicate) -> Self {
        let op = match &value {
            assignment_authority::ScanPredicate::Eq(..) => proto::ScanPredicateOp::Eq,
            assignment_authority::ScanPredicate::NotEq(..) => proto::ScanPredicateOp::NotEq,
            assignment_authority::ScanPredicate::Lt(..) => proto::ScanPredicateOp::Lt,
            assignment_authority::ScanPredicate::LtEq(..) => proto::ScanPredicateOp::LtEq,
            assignment_authority::ScanPredicate::Gt(..) => proto::ScanPredicateOp::Gt,
            assignment_authority::ScanPredicate::GtEq(..) => proto::ScanPredicateOp::GtEq,
            assignment_authority::ScanPredicate::IsNull(_) => proto::ScanPredicateOp::IsNull,
            assignment_authority::ScanPredicate::IsNotNull(_) => proto::ScanPredicateOp::IsNotNull,
        };
        let column = value.column().to_string();
        let literal = value.literal().cloned().map(Into::into);
        Self {
            op: op as i32,
            column,
            literal,
        }
    }
}

impl TryFrom<proto::FollowerScanAssignment> for domain::FollowerScanAssignment {
    type Error = PrivateConversionError;

    /// Decodes one role-local scan assignment and preserves wrapper presence.
    ///
    /// This is a hard v2-only decode: `required_columns` is required and
    /// non-empty on every wire assignment, always including the hidden
    /// tenant column. There is no v1 fallback and no default-empty
    /// projection — a peer advertising v1 semantics by omitting this field
    /// is rejected outright rather than silently admitted with an
    /// unprojected (tenant-dropping) closure.
    ///
    /// # Errors
    /// Returns [`PrivateConversionError`] when identity, binding, persisted
    /// assignment, schema fingerprint, or `required_columns` is absent, or a
    /// closed predicate is malformed.
    fn try_from(value: proto::FollowerScanAssignment) -> Result<Self, Self::Error> {
        nonempty(&value.scan_id, "scan_id")?;
        nonempty(&value.schema_fingerprint, "schema_fingerprint")?;
        nonempty_vec(&value.required_columns, "required_columns")?;
        let persisted = value
            .persisted
            .ok_or(PrivateConversionError::Missing("persisted"))?;
        let predicates = value
            .predicates
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            scan_id: value.scan_id,
            binding: value
                .binding
                .ok_or(PrivateConversionError::Missing("binding"))?
                .try_into()?,
            persisted: domain::PersistedFileAssignment {
                files: persisted.files,
            },
            scribe_provider_cut: value
                .scribe_provider_cut
                .map(TryInto::try_into)
                .transpose()?,
            schema_fingerprint: value.schema_fingerprint,
            required_columns: value.required_columns,
            predicates,
        })
    }
}

impl From<domain::FollowerScanAssignment> for proto::FollowerScanAssignment {
    /// Encodes one validated role-local scan assignment.
    fn from(value: domain::FollowerScanAssignment) -> Self {
        Self {
            scan_id: value.scan_id,
            binding: Some(value.binding.into()),
            persisted: Some(proto::PersistedFileAssignment {
                files: value.persisted.files,
            }),
            scribe_provider_cut: value.scribe_provider_cut.map(Into::into),
            schema_fingerprint: value.schema_fingerprint,
            required_columns: value.required_columns,
            predicates: value.predicates.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<proto::ScribeProviderCut> for domain::ScribeProviderCut {
    type Error = PrivateConversionError;

    /// Decodes the bounded Scribe provider projection without erasing endpoint presence.
    ///
    /// # Errors
    /// Returns a conversion error when either WAL endpoint is absent or the
    /// complete cut violates its canonical ordering, cursor, or signed bounds.
    fn try_from(value: proto::ScribeProviderCut) -> Result<Self, Self::Error> {
        let cut = Self {
            writer_epoch: value.writer_epoch,
            start_partition: time_partition(value.start_partition, "start_partition")?,
            end_partition: time_partition(value.end_partition, "end_partition")?,
            required_columns: value.required_columns,
            persisted_cursor: value.persisted_cursor,
            persisted_ranges: value
                .persisted_ranges
                .into_iter()
                .map(|range| {
                    Ok(domain::PersistedWalRange {
                        start_lsn: range
                            .start_lsn
                            .ok_or(PrivateConversionError::Missing("start_lsn"))?,
                        end_lsn: range
                            .end_lsn
                            .ok_or(PrivateConversionError::Missing("end_lsn"))?,
                    })
                })
                .collect::<Result<_, PrivateConversionError>>()?,
            maximum_batch_count: value.maximum_batch_count,
            maximum_retained_bytes: value.maximum_retained_bytes,
        };
        if !cut.is_valid() {
            return Err(PrivateConversionError::Invalid {
                field: "scribe_provider_cut",
            });
        }
        Ok(cut)
    }
}

impl From<domain::ScribeProviderCut> for proto::ScribeProviderCut {
    /// Encodes the bounded Scribe provider projection.
    fn from(value: domain::ScribeProviderCut) -> Self {
        Self {
            writer_epoch: value.writer_epoch,
            start_partition: Some(time_partition_proto(value.start_partition)),
            end_partition: Some(time_partition_proto(value.end_partition)),
            required_columns: value.required_columns,
            persisted_cursor: value.persisted_cursor,
            persisted_ranges: value
                .persisted_ranges
                .into_iter()
                .map(|range| proto::PersistedWalRange {
                    start_lsn: Some(range.start_lsn),
                    end_lsn: Some(range.end_lsn),
                })
                .collect(),
            maximum_batch_count: value.maximum_batch_count,
            maximum_retained_bytes: value.maximum_retained_bytes,
        }
    }
}

impl TryFrom<proto::ExecuteFragmentRequest> for domain::ExecuteFragmentRequest {
    type Error = PrivateConversionError;

    /// Decodes one authenticated worker-fragment execution request.
    ///
    /// # Errors
    /// Returns [`PrivateConversionError`] for a missing or invalid ticket, an
    /// empty fragment payload, or a malformed reservation identifier.
    fn try_from(value: proto::ExecuteFragmentRequest) -> Result<Self, Self::Error> {
        if value.physical_plan_bytes.is_empty() {
            return Err(PrivateConversionError::Invalid {
                field: "physical_plan_bytes",
            });
        }
        nonempty(&value.plan_fingerprint, "plan_fingerprint")?;
        Ok(Self {
            ticket: value
                .ticket
                .ok_or(PrivateConversionError::Missing("ticket"))?
                .try_into()?,
            physical_plan_bytes: value.physical_plan_bytes,
            reservation_id: domain::ReservationId::new(uuid_bytes(
                &value.reservation_id,
                "reservation_id",
            )?),
            leader_fence: value
                .leader_fence
                .ok_or(PrivateConversionError::Missing("leader_fence"))?
                .try_into()?,
            target_fence: value
                .target_fence
                .ok_or(PrivateConversionError::Missing("target_fence"))?
                .try_into()?,
            assignments: value
                .assignments
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            plan_fingerprint: value.plan_fingerprint,
        })
    }
}

impl From<domain::ExecuteFragmentRequest> for proto::ExecuteFragmentRequest {
    /// Encodes an authenticated worker-fragment execution request.
    fn from(value: domain::ExecuteFragmentRequest) -> Self {
        Self {
            ticket: Some(value.ticket.into()),
            physical_plan_bytes: value.physical_plan_bytes,
            reservation_id: value.reservation_id.as_uuid().as_bytes().to_vec(),
            leader_fence: Some(value.leader_fence.into()),
            target_fence: Some(value.target_fence.into()),
            assignments: value.assignments.into_iter().map(Into::into).collect(),
            plan_fingerprint: value.plan_fingerprint,
        }
    }
}

impl TryFrom<proto::WorkerAttemptFrame> for domain::WorkerAttemptFrame {
    type Error = PrivateConversionError;

    /// Decodes one worker stream frame and validates terminal footer integrity.
    ///
    /// # Errors
    /// Returns [`PrivateConversionError`] for a missing frame or for an
    /// incomplete or malformed worker footer.
    fn try_from(value: proto::WorkerAttemptFrame) -> Result<Self, Self::Error> {
        match value
            .frame
            .ok_or(PrivateConversionError::Missing("frame"))?
        {
            proto::worker_attempt_frame::Frame::ArrowIpcSchema(value) => Ok(Self::Schema(value)),
            proto::worker_attempt_frame::Frame::ArrowIpcBatch(value) => Ok(Self::Batch(value)),
            proto::worker_attempt_frame::Frame::Footer(value) => {
                nonempty(&value.fragment_id, "fragment_id")?;
                if !value.completed {
                    return Err(PrivateConversionError::Invalid { field: "completed" });
                }
                Ok(Self::Footer(domain::WorkerFooter {
                    fragment_id: value.fragment_id,
                    manifest_digest: domain::QueryAuditDigest::new(value.manifest_digest).map_err(
                        |_| PrivateConversionError::Invalid {
                            field: "manifest_digest",
                        },
                    )?,
                    row_count: value.row_count,
                    encoded_bytes: value.encoded_bytes,
                    payload_digest: domain::QueryAuditDigest::new(value.payload_digest).map_err(
                        |_| PrivateConversionError::Invalid {
                            field: "payload_digest",
                        },
                    )?,
                    completed: value.completed,
                    // An absent message means a follower that reported no scan
                    // evidence at all, which is the same as an all-empty one.
                    scan_stats: value.scan_stats.map(Into::into).unwrap_or_default(),
                }))
            }
        }
    }
}

impl From<proto::WorkerScanStats> for domain::WorkerScanStats {
    /// Decodes follower scan evidence, preserving absent-versus-zero bytes.
    fn from(value: proto::WorkerScanStats) -> Self {
        Self {
            bytes_scanned: value.bytes_scanned,
            files_scanned: value.files_scanned,
            partitions_scanned: value.partitions_scanned,
            row_groups_scanned: value.row_groups_scanned,
            row_groups_pruned: value.row_groups_pruned,
        }
    }
}

impl From<domain::WorkerScanStats> for proto::WorkerScanStats {
    /// Encodes follower scan evidence, preserving absent-versus-zero bytes.
    fn from(value: domain::WorkerScanStats) -> Self {
        Self {
            bytes_scanned: value.bytes_scanned,
            files_scanned: value.files_scanned,
            partitions_scanned: value.partitions_scanned,
            row_groups_scanned: value.row_groups_scanned,
            row_groups_pruned: value.row_groups_pruned,
        }
    }
}

impl From<domain::WorkerAttemptFrame> for proto::WorkerAttemptFrame {
    /// Encodes one validated worker stream frame into its protobuf oneof.
    fn from(value: domain::WorkerAttemptFrame) -> Self {
        use proto::worker_attempt_frame::Frame;
        let frame = match value {
            domain::WorkerAttemptFrame::Schema(value) => Frame::ArrowIpcSchema(value),
            domain::WorkerAttemptFrame::Batch(value) => Frame::ArrowIpcBatch(value),
            domain::WorkerAttemptFrame::Footer(value) => Frame::Footer(proto::WorkerFooter {
                fragment_id: value.fragment_id,
                manifest_digest: value.manifest_digest.into(),
                row_count: value.row_count,
                encoded_bytes: value.encoded_bytes,
                payload_digest: value.payload_digest.into(),
                completed: value.completed,
                scan_stats: Some(value.scan_stats.into()),
            }),
        };
        Self { frame: Some(frame) }
    }
}

/// Decodes one required closed admission class.
///
/// # Errors
/// Returns [`PrivateConversionError::RequiredEnum`] for zero or unknown values.
fn query_class(value: i32) -> Result<domain::QueryClass, PrivateConversionError> {
    match proto::QueryClass::try_from(value)
        .map_err(|_| PrivateConversionError::RequiredEnum("query_class"))?
    {
        proto::QueryClass::Interactive => Ok(domain::QueryClass::Interactive),
        proto::QueryClass::Analytical => Ok(domain::QueryClass::Analytical),
        proto::QueryClass::Unspecified => Err(PrivateConversionError::RequiredEnum("query_class")),
    }
}

/// Decodes a private runtime role without accepting protobuf's zero value.
///
/// # Errors
/// Returns [`PrivateConversionError::RequiredEnum`] for unknown or unspecified roles.
fn cluster_role(value: i32) -> Result<domain::ClusterRole, PrivateConversionError> {
    match proto::ClusterRole::try_from(value)
        .map_err(|_| PrivateConversionError::RequiredEnum("role"))?
    {
        proto::ClusterRole::Scribe => Ok(domain::ClusterRole::Scribe),
        proto::ClusterRole::Oracle => Ok(domain::ClusterRole::Oracle),
        proto::ClusterRole::Unspecified => Err(PrivateConversionError::RequiredEnum("role")),
    }
}

/// Decodes the shared authenticated owner-local lifecycle lookup fields.
///
/// # Errors
/// Returns a conversion error for malformed tenant or request identities.
fn decode_lifecycle_lookup(
    tenant_id: &str,
    request_id: &str,
) -> Result<domain::OracleLifecycleLookupRequest, PrivateConversionError> {
    Ok(domain::OracleLifecycleLookupRequest {
        tenant_id: DataTenantId::from_str(tenant_id)
            .map_err(|_| PrivateConversionError::InvalidUuid("tenant_id"))?,
        request_id: RequestId::parse(request_id).map_err(|_| PrivateConversionError::Invalid {
            field: "request_id",
        })?,
    })
}

/// Decodes an exact 16-byte UUID field.
///
/// # Errors
/// Returns [`PrivateConversionError::InvalidUuid`] for malformed bytes.
fn uuid_bytes(value: &[u8], field: &'static str) -> Result<uuid::Uuid, PrivateConversionError> {
    uuid::Uuid::from_slice(value).map_err(|_| PrivateConversionError::InvalidUuid(field))
}

/// Decodes a canonical UUID string field.
///
/// # Errors
/// Returns [`PrivateConversionError::InvalidUuid`] for malformed text.
fn uuid_string(value: &str, field: &'static str) -> Result<uuid::Uuid, PrivateConversionError> {
    uuid::Uuid::parse_str(value).map_err(|_| PrivateConversionError::InvalidUuid(field))
}

/// Converts non-negative Unix milliseconds into a UTC timestamp.
///
/// # Errors
/// Returns [`PrivateConversionError::Invalid`] when milliseconds overflow or
/// cannot be represented by `chrono`.
fn datetime(
    value: u64,
    field: &'static str,
) -> Result<chrono::DateTime<chrono::Utc>, PrivateConversionError> {
    let millis = i64::try_from(value).map_err(|_| PrivateConversionError::Invalid { field })?;
    chrono::DateTime::from_timestamp_millis(millis).ok_or(PrivateConversionError::Invalid { field })
}

/// Converts an invariant-valid post-epoch timestamp into wire milliseconds.
///
/// # Panics
/// Panics when a domain timestamp predates the Unix epoch.
fn unix_millis(value: chrono::DateTime<chrono::Utc>) -> u64 {
    u64::try_from(value.timestamp_millis())
        .expect("private contract timestamps are at or after the Unix epoch")
}

/// Validates one required non-whitespace string.
///
/// # Errors
/// Returns [`PrivateConversionError::Invalid`] when empty after trimming.
fn nonempty(value: &str, field: &'static str) -> Result<(), PrivateConversionError> {
    if value.trim().is_empty() {
        Err(PrivateConversionError::Invalid { field })
    } else {
        Ok(())
    }
}

/// Validates one required, non-empty repeated field.
///
/// `required_columns` is a hard v2 requirement on every
/// [`proto::FollowerScanAssignment`]: it always carries the hidden tenant
/// column alongside the requested projection, so an empty list can only mean
/// a v1 peer or a malformed wire value, never a legitimate "no projection"
/// assignment. Rejecting it here — before any provider or object I/O sees
/// the assignment — is what keeps an omitted or defaulted field from
/// silently becoming a tenant-isolation hole instead of a decode failure.
///
/// # Errors
/// Returns [`PrivateConversionError::Missing`] when `values` is empty.
fn nonempty_vec<T>(values: &[T], field: &'static str) -> Result<(), PrivateConversionError> {
    if values.is_empty() {
        Err(PrivateConversionError::Missing(field))
    } else {
        Ok(())
    }
}

/// Validates one numeric protocol field is nonzero.
///
/// # Errors
/// Returns [`PrivateConversionError::Invalid`] for the type's zero value.
fn positive<T>(value: T, field: &'static str) -> Result<(), PrivateConversionError>
where
    T: Default + PartialEq,
{
    if value == T::default() {
        Err(PrivateConversionError::Invalid { field })
    } else {
        Ok(())
    }
}

/// Requires the exact private protocol version supported in v1.
///
/// # Errors
/// Returns [`PrivateConversionError::Invalid`] for any version other than one.
fn protocol_v1(value: u16, field: &'static str) -> Result<(), PrivateConversionError> {
    if value == 1 {
        Ok(())
    } else {
        Err(PrivateConversionError::Invalid { field })
    }
}

/// Enforces one hard byte-slice protocol ceiling.
///
/// # Errors
/// Returns [`PrivateConversionError::TooLarge`] above `maximum`.
fn bounded(
    value: &[u8],
    maximum: usize,
    field: &'static str,
) -> Result<(), PrivateConversionError> {
    if value.len() > maximum {
        Err(PrivateConversionError::TooLarge { field })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Required private enum zero values fail closed.
    #[test]
    fn reserve_slots_rejects_unspecified_query_class() {
        let request = proto::ReserveNodeSlotsRequest {
            query_id: uuid::Uuid::now_v7().as_bytes().to_vec(),
            leader_node_id: uuid::Uuid::now_v7().to_string(),
            leader_fencing_token: 1,
            query_class: 0,
            slot_units: 1,
            expires_at_unix_ms: 1,
        };
        assert!(matches!(
            domain::ReserveNodeSlotsRequest::try_from(request),
            Err(PrivateConversionError::RequiredEnum("query_class"))
        ));
    }

    /// Every private UUID byte field requires exactly 16 bytes.
    #[test]
    fn release_slots_rejects_malformed_uuid_bytes() {
        let request = proto::ReleaseNodeSlotsRequest {
            reservation_id: vec![0; 15],
            query_id: uuid::Uuid::now_v7().as_bytes().to_vec(),
            leader_node_id: uuid::Uuid::now_v7().to_string(),
            leader_fencing_token: 1,
        };
        assert!(matches!(
            domain::ReleaseNodeSlotsRequest::try_from(request),
            Err(PrivateConversionError::InvalidUuid("reservation_id"))
        ));
    }

    /// Peer ticket bounds are enforced before opaque claims can be decoded.
    #[test]
    fn peer_ticket_rejects_field_bound_violations() {
        let ticket = proto::SignedPeerTicket {
            key_id: "k".into(),
            claims_bytes: vec![0; MAX_CLAIMS_BYTES + 1],
            signature: vec![0; SIGNATURE_BYTES],
        };
        assert!(matches!(
            domain::SignedPeerTicket::try_from(ticket),
            Err(PrivateConversionError::TooLarge {
                field: "claims_bytes"
            })
        ));
    }

    /// Peer signatures must use the exact v1 Ed25519 byte width.
    #[test]
    fn peer_ticket_rejects_wrong_signature_width() {
        let ticket = proto::SignedPeerTicket {
            key_id: "k".into(),
            claims_bytes: vec![],
            signature: vec![0; SIGNATURE_BYTES - 1],
        };
        assert!(matches!(
            domain::SignedPeerTicket::try_from(ticket),
            Err(PrivateConversionError::Invalid { field: "signature" })
        ));
    }

    /// Peer key identifiers are bounded ASCII.
    #[test]
    fn peer_ticket_rejects_non_ascii_key_id() {
        let ticket = proto::SignedPeerTicket {
            key_id: "é".into(),
            claims_bytes: vec![],
            signature: vec![0; SIGNATURE_BYTES],
        };
        assert!(matches!(
            domain::SignedPeerTicket::try_from(ticket),
            Err(PrivateConversionError::Invalid { field: "key_id" })
        ));
    }

    /// Missing private oneofs fail before reaching runtime owners.
    #[test]
    fn worker_attempt_rejects_missing_frame() {
        assert!(matches!(
            domain::WorkerAttemptFrame::try_from(proto::WorkerAttemptFrame { frame: None }),
            Err(PrivateConversionError::Missing("frame"))
        ));
    }

    /// A valid private reservation request round-trips without losing fences.
    #[test]
    fn reserve_slots_round_trips() {
        let expected = domain::ReserveNodeSlotsRequest {
            query_id: domain::QueryId::new(uuid::Uuid::now_v7()),
            leader_node_id: domain::NodeId::new(uuid::Uuid::now_v7()),
            leader_fencing_token: 42,
            query_class: domain::QueryClass::Analytical,
            slot_units: 3,
            expires_at: chrono::DateTime::from_timestamp_millis(99).expect("valid timestamp"),
        };
        let actual = domain::ReserveNodeSlotsRequest::try_from(
            proto::ReserveNodeSlotsRequest::from(expected.clone()),
        )
        .expect("valid reservation request round-trips");
        assert_eq!(actual, expected);
    }

    /// Builds one hourly partition from its exact epoch-microsecond boundary.
    fn hour_partition(start_unix_micros: i64) -> domain::TimePartitionWire {
        partition(domain::TimeGranularityWire::Hour, start_unix_micros)
    }

    /// Builds one partition from a granularity and an exact boundary.
    fn partition(
        granularity: domain::TimeGranularityWire,
        start_unix_micros: i64,
    ) -> domain::TimePartitionWire {
        let start = chrono::DateTime::from_timestamp_micros(start_unix_micros)
            .expect("fixture start is representable");
        domain::TimePartitionWire::new(granularity, start).expect("fixture start is canonical")
    }

    /// Every partition-bearing private message round-trips its exact typed
    /// partition, and every malformed encoding is rejected before a caller can
    /// act on it: the unspecified enum tag, an unknown tag, a noncanonical
    /// start for each granularity, and an absent nested message.
    #[test]
    fn scribe_cut_v3_contract() {
        for granularity in [
            domain::TimeGranularityWire::Hour,
            domain::TimeGranularityWire::Day,
        ] {
            let start = match granularity {
                domain::TimeGranularityWire::Hour => 1_787_493_600_000_000,
                domain::TimeGranularityWire::Day => 1_787_443_200_000_000,
            };
            let expected = partition(granularity, start);
            let restored = time_partition(Some(time_partition_proto(expected)), "time_partition")
                .expect("valid partition round-trips");
            assert_eq!(restored, expected);
        }

        let cut = domain::ScribeProviderCut {
            writer_epoch: 7,
            start_partition: hour_partition(1_787_493_600_000_000),
            end_partition: hour_partition(1_787_497_200_000_000),
            required_columns: vec!["service_name".to_owned()],
            persisted_cursor: 41,
            persisted_ranges: vec![domain::PersistedWalRange {
                start_lsn: 1,
                end_lsn: 40,
            }],
            maximum_batch_count: 16,
            maximum_retained_bytes: 1_048_576,
        };
        assert_eq!(
            domain::ScribeProviderCut::try_from(proto::ScribeProviderCut::from(cut.clone()))
                .expect("valid cut round-trips"),
            cut
        );

        // The protobuf zero tag is "unspecified" and must never default.
        assert!(matches!(
            time_partition(
                Some(proto::TimePartition {
                    granularity: proto::TimeGranularity::Unspecified as i32,
                    start_unix_micros: 1_787_493_600_000_000,
                }),
                "time_partition"
            ),
            Err(PrivateConversionError::RequiredEnum("time_partition"))
        ));

        // An unknown tag is rejected the same way, not silently mapped.
        assert!(matches!(
            time_partition(
                Some(proto::TimePartition {
                    granularity: 9,
                    start_unix_micros: 1_787_493_600_000_000,
                }),
                "time_partition"
            ),
            Err(PrivateConversionError::RequiredEnum("time_partition"))
        ));

        // Noncanonical starts fail per granularity.
        for (granularity, start) in [
            (proto::TimeGranularity::Hour, 1_787_493_600_000_001),
            (proto::TimeGranularity::Day, 1_787_493_600_000_000),
        ] {
            assert!(matches!(
                time_partition(
                    Some(proto::TimePartition {
                        granularity: granularity as i32,
                        start_unix_micros: start,
                    }),
                    "time_partition"
                ),
                Err(PrivateConversionError::Invalid {
                    field: "time_partition"
                })
            ));
        }

        // An absent nested message is missing, never an implicit epoch value.
        assert!(matches!(
            time_partition(None, "time_partition"),
            Err(PrivateConversionError::Missing("time_partition"))
        ));

        // A cut whose endpoints disagree on granularity is not a valid range.
        let mut mixed = cut;
        mixed.end_partition = partition(domain::TimeGranularityWire::Day, 1_787_443_200_000_000);
        assert!(matches!(
            domain::ScribeProviderCut::try_from(proto::ScribeProviderCut::from(mixed)),
            Err(PrivateConversionError::Invalid {
                field: "scribe_provider_cut"
            })
        ));
    }

    /// A valid private tail cursor round-trips its exact row identity.
    #[test]
    fn tail_cursor_round_trips() {
        let expected = domain::TailCursor {
            writer_epoch: 4,
            wal_lsn: 8,
            batch_id: uuid::Uuid::now_v7(),
            row_ordinal: 12,
        };
        let actual = domain::TailCursor::try_from(proto::TailCursor::from(expected.clone()))
            .expect("valid cursor round-trips");
        assert_eq!(actual, expected);
    }

    /// Tail acquire, fence, page, and release messages preserve exact identities.
    #[test]
    fn private_tail_messages_round_trip() {
        let binding = domain::TenantTableBinding {
            tenant_id: wyrd_spec::DataTenantId::new_v7(),
            namespace: "vala".into(),
            table: "traces".into(),
        };
        let cursor = domain::TailCursor {
            writer_epoch: 4,
            wal_lsn: 8,
            batch_id: uuid::Uuid::now_v7(),
            row_ordinal: 12,
        };
        let acquire = domain::AcquireTailFenceRequest {
            query_id: uuid::Uuid::now_v7(),
            binding: binding.clone(),
            time_partition: hour_partition(1_787_493_600_000_000),
            exclusive_sealed: cursor.clone(),
            deadline: chrono::DateTime::from_timestamp_millis(50).expect("valid timestamp"),
            schema_fingerprint: domain::SchemaFingerprint::new("schema-1")
                .expect("valid schema fingerprint"),
            tail_protocol_version: 1,
        };
        assert_eq!(
            domain::AcquireTailFenceRequest::try_from(proto::AcquireTailFenceRequest::from(
                acquire.clone()
            ))
            .expect("acquire round-trips"),
            acquire
        );

        let fence = domain::TailReadFence {
            fence_id: domain::TailFenceId::new(uuid::Uuid::now_v7()),
            binding,
            time_partition: hour_partition(1_787_493_600_000_000),
            stream: domain::TailStreamIdentity {
                node_id: domain::NodeId::new(uuid::Uuid::now_v7()),
                writer_epoch: 4,
            },
            exclusive_sealed: cursor.clone(),
            inclusive_live: domain::TailCursor {
                wal_lsn: 10,
                ..cursor.clone()
            },
            schema_fingerprint: domain::SchemaFingerprint::new("schema-1")
                .expect("valid schema fingerprint"),
            tail_protocol_version: 1,
            expires_at: chrono::DateTime::from_timestamp_millis(100).expect("valid timestamp"),
        };
        assert_eq!(
            domain::TailReadFence::try_from(proto::TailReadFence::from(fence.clone()))
                .expect("fence round-trips"),
            fence
        );

        let page_request = domain::TailPageRequest {
            query_id: uuid::Uuid::nil(),
            fence_id: fence.fence_id,
            after: Some(cursor.clone()),
            max_rows: 10,
            max_encoded_bytes: 1024,
        };
        assert_eq!(
            domain::TailPageRequest::try_from(proto::TailPageRequest::from(page_request.clone()))
                .expect("page request round-trips"),
            page_request
        );
        let page = domain::TailPage {
            batches: vec![vec![1, 2, 3]],
            next: Some(cursor),
            complete: false,
        };
        assert_eq!(
            domain::TailPage::try_from(proto::TailPage::from(page.clone()))
                .expect("page round-trips"),
            page
        );
        let release = domain::ReleaseTailFenceRequest {
            query_id: uuid::Uuid::nil(),
            fence_id: fence.fence_id,
        };
        assert_eq!(
            domain::ReleaseTailFenceRequest::try_from(proto::ReleaseTailFenceRequest::from(
                release.clone()
            ))
            .expect("release round-trips"),
            release
        );
    }

    /// Reservation outcomes, release, and execution requests round-trip.
    #[test]
    fn private_peer_messages_round_trip() {
        let reservation_id = domain::ReservationId::new(uuid::Uuid::now_v7());
        let outcome = domain::ReserveNodeSlotsResponse::Pending(domain::PendingNodeReservation {
            reservation_id,
            expires_at: chrono::DateTime::from_timestamp_millis(100).expect("valid timestamp"),
        });
        assert_eq!(
            domain::ReserveNodeSlotsResponse::try_from(proto::ReserveNodeSlotsResponse::from(
                outcome.clone()
            ))
            .expect("reservation outcome round-trips"),
            outcome
        );
        let release = domain::ReleaseNodeSlotsRequest {
            reservation_id,
            query_id: domain::QueryId::new(uuid::Uuid::now_v7()),
            leader_node_id: domain::NodeId::new(uuid::Uuid::now_v7()),
            leader_fencing_token: 9,
        };
        assert_eq!(
            domain::ReleaseNodeSlotsRequest::try_from(proto::ReleaseNodeSlotsRequest::from(
                release.clone()
            ))
            .expect("reservation release round-trips"),
            release
        );
        let execute = domain::ExecuteFragmentRequest {
            ticket: domain::SignedPeerTicket {
                key_id: "key-1".into(),
                claims_bytes: vec![1, 2],
                signature: vec![3; SIGNATURE_BYTES],
            },
            physical_plan_bytes: vec![4, 5],
            reservation_id,
            leader_fence: domain::OracleRoleFence {
                node_id: domain::NodeId::new(uuid::Uuid::now_v7()),
                role: domain::ClusterRole::Oracle,
                fencing_token: 7,
            },
            target_fence: domain::OracleRoleFence {
                node_id: domain::NodeId::new(uuid::Uuid::now_v7()),
                role: domain::ClusterRole::Scribe,
                fencing_token: 8,
            },
            assignments: Vec::new(),
            plan_fingerprint: "sha256:physical-plan".to_owned(),
        };
        assert_eq!(
            domain::ExecuteFragmentRequest::try_from(proto::ExecuteFragmentRequest::from(
                execute.clone(),
            ))
            .expect("execute request round-trips"),
            execute
        );
    }

    /// The private lifecycle surface contains only owner-local list/get/cancel operations.
    #[test]
    /// # Panics
    /// Panics if an owner-local lifecycle message does not round-trip exactly.
    fn oracle_private_lifecycle_uses_owner_local_list_get_cancel() {
        let tenant_id = wyrd_spec::DataTenantId::new_v7();
        let request_id = RequestId::now_v7();
        let request = domain::CancelOracleLifecycleRequest {
            tenant_id,
            request_id: request_id.clone(),
        };
        let decoded = domain::CancelOracleLifecycleRequest::try_from(
            proto::CancelOracleLifecycleRequest::from(request),
        )
        .expect("owner-local cancellation round-trips");
        assert_eq!(decoded.tenant_id, tenant_id);
        assert_eq!(decoded.request_id, request_id);
        let lookup = domain::OracleLifecycleLookupRequest {
            tenant_id,
            request_id: request_id.clone(),
        };
        let listing = domain::ListOracleLifecyclesRequest { tenant_id };
        let listed = domain::ListOracleLifecyclesRequest::try_from(
            proto::ListOracleLifecyclesRequest::from(listing.clone()),
        )
        .expect("owner-local tenant list round-trips");
        let fetched = domain::OracleLifecycleLookupRequest::try_from(
            proto::GetOracleLifecycleRequest::from(lookup.clone()),
        )
        .expect("owner-local get lookup round-trips");
        assert_eq!(listed, listing);
        assert_eq!(fetched, lookup);

        let summary = domain::RunningQuerySummary {
            request_id: request_id.clone(),
            query_class: domain::QueryClass::Interactive,
            started_at: chrono::DateTime::from_timestamp(1, 0).expect("valid timestamp"),
            deadline: chrono::DateTime::from_timestamp(2, 0).expect("valid timestamp"),
            state: domain::RunningQueryLifecycleState::Running,
            progress: domain::RunningQueryProgress {
                completed_participants: 1,
                total_participants: 2,
            },
            cancellation_requested: false,
        };
        let list = domain::ListOracleLifecyclesResponse {
            queries: vec![summary.clone()],
        };
        assert_eq!(
            domain::ListOracleLifecyclesResponse::try_from(
                proto::ListOracleLifecyclesResponse::from(list.clone())
            )
            .expect("owner-local list response round-trips"),
            list
        );
        let get = domain::GetOracleLifecycleResponse { query: summary };
        assert_eq!(
            domain::GetOracleLifecycleResponse::try_from(proto::GetOracleLifecycleResponse::from(
                get.clone()
            ))
            .expect("owner-local get response round-trips"),
            get
        );
    }

    /// Optional protobuf WAL endpoints distinguish explicit zero from absence.
    #[test]
    /// # Panics
    /// Panics if optional persisted-WAL endpoints lose their wire presence.
    fn oracle_execute_fragment_preserves_optional_wal_endpoint_presence() {
        let present = proto::PersistedWalRange {
            start_lsn: Some(0),
            end_lsn: Some(0),
        };
        assert_eq!(present.start_lsn, Some(0));
        assert_eq!(present.end_lsn, Some(0));
        let missing = proto::PersistedWalRange {
            start_lsn: None,
            end_lsn: Some(0),
        };
        assert!(missing.start_lsn.is_none());
    }

    /// A completed worker footer round-trips as the terminal frame.
    #[test]
    fn worker_footer_round_trips() {
        let expected = domain::WorkerAttemptFrame::Footer(domain::WorkerFooter {
            fragment_id: "fragment-1".into(),
            manifest_digest: domain::QueryAuditDigest::new("sha256:manifest")
                .expect("valid manifest digest"),
            row_count: 2,
            encoded_bytes: 32,
            payload_digest: domain::QueryAuditDigest::new("sha256:payload")
                .expect("valid payload digest"),
            completed: true,
            scan_stats: domain::WorkerScanStats {
                bytes_scanned: Some(4_096),
                files_scanned: 3,
                partitions_scanned: 2,
                row_groups_scanned: 5,
                row_groups_pruned: 7,
            },
        });
        let actual =
            domain::WorkerAttemptFrame::try_from(proto::WorkerAttemptFrame::from(expected.clone()))
                .expect("valid footer round-trips");
        assert_eq!(actual, expected);
    }

    /// Every closed scalar/op round-trips through the v2 wire, and malformed
    /// closed-shape input (unspecified op, literal on a null-check, missing
    /// literal on a comparison) is rejected before it reaches the domain type.
    #[test]
    fn follower_assignment_v2_contract() {
        let predicates = vec![
            assignment_authority::ScanPredicate::Eq(
                "service_name".into(),
                assignment_authority::ScanLiteral::Utf8("api".into()),
            ),
            assignment_authority::ScanPredicate::NotEq(
                "status".into(),
                assignment_authority::ScanLiteral::I64(-1),
            ),
            assignment_authority::ScanPredicate::Lt(
                "duration_ms".into(),
                assignment_authority::ScanLiteral::U64(500),
            ),
            assignment_authority::ScanPredicate::LtEq(
                "score".into(),
                assignment_authority::ScanLiteral::F64Bits(1.5_f64.to_bits()),
            ),
            assignment_authority::ScanPredicate::Gt(
                "wyrd_event_time".into(),
                assignment_authority::ScanLiteral::TimestampMicros(1_000_000),
            ),
            assignment_authority::ScanPredicate::GtEq(
                "active".into(),
                assignment_authority::ScanLiteral::Bool(true),
            ),
            assignment_authority::ScanPredicate::IsNull("optional_field".into()),
            assignment_authority::ScanPredicate::IsNotNull("required_field".into()),
        ];

        let expected = domain::FollowerScanAssignment {
            scan_id: "scan-1".into(),
            binding: domain::TenantTableBinding {
                tenant_id: wyrd_spec::DataTenantId::new_v7(),
                namespace: "logs".into(),
                table: "records".into(),
            },
            persisted: domain::PersistedFileAssignment {
                files: vec!["s3://bucket/logs/a.parquet".into()],
            },
            scribe_provider_cut: None,
            schema_fingerprint: "0".repeat(64),
            required_columns: vec![
                "service_name".into(),
                "wyrd_event_time".into(),
                "data_tenant_id".into(),
            ],
            predicates,
        };

        let actual = domain::FollowerScanAssignment::try_from(proto::FollowerScanAssignment::from(
            expected.clone(),
        ))
        .expect("valid v2 assignment round-trips");
        assert_eq!(actual, expected);

        // Unspecified op is rejected before the literal is inspected.
        let unspecified_op = proto::FollowerScanAssignment::from(expected.clone());
        let mut malformed = unspecified_op.clone();
        malformed.predicates = vec![proto::ScanPredicate {
            op: proto::ScanPredicateOp::Unspecified as i32,
            column: "x".into(),
            literal: None,
        }];
        assert!(matches!(
            domain::FollowerScanAssignment::try_from(malformed),
            Err(PrivateConversionError::RequiredEnum("scan_predicate.op"))
        ));

        // A literal on a null-check violates the closed shape.
        let mut literal_on_null_check = unspecified_op.clone();
        literal_on_null_check.predicates = vec![proto::ScanPredicate {
            op: proto::ScanPredicateOp::IsNull as i32,
            column: "x".into(),
            literal: Some(proto::ScanLiteral {
                value: Some(crate::wyrd::v1::scan_literal::Value::BoolValue(true)),
            }),
        }];
        assert!(matches!(
            domain::FollowerScanAssignment::try_from(literal_on_null_check),
            Err(PrivateConversionError::Invalid {
                field: "scan_predicate.literal"
            })
        ));

        // A missing literal on a comparison is rejected.
        let mut missing_literal = unspecified_op.clone();
        missing_literal.predicates = vec![proto::ScanPredicate {
            op: proto::ScanPredicateOp::Eq as i32,
            column: "x".into(),
            literal: None,
        }];
        assert!(matches!(
            domain::FollowerScanAssignment::try_from(missing_literal),
            Err(PrivateConversionError::Missing("scan_predicate.literal"))
        ));

        // An empty predicate column is rejected regardless of operator.
        let mut empty_column = unspecified_op;
        empty_column.predicates = vec![proto::ScanPredicate {
            op: proto::ScanPredicateOp::IsNull as i32,
            column: String::new(),
            literal: None,
        }];
        assert!(matches!(
            domain::FollowerScanAssignment::try_from(empty_column),
            Err(PrivateConversionError::Invalid {
                field: "scan_predicate.column"
            })
        ));
    }

    /// A v1-shaped peer that omits `required_columns` is rejected outright:
    /// there is no dual decoder and no default-empty projection fallback.
    /// An empty `required_columns` would silently drop the hidden tenant
    /// column, so decoding must fail closed rather than admit the
    /// assignment with an unprojected read.
    #[test]
    fn follower_assignment_v1_shaped_peer_is_rejected() {
        let expected = domain::FollowerScanAssignment {
            scan_id: "scan-1".into(),
            binding: domain::TenantTableBinding {
                tenant_id: wyrd_spec::DataTenantId::new_v7(),
                namespace: "logs".into(),
                table: "records".into(),
            },
            persisted: domain::PersistedFileAssignment {
                files: vec!["s3://bucket/logs/a.parquet".into()],
            },
            scribe_provider_cut: None,
            schema_fingerprint: "0".repeat(64),
            required_columns: vec!["data_tenant_id".into()],
            predicates: Vec::new(),
        };
        let mut v1_shaped = proto::FollowerScanAssignment::from(expected);
        v1_shaped.required_columns = Vec::new();
        assert!(matches!(
            domain::FollowerScanAssignment::try_from(v1_shaped),
            Err(PrivateConversionError::Missing("required_columns"))
        ));
    }
}
