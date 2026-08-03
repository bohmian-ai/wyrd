//! Validated protobuf conversions for private Scribe and Oracle peer contracts.

use std::str::FromStr;

use wyrd_spec::vala::api as domain;

use crate::wyrd::v1 as proto;

/// Hard protocol ceiling for opaque signed claims.
const MAX_CLAIMS_BYTES: usize = 16 * 1024;
/// Exact v1 peer signature width.
const SIGNATURE_BYTES: usize = 64;
/// Hard protocol ceiling for an ASCII signing-key identifier.
const MAX_KEY_ID_BYTES: usize = 64;

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
            event_day: domain::EventDay::new(value.event_day)
                .map_err(|_| PrivateConversionError::Invalid { field: "event_day" })?,
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
            event_day: value.event_day.as_str().to_owned(),
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
            event_day: domain::EventDay::new(value.event_day)
                .map_err(|_| PrivateConversionError::Invalid { field: "event_day" })?,
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
            event_day: value.event_day.as_str().to_owned(),
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

impl TryFrom<proto::ExecuteFragmentRequest> for domain::ExecuteFragmentRequest {
    type Error = PrivateConversionError;

    /// Decodes one authenticated worker-fragment execution request.
    ///
    /// # Errors
    /// Returns [`PrivateConversionError`] for a missing or invalid ticket, an
    /// empty fragment payload, or a malformed reservation identifier.
    fn try_from(value: proto::ExecuteFragmentRequest) -> Result<Self, Self::Error> {
        if value.fragment_bytes.is_empty() {
            return Err(PrivateConversionError::Invalid {
                field: "fragment_bytes",
            });
        }
        Ok(Self {
            ticket: value
                .ticket
                .ok_or(PrivateConversionError::Missing("ticket"))?
                .try_into()?,
            fragment_bytes: value.fragment_bytes,
            reservation_id: domain::ReservationId::new(uuid_bytes(
                &value.reservation_id,
                "reservation_id",
            )?),
        })
    }
}

impl From<domain::ExecuteFragmentRequest> for proto::ExecuteFragmentRequest {
    /// Encodes an authenticated worker-fragment execution request.
    fn from(value: domain::ExecuteFragmentRequest) -> Self {
        Self {
            ticket: Some(value.ticket.into()),
            fragment_bytes: value.fragment_bytes,
            reservation_id: value.reservation_id.as_uuid().as_bytes().to_vec(),
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
                }))
            }
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
            event_day: domain::EventDay::new("2026-07-30").expect("valid event day"),
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
            event_day: domain::EventDay::new("2026-07-30").expect("valid event day"),
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
            fragment_bytes: vec![4, 5],
            reservation_id,
        };
        assert_eq!(
            domain::ExecuteFragmentRequest::try_from(proto::ExecuteFragmentRequest::from(
                execute.clone()
            ))
            .expect("execute request round-trips"),
            execute
        );
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
        });
        let actual =
            domain::WorkerAttemptFrame::try_from(proto::WorkerAttemptFrame::from(expected.clone()))
                .expect("valid footer round-trips");
        assert_eq!(actual, expected);
    }
}
