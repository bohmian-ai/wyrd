//! Validated protobuf conversions for private Scribe and Oracle peer contracts.

use std::str::FromStr;

use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api as domain;
use wyrd_spec::vala::assignment_authority;
use wyrd_spec::DataTenantId;

use crate::wyrd::v1 as proto;

/// Hard protocol ceiling for typed peer-context claims, enforced before decode.
const MAX_CLAIMS_BYTES: usize = 16 * 1024;

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
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
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
    /// unknown class, a missing graph, or an invalid expiry timestamp.
    fn try_from(value: proto::ReserveNodeSlotsRequest) -> Result<Self, Self::Error> {
        Ok(Self {
            query_id: domain::QueryId::new(uuid_bytes(&value.query_id, "query_id")?),
            leader_node_id: domain::NodeId::new(uuid_string(
                &value.leader_node_id,
                "leader_node_id",
            )?),
            leader_fencing_token: value.leader_fencing_token,
            expires_at: datetime(value.expires_at_unix_ms, "expires_at_unix_ms")?,
            graph: analytical_graph_ref(
                value
                    .graph
                    .ok_or(PrivateConversionError::Missing("graph"))?,
            )?,
        })
    }
}

/// Decodes the two-identity name of one distributed Analytical graph.
///
/// # Errors
///
/// Returns [`PrivateConversionError`] when either identity is not a UUID, so a
/// reservation can never be bound to a graph name the leader did not encode.
fn analytical_graph_ref(
    value: proto::AnalyticalGraphRef,
) -> Result<domain::AnalyticalGraphRef, PrivateConversionError> {
    Ok(domain::AnalyticalGraphRef {
        public_query_id: uuid_bytes(&value.public_query_id, "public_query_id")?,
        datafusion_query_id: uuid_bytes(&value.datafusion_query_id, "datafusion_query_id")?,
    })
}

impl From<domain::ReserveNodeSlotsRequest> for proto::ReserveNodeSlotsRequest {
    /// Encodes a validated Oracle capacity-reservation request.
    ///
    /// The typed context is deliberately absent here: it binds the digest of
    /// this encoding, so the leader's transport stamps it after conversion.
    fn from(value: domain::ReserveNodeSlotsRequest) -> Self {
        Self {
            context: None,
            query_id: value.query_id.as_uuid().as_bytes().to_vec(),
            leader_node_id: value.leader_node_id.as_uuid().to_string(),
            leader_fencing_token: value.leader_fencing_token,
            expires_at_unix_ms: unix_millis(value.expires_at),
            graph: Some(proto::AnalyticalGraphRef {
                public_query_id: value.graph.public_query_id.as_bytes().to_vec(),
                datafusion_query_id: value.graph.datafusion_query_id.as_bytes().to_vec(),
            }),
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

impl TryFrom<proto::PeerContext> for domain::PeerContext {
    type Error = PrivateConversionError;

    /// Decodes a typed peer context under the fixed claims-byte bound.
    ///
    /// # Errors
    /// Returns [`PrivateConversionError::TooLarge`] when the claims exceed the
    /// private protocol's bound; nothing is decoded before this check.
    fn try_from(value: proto::PeerContext) -> Result<Self, Self::Error> {
        bounded(&value.claims_bytes, MAX_CLAIMS_BYTES, "claims_bytes")?;
        Ok(Self {
            claims_bytes: value.claims_bytes,
        })
    }
}

impl From<domain::PeerContext> for proto::PeerContext {
    /// Encodes an already bounded typed peer context.
    fn from(value: domain::PeerContext) -> Self {
        Self {
            claims_bytes: value.claims_bytes,
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
            Value::BytesValue(inner) => Ok(Self::Bytes(inner)),
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
            assignment_authority::ScanLiteral::Bytes(inner) => Value::BytesValue(inner),
        };
        Self { value: Some(value) }
    }
}

impl TryFrom<proto::ScanLeafRef> for assignment_authority::ScanLeaf {
    type Error = PrivateConversionError;

    /// Decodes one predicate leaf from its protobuf oneof.
    ///
    /// Column names must be non-blank. A Struct or Variant path must hold at
    /// least one element, and every element must be non-empty; elements are
    /// otherwise whole UTF-8 names, so a whitespace key stays a valid key.
    ///
    /// # Errors
    /// Returns [`PrivateConversionError::Missing`] when the oneof is empty and
    /// [`PrivateConversionError::Invalid`] for a blank column, an empty path,
    /// or an empty path element.
    fn try_from(value: proto::ScanLeafRef) -> Result<Self, Self::Error> {
        use crate::wyrd::v1::scan_leaf_ref::Kind;
        /// Validates one non-empty path of non-empty elements.
        fn path(
            elements: Vec<String>,
            field: &'static str,
        ) -> Result<Vec<String>, PrivateConversionError> {
            if elements.is_empty() || elements.iter().any(String::is_empty) {
                return Err(PrivateConversionError::Invalid { field });
            }
            Ok(elements)
        }
        match value
            .kind
            .ok_or(PrivateConversionError::Missing("scan_predicate.leaf.kind"))?
        {
            Kind::Column(leaf) => {
                nonempty(&leaf.column, "scan_predicate.leaf.column")?;
                Ok(Self::Column(leaf.column))
            }
            Kind::StructField(leaf) => {
                nonempty(&leaf.column, "scan_predicate.leaf.struct_field.column")?;
                Ok(Self::StructField {
                    column: leaf.column,
                    fields: path(leaf.fields, "scan_predicate.leaf.struct_field.fields")?,
                })
            }
            Kind::Variant(leaf) => {
                nonempty(&leaf.column, "scan_predicate.leaf.variant.column")?;
                Ok(Self::Variant {
                    column: leaf.column,
                    keys: path(leaf.keys, "scan_predicate.leaf.variant.keys")?,
                })
            }
        }
    }
}

impl From<assignment_authority::ScanLeaf> for proto::ScanLeafRef {
    /// Encodes one predicate leaf into its protobuf oneof.
    fn from(value: assignment_authority::ScanLeaf) -> Self {
        use crate::wyrd::v1::scan_leaf_ref::Kind;
        let kind = match value {
            assignment_authority::ScanLeaf::Column(column) => {
                Kind::Column(proto::ScanColumnRef { column })
            }
            assignment_authority::ScanLeaf::StructField { column, fields } => {
                Kind::StructField(proto::ScanStructRef { column, fields })
            }
            assignment_authority::ScanLeaf::Variant { column, keys } => {
                Kind::Variant(proto::ScanVariantRef { column, keys })
            }
        };
        Self { kind: Some(kind) }
    }
}

impl TryFrom<proto::ScanPredicate> for assignment_authority::ScanPredicate {
    type Error = PrivateConversionError;

    /// Decodes one closed leaf predicate, validating the operator's literal
    /// cardinality: comparisons carry exactly one literal, `IN` at least one
    /// literal of a single type, and null-checks none.
    ///
    /// # Errors
    /// Returns [`PrivateConversionError`] when the operator is unspecified or
    /// unknown, the leaf is missing or malformed, a literal is malformed, the
    /// literal count violates the operator's cardinality, or an `IN` list
    /// mixes literal types.
    fn try_from(value: proto::ScanPredicate) -> Result<Self, Self::Error> {
        let op = proto::ScanPredicateOp::try_from(value.op)
            .map_err(|_| PrivateConversionError::RequiredEnum("scan_predicate.op"))?;
        if op == proto::ScanPredicateOp::Unspecified {
            return Err(PrivateConversionError::RequiredEnum("scan_predicate.op"));
        }
        let leaf: assignment_authority::ScanLeaf = value
            .leaf
            .ok_or(PrivateConversionError::Missing("scan_predicate.leaf"))?
            .try_into()?;
        let mut literals = value
            .literals
            .into_iter()
            .map(assignment_authority::ScanLiteral::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        let cardinality = PrivateConversionError::Invalid {
            field: "scan_predicate.literals",
        };
        match op {
            proto::ScanPredicateOp::IsNull | proto::ScanPredicateOp::IsNotNull => {
                if !literals.is_empty() {
                    return Err(cardinality);
                }
                Ok(if op == proto::ScanPredicateOp::IsNull {
                    Self::IsNull(leaf)
                } else {
                    Self::IsNotNull(leaf)
                })
            }
            proto::ScanPredicateOp::In => {
                let Some(first) = literals.first() else {
                    return Err(cardinality);
                };
                let tag = first.digest_tag();
                if literals.iter().any(|literal| literal.digest_tag() != tag) {
                    return Err(cardinality);
                }
                Ok(Self::In(leaf, literals))
            }
            comparison => {
                let (Some(literal), None) = (literals.pop(), literals.pop()) else {
                    return Err(cardinality);
                };
                Ok(match comparison {
                    proto::ScanPredicateOp::Eq => Self::Eq(leaf, literal),
                    proto::ScanPredicateOp::NotEq => Self::NotEq(leaf, literal),
                    proto::ScanPredicateOp::Lt => Self::Lt(leaf, literal),
                    proto::ScanPredicateOp::LtEq => Self::LtEq(leaf, literal),
                    proto::ScanPredicateOp::Gt => Self::Gt(leaf, literal),
                    proto::ScanPredicateOp::GtEq => Self::GtEq(leaf, literal),
                    proto::ScanPredicateOp::Unspecified
                    | proto::ScanPredicateOp::In
                    | proto::ScanPredicateOp::IsNull
                    | proto::ScanPredicateOp::IsNotNull => unreachable!(
                        "comparison arm excludes Unspecified/In/IsNull/IsNotNull by construction"
                    ),
                })
            }
        }
    }
}

impl From<assignment_authority::ScanPredicate> for proto::ScanPredicate {
    /// Encodes one closed leaf predicate into its protobuf op/leaf/literals
    /// shape, preserving `IN` literal order.
    fn from(value: assignment_authority::ScanPredicate) -> Self {
        use assignment_authority::ScanPredicate as Domain;
        let (op, leaf, literals) = match value {
            Domain::Eq(leaf, literal) => (proto::ScanPredicateOp::Eq, leaf, vec![literal]),
            Domain::NotEq(leaf, literal) => (proto::ScanPredicateOp::NotEq, leaf, vec![literal]),
            Domain::Lt(leaf, literal) => (proto::ScanPredicateOp::Lt, leaf, vec![literal]),
            Domain::LtEq(leaf, literal) => (proto::ScanPredicateOp::LtEq, leaf, vec![literal]),
            Domain::Gt(leaf, literal) => (proto::ScanPredicateOp::Gt, leaf, vec![literal]),
            Domain::GtEq(leaf, literal) => (proto::ScanPredicateOp::GtEq, leaf, vec![literal]),
            Domain::In(leaf, literals) => (proto::ScanPredicateOp::In, leaf, literals),
            Domain::IsNull(leaf) => (proto::ScanPredicateOp::IsNull, leaf, Vec::new()),
            Domain::IsNotNull(leaf) => (proto::ScanPredicateOp::IsNotNull, leaf, Vec::new()),
        };
        Self {
            op: op as i32,
            leaf: Some(leaf.into()),
            literals: literals.into_iter().map(Into::into).collect(),
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
                files: persisted
                    .descriptors
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<_>, _>>()?,
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
                descriptors: value.persisted.files.into_iter().map(Into::into).collect(),
            }),
            scribe_provider_cut: value.scribe_provider_cut.map(Into::into),
            schema_fingerprint: value.schema_fingerprint,
            required_columns: value.required_columns,
            predicates: value.predicates.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<proto::PersistedFileDescriptor> for domain::PersistedFileDescriptor {
    type Error = PrivateConversionError;

    /// Decodes one signed persisted-file descriptor.
    ///
    /// The wire `source` oneof is the only authority for which variant this is:
    /// the source decides which reader and which cache identity the object
    /// gets, so it is never inferred from the path or from a `scan_id` suffix.
    ///
    /// Every shape rule is enforced here rather than at the point of use: a
    /// descriptor that survives this decode is one the follower may resolve
    /// without consulting the catalog again, so a malformed one must never get
    /// that far. A zero-row file is well-formed — an empty object still
    /// participates in residual execution.
    ///
    /// # Errors
    /// Returns [`PrivateConversionError::Missing`] when the source oneof is
    /// absent, [`PrivateConversionError::InvalidUuid`] when the file-list
    /// identity is not exactly 16 bytes,
    /// [`PrivateConversionError::Invalid`] with field `hot_object_checksum`
    /// when the checksum does not decode to exactly 32 bytes, and
    /// [`PrivateConversionError::Invalid`] with field
    /// `persisted_file_descriptor` when the decoded descriptor has an empty
    /// path, a zero size, a nonpositive pinned snapshot, or an event-time pair
    /// that is half-present or reversed.
    fn try_from(value: proto::PersistedFileDescriptor) -> Result<Self, Self::Error> {
        use proto::persisted_file_descriptor::Source;
        let descriptor = match value
            .source
            .ok_or(PrivateConversionError::Missing("persisted_file_source"))?
        {
            Source::Hot(hot) => Self::Hot(domain::HotFileDescriptor {
                path: hot.path,
                size_bytes: hot.size_bytes,
                row_count: hot.row_count,
                file_list_id: uuid::Uuid::from_slice(&hot.file_list_id)
                    .map_err(|_| PrivateConversionError::InvalidUuid("hot_file_list_id"))?,
                sha256: <[u8; 32]>::try_from(hot.sha256.as_slice()).map_err(|_| {
                    PrivateConversionError::Invalid {
                        field: "hot_object_checksum",
                    }
                })?,
                min_event_time_micros: hot.min_event_time_micros,
                max_event_time_micros: hot.max_event_time_micros,
            }),
            Source::Iceberg(iceberg) => Self::Iceberg(domain::IcebergFileDescriptor {
                path: iceberg.path,
                size_bytes: iceberg.size_bytes,
                row_count: iceberg.row_count,
                snapshot_id: iceberg.snapshot_id,
                min_event_time_micros: iceberg.min_event_time_micros,
                max_event_time_micros: iceberg.max_event_time_micros,
            }),
        };
        if !descriptor.is_valid() {
            return Err(PrivateConversionError::Invalid {
                field: "persisted_file_descriptor",
            });
        }
        Ok(descriptor)
    }
}

impl From<domain::PersistedFileDescriptor> for proto::PersistedFileDescriptor {
    /// Encodes one signed persisted-file descriptor onto the private wire.
    fn from(value: domain::PersistedFileDescriptor) -> Self {
        use proto::persisted_file_descriptor::Source;
        let source = match value {
            domain::PersistedFileDescriptor::Hot(hot) => Source::Hot(proto::HotFileDescriptor {
                path: hot.path,
                size_bytes: hot.size_bytes,
                row_count: hot.row_count,
                file_list_id: hot.file_list_id.as_bytes().to_vec(),
                sha256: hot.sha256.to_vec(),
                min_event_time_micros: hot.min_event_time_micros,
                max_event_time_micros: hot.max_event_time_micros,
            }),
            domain::PersistedFileDescriptor::Iceberg(iceberg) => {
                Source::Iceberg(proto::IcebergFileDescriptor {
                    path: iceberg.path,
                    size_bytes: iceberg.size_bytes,
                    row_count: iceberg.row_count,
                    snapshot_id: iceberg.snapshot_id,
                    min_event_time_micros: iceberg.min_event_time_micros,
                    max_event_time_micros: iceberg.max_event_time_micros,
                })
            }
        };
        Self {
            source: Some(source),
        }
    }
}

impl TryFrom<proto::ScribeProviderCut> for domain::ScribeProviderCut {
    type Error = PrivateConversionError;

    /// Decodes the bounded Scribe provider projection.
    ///
    /// The cut bounds partitions and writer incarnation only. It
    /// carries no published-WAL statement, because Scribe's generation
    /// authority is the exact answer to what a follower may still read from
    /// memory and a wire-carried interval would be a weaker second one.
    ///
    /// # Errors
    /// Returns a conversion error when either partition endpoint is absent or
    /// the complete cut violates its canonical ordering.
    fn try_from(value: proto::ScribeProviderCut) -> Result<Self, Self::Error> {
        let cut = Self {
            writer_epoch: value.writer_epoch,
            start_partition: time_partition(value.start_partition, "start_partition")?,
            end_partition: time_partition(value.end_partition, "end_partition")?,
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
        }
    }
}

impl TryFrom<proto::ExecuteFragmentRequest> for domain::ExecuteFragmentRequest {
    type Error = PrivateConversionError;

    /// Decodes one authenticated worker-fragment execution request.
    ///
    /// # Errors
    /// Returns [`PrivateConversionError`] for a missing or oversized context, an
    /// empty fragment payload, or a malformed reservation identifier.
    fn try_from(value: proto::ExecuteFragmentRequest) -> Result<Self, Self::Error> {
        if value.physical_plan_bytes.is_empty() {
            return Err(PrivateConversionError::Invalid {
                field: "physical_plan_bytes",
            });
        }
        nonempty(&value.plan_fingerprint, "plan_fingerprint")?;
        Ok(Self {
            context: value
                .context
                .ok_or(PrivateConversionError::Missing("context"))?
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
            context: Some(value.context.into()),
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
            row_groups_pruned_bloom: value.row_groups_pruned_bloom,
            rows_pruned_page_index: value.rows_pruned_page_index,
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
            row_groups_pruned_bloom: value.row_groups_pruned_bloom,
            rows_pruned_page_index: value.rows_pruned_page_index,
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

    /// One well-formed hot descriptor for assignment round-trip fixtures.
    fn test_hot_descriptor() -> domain::PersistedFileDescriptor {
        domain::PersistedFileDescriptor::Hot(domain::HotFileDescriptor {
            path: "s3://bucket/logs/a.parquet".to_owned(),
            size_bytes: 4_194_304,
            row_count: 128,
            file_list_id: uuid::Uuid::parse_str("0f0e0d0c-0b0a-0908-0706-050403020100")
                .expect("fixture file-list identity parses"),
            sha256: [0x11; 32],
            min_event_time_micros: Some(1_787_493_600_000_000),
            max_event_time_micros: Some(1_787_497_200_000_000),
        })
    }

    /// Builds one otherwise valid wire Iceberg descriptor for shape rejection.
    fn wire_iceberg(
        path: &str,
        size_bytes: u64,
        snapshot_id: i64,
        bounds: (Option<i64>, Option<i64>),
    ) -> proto::PersistedFileDescriptor {
        proto::PersistedFileDescriptor {
            source: Some(proto::persisted_file_descriptor::Source::Iceberg(
                proto::IcebergFileDescriptor {
                    path: path.to_owned(),
                    size_bytes,
                    row_count: 128,
                    snapshot_id,
                    min_event_time_micros: bounds.0,
                    max_event_time_micros: bounds.1,
                },
            )),
        }
    }

    /// A follower trusts the descriptor instead of re-querying the catalog, so
    /// the private wire must round-trip every field of both variants exactly and
    /// refuse every malformed shape rather than defaulting it.
    ///
    /// The rejections matter more than the round-trip: a defaulted source would
    /// silently reclassify which reader and cache identity an object gets, and a
    /// descriptor that passed shape validation with a zero size or a
    /// half-present interval would reach the follower's resolution path.
    #[test]
    fn follower_assignment_descriptor_round_trips_and_rejects_malformed_variants() {
        for descriptor in [
            test_hot_descriptor(),
            domain::PersistedFileDescriptor::Iceberg(domain::IcebergFileDescriptor {
                path: "s3://bucket/logs/published.parquet".to_owned(),
                size_bytes: 8_388_608,
                row_count: 0,
                snapshot_id: 8_675_309,
                min_event_time_micros: None,
                max_event_time_micros: None,
            }),
        ] {
            let encoded = proto::PersistedFileDescriptor::from(descriptor.clone());
            let decoded = domain::PersistedFileDescriptor::try_from(encoded)
                .expect("a well-formed descriptor round-trips");
            assert_eq!(
                decoded, descriptor,
                "a zero-row file is valid and must survive the round trip"
            );
        }

        // An absent source oneof is rejected, never defaulted to a variant.
        assert!(matches!(
            domain::PersistedFileDescriptor::try_from(proto::PersistedFileDescriptor {
                source: None
            }),
            Err(PrivateConversionError::Missing("persisted_file_source"))
        ));

        // A file-list identity that is not 16 bytes is rejected.
        assert!(matches!(
            domain::PersistedFileDescriptor::try_from(proto::PersistedFileDescriptor {
                source: Some(proto::persisted_file_descriptor::Source::Hot(
                    proto::HotFileDescriptor {
                        path: "s3://bucket/logs/a.parquet".to_owned(),
                        size_bytes: 1,
                        row_count: 1,
                        file_list_id: vec![0u8; 4],
                        sha256: vec![0x11; 32],
                        min_event_time_micros: None,
                        max_event_time_micros: None,
                    }
                )),
            }),
            Err(PrivateConversionError::InvalidUuid("hot_file_list_id"))
        ));

        // A checksum that does not decode to exactly 32 bytes is a different
        // identity domain, not a truncation to tolerate.
        assert!(matches!(
            domain::PersistedFileDescriptor::try_from(proto::PersistedFileDescriptor {
                source: Some(proto::persisted_file_descriptor::Source::Hot(
                    proto::HotFileDescriptor {
                        path: "s3://bucket/logs/a.parquet".to_owned(),
                        size_bytes: 1,
                        row_count: 1,
                        file_list_id: uuid::Uuid::from_u128(7).as_bytes().to_vec(),
                        sha256: vec![0x11; 16],
                        min_event_time_micros: None,
                        max_event_time_micros: None,
                    }
                )),
            }),
            Err(PrivateConversionError::Invalid {
                field: "hot_object_checksum"
            })
        ));

        // Shape violations the digest cannot catch: an empty path, a zero size,
        // a nonpositive pinned snapshot, and a half-present or reversed interval.
        for malformed in [
            wire_iceberg("", 1, 1, (None, None)),
            wire_iceberg("s3://bucket/logs/a.parquet", 0, 1, (None, None)),
            wire_iceberg("s3://bucket/logs/a.parquet", 1, 0, (None, None)),
            wire_iceberg("s3://bucket/logs/a.parquet", 1, -1, (None, None)),
            wire_iceberg("s3://bucket/logs/a.parquet", 1, 1, (Some(10), None)),
            wire_iceberg("s3://bucket/logs/a.parquet", 1, 1, (None, Some(10))),
            wire_iceberg("s3://bucket/logs/a.parquet", 1, 1, (Some(20), Some(10))),
        ] {
            assert!(
                matches!(
                    domain::PersistedFileDescriptor::try_from(malformed.clone()),
                    Err(PrivateConversionError::Invalid {
                        field: "persisted_file_descriptor"
                    })
                ),
                "{malformed:?} must be refused"
            );
        }
    }

    /// A reservation that names no graph fails before reaching a follower.
    #[test]
    fn reserve_slots_rejects_missing_graph() {
        let request = proto::ReserveNodeSlotsRequest {
            query_id: uuid::Uuid::now_v7().as_bytes().to_vec(),
            leader_node_id: uuid::Uuid::now_v7().to_string(),
            leader_fencing_token: 1,
            expires_at_unix_ms: 1,
            context: None,
            graph: None,
        };
        assert!(matches!(
            domain::ReserveNodeSlotsRequest::try_from(request),
            Err(PrivateConversionError::Missing("graph"))
        ));
    }

    /// Every private UUID byte field requires exactly 16 bytes.
    #[test]
    fn held_grant_rejects_malformed_uuid_bytes() {
        let response = proto::ReserveNodeSlotsResponse {
            outcome: Some(proto::reserve_node_slots_response::Outcome::Pending(
                proto::PendingNodeReservation {
                    reservation_id: vec![0; 15],
                },
            )),
        };
        assert!(matches!(
            domain::ReserveNodeSlotsResponse::try_from(response),
            Err(PrivateConversionError::InvalidUuid("reservation_id"))
        ));
    }

    /// Peer context bounds are enforced before typed claims can be decoded.
    #[test]
    fn peer_context_rejects_oversized_claims() {
        let context = proto::PeerContext {
            claims_bytes: vec![0; MAX_CLAIMS_BYTES + 1],
        };
        assert!(matches!(
            domain::PeerContext::try_from(context),
            Err(PrivateConversionError::TooLarge {
                field: "claims_bytes"
            })
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
            expires_at: chrono::DateTime::from_timestamp_millis(99).expect("valid timestamp"),
            graph: domain::AnalyticalGraphRef {
                public_query_id: uuid::Uuid::now_v7(),
                datafusion_query_id: uuid::Uuid::now_v7(),
            },
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

    /// Tail discovery bindings and stream identities preserve exact identities.
    #[test]
    fn private_tail_discovery_messages_round_trip() {
        let binding = domain::TenantTableBinding {
            tenant_id: wyrd_spec::DataTenantId::new_v7(),
            namespace: "vala".into(),
            table: "traces".into(),
        };
        assert_eq!(
            domain::TenantTableBinding::try_from(proto::TenantTableBinding::from(binding.clone()))
                .expect("binding round-trips"),
            binding
        );
        let stream = domain::TailStreamIdentity {
            node_id: domain::NodeId::new(uuid::Uuid::now_v7()),
            writer_epoch: 4,
        };
        assert_eq!(
            domain::TailStreamIdentity::try_from(proto::TailStreamIdentity::from(stream.clone()))
                .expect("stream identity round-trips"),
            stream
        );
    }

    /// Held-grant outcomes and execution requests round-trip.
    #[test]
    fn private_peer_messages_round_trip() {
        let reservation_id = domain::ReservationId::new(uuid::Uuid::now_v7());
        let outcome = domain::ReserveNodeSlotsResponse::Pending(domain::PendingNodeReservation {
            reservation_id,
        });
        assert_eq!(
            domain::ReserveNodeSlotsResponse::try_from(proto::ReserveNodeSlotsResponse::from(
                outcome.clone()
            ))
            .expect("reservation outcome round-trips"),
            outcome
        );
        let execute = domain::ExecuteFragmentRequest {
            context: domain::PeerContext {
                claims_bytes: vec![1, 2],
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

    /// A completed worker footer round-trips as the terminal frame.
    ///
    /// # Panics
    ///
    /// Panics when a fixture digest is invalid, the footer fails to convert
    /// back from the wire, or the round-tripped frame differs.
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
                row_groups_pruned_bloom: 3,
                rows_pruned_page_index: 11,
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
    ///
    /// # Panics
    ///
    /// Panics when a valid scalar or operator fails to round-trip through the
    /// v2 wire, or a malformed closed-shape predicate is accepted.
    #[test]
    fn follower_assignment_v2_contract() {
        let predicates = vec![
            assignment_authority::ScanPredicate::Eq(
                assignment_authority::ScanLeaf::Column("service_name".into()),
                assignment_authority::ScanLiteral::Utf8("api".into()),
            ),
            assignment_authority::ScanPredicate::NotEq(
                assignment_authority::ScanLeaf::Column("status".into()),
                assignment_authority::ScanLiteral::I64(-1),
            ),
            assignment_authority::ScanPredicate::Lt(
                assignment_authority::ScanLeaf::Column("duration_ms".into()),
                assignment_authority::ScanLiteral::U64(500),
            ),
            assignment_authority::ScanPredicate::LtEq(
                assignment_authority::ScanLeaf::Column("score".into()),
                assignment_authority::ScanLiteral::F64Bits(1.5_f64.to_bits()),
            ),
            assignment_authority::ScanPredicate::Gt(
                assignment_authority::ScanLeaf::Column("wyrd_event_time".into()),
                assignment_authority::ScanLiteral::TimestampMicros(1_000_000),
            ),
            assignment_authority::ScanPredicate::GtEq(
                assignment_authority::ScanLeaf::Column("active".into()),
                assignment_authority::ScanLiteral::Bool(true),
            ),
            assignment_authority::ScanPredicate::Eq(
                assignment_authority::ScanLeaf::Column("trace_id".into()),
                assignment_authority::ScanLiteral::Bytes(vec![0xff, 0x00, 0x7f]),
            ),
            assignment_authority::ScanPredicate::IsNull(assignment_authority::ScanLeaf::Column(
                "optional_field".into(),
            )),
            assignment_authority::ScanPredicate::IsNotNull(assignment_authority::ScanLeaf::Column(
                "required_field".into(),
            )),
        ];

        let expected = domain::FollowerScanAssignment {
            scan_id: "scan-1".into(),
            binding: domain::TenantTableBinding {
                tenant_id: wyrd_spec::DataTenantId::new_v7(),
                namespace: "logs".into(),
                table: "records".into(),
            },
            persisted: domain::PersistedFileAssignment {
                files: vec![test_hot_descriptor()],
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
    }

    /// Every leaf kind and operator round-trips through the in-place protobuf
    /// predicate, and every malformed tag, leaf, path, cardinality, and mixed
    /// `IN` list is refused at this sole wire/domain boundary.
    ///
    /// # Panics
    ///
    /// Panics when a valid predicate fails to round-trip or a malformed one is
    /// accepted.
    #[test]
    fn leaf_predicates_round_trip_and_reject_malformed() {
        use crate::wyrd::v1::scan_leaf_ref::Kind;
        use assignment_authority::{ScanLiteral, ScanPredicate};
        let names = |values: &[&str]| values.iter().map(|v| (*v).to_string()).collect::<Vec<_>>();
        let leaves = [
            assignment_authority::ScanLeaf::Column("service_name".into()),
            assignment_authority::ScanLeaf::StructField {
                column: "drift_report".into(),
                fields: names(&["method"]),
            },
            assignment_authority::ScanLeaf::Variant {
                column: "attributes".into(),
                keys: names(&["http", "route"]),
            },
        ];
        let utf8 = |value: &str| ScanLiteral::Utf8(value.into());
        for leaf in leaves {
            let predicates = [
                ScanPredicate::Eq(leaf.clone(), utf8("a")),
                ScanPredicate::NotEq(leaf.clone(), ScanLiteral::I64(-1)),
                ScanPredicate::Lt(leaf.clone(), ScanLiteral::U64(5)),
                ScanPredicate::LtEq(leaf.clone(), ScanLiteral::F64Bits(1.5_f64.to_bits())),
                ScanPredicate::Gt(leaf.clone(), ScanLiteral::TimestampMicros(1)),
                ScanPredicate::GtEq(leaf.clone(), ScanLiteral::Bool(true)),
                ScanPredicate::In(leaf.clone(), vec![utf8("b"), utf8("a")]),
                ScanPredicate::In(leaf.clone(), vec![ScanLiteral::Bytes(vec![0xff])]),
                ScanPredicate::IsNull(leaf.clone()),
                ScanPredicate::IsNotNull(leaf.clone()),
            ];
            for predicate in predicates {
                let wire = proto::ScanPredicate::from(predicate.clone());
                assert_eq!(ScanPredicate::try_from(wire), Ok(predicate));
            }
        }

        let literal = |value: &str| proto::ScanLiteral::from(utf8(value));
        let column = |name: &str| proto::ScanLeafRef {
            kind: Some(Kind::Column(proto::ScanColumnRef {
                column: name.into(),
            })),
        };
        let predicate =
            |op: proto::ScanPredicateOp,
             leaf: Option<proto::ScanLeafRef>,
             literals: Vec<proto::ScanLiteral>| proto::ScanPredicate {
                op: op as i32,
                leaf,
                literals,
            };
        use proto::ScanPredicateOp as Op;
        let cases = [
            (
                predicate(Op::Unspecified, Some(column("x")), vec![literal("a")]),
                PrivateConversionError::RequiredEnum("scan_predicate.op"),
            ),
            (
                proto::ScanPredicate {
                    op: 99,
                    leaf: Some(column("x")),
                    literals: vec![literal("a")],
                },
                PrivateConversionError::RequiredEnum("scan_predicate.op"),
            ),
            (
                predicate(Op::Eq, None, vec![literal("a")]),
                PrivateConversionError::Missing("scan_predicate.leaf"),
            ),
            (
                predicate(
                    Op::Eq,
                    Some(proto::ScanLeafRef { kind: None }),
                    vec![literal("a")],
                ),
                PrivateConversionError::Missing("scan_predicate.leaf.kind"),
            ),
            (
                predicate(Op::Eq, Some(column(" ")), vec![literal("a")]),
                PrivateConversionError::Invalid {
                    field: "scan_predicate.leaf.column",
                },
            ),
            (
                predicate(
                    Op::Eq,
                    Some(proto::ScanLeafRef {
                        kind: Some(Kind::StructField(proto::ScanStructRef {
                            column: String::new(),
                            fields: names(&["a"]),
                        })),
                    }),
                    vec![literal("a")],
                ),
                PrivateConversionError::Invalid {
                    field: "scan_predicate.leaf.struct_field.column",
                },
            ),
            (
                predicate(
                    Op::Eq,
                    Some(proto::ScanLeafRef {
                        kind: Some(Kind::StructField(proto::ScanStructRef {
                            column: "s".into(),
                            fields: Vec::new(),
                        })),
                    }),
                    vec![literal("a")],
                ),
                PrivateConversionError::Invalid {
                    field: "scan_predicate.leaf.struct_field.fields",
                },
            ),
            (
                predicate(
                    Op::Eq,
                    Some(proto::ScanLeafRef {
                        kind: Some(Kind::Variant(proto::ScanVariantRef {
                            column: "v".into(),
                            keys: Vec::new(),
                        })),
                    }),
                    vec![literal("a")],
                ),
                PrivateConversionError::Invalid {
                    field: "scan_predicate.leaf.variant.keys",
                },
            ),
            (
                predicate(
                    Op::Eq,
                    Some(proto::ScanLeafRef {
                        kind: Some(Kind::Variant(proto::ScanVariantRef {
                            column: "v".into(),
                            keys: names(&["a", ""]),
                        })),
                    }),
                    vec![literal("a")],
                ),
                PrivateConversionError::Invalid {
                    field: "scan_predicate.leaf.variant.keys",
                },
            ),
            (
                predicate(Op::Eq, Some(column("x")), Vec::new()),
                PrivateConversionError::Invalid {
                    field: "scan_predicate.literals",
                },
            ),
            (
                predicate(Op::Gt, Some(column("x")), vec![literal("a"), literal("b")]),
                PrivateConversionError::Invalid {
                    field: "scan_predicate.literals",
                },
            ),
            (
                predicate(Op::IsNull, Some(column("x")), vec![literal("a")]),
                PrivateConversionError::Invalid {
                    field: "scan_predicate.literals",
                },
            ),
            (
                predicate(Op::IsNotNull, Some(column("x")), vec![literal("a")]),
                PrivateConversionError::Invalid {
                    field: "scan_predicate.literals",
                },
            ),
            (
                predicate(Op::In, Some(column("x")), Vec::new()),
                PrivateConversionError::Invalid {
                    field: "scan_predicate.literals",
                },
            ),
            (
                predicate(
                    Op::In,
                    Some(column("x")),
                    vec![literal("a"), proto::ScanLiteral::from(ScanLiteral::I64(1))],
                ),
                PrivateConversionError::Invalid {
                    field: "scan_predicate.literals",
                },
            ),
            (
                predicate(
                    Op::In,
                    Some(column("x")),
                    vec![proto::ScanLiteral { value: None }],
                ),
                PrivateConversionError::Missing("scan_literal.value"),
            ),
        ];
        for (wire, expected) in cases {
            assert_eq!(ScanPredicate::try_from(wire), Err(expected));
        }
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
                files: vec![test_hot_descriptor()],
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
