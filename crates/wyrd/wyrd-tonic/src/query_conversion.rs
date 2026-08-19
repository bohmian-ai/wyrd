//! Validated conversions between protobuf and pure Bifrost query contracts.

use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api as domain;

use crate::wyrd::v1 as proto;

/// Error returned when a protobuf message violates the logical query contract.
#[derive(Debug, thiserror::Error)]
pub enum QueryConversionError {
    /// A required enum used its protobuf `UNSPECIFIED` value.
    #[error("required protobuf enum `{0}` is unspecified or unknown")]
    RequiredEnum(&'static str),
    /// A required oneof frame was absent.
    #[error("query stream frame is missing its frame payload")]
    MissingFrame,
    /// A batch frame did not include its decoded row count.
    #[error("query batch frame is missing its decoded row count")]
    MissingBatchRowCount,
    /// A non-batch frame incorrectly supplied a batch row count.
    #[error("query non-batch frame supplied a batch row count")]
    UnexpectedBatchRowCount,
    /// Row accumulation overflowed the public terminal counter.
    #[error("query stream row count overflow")]
    RowCountOverflow,
    /// A frame arrived after the required terminal frame.
    #[error("query stream frame arrived after terminal")]
    FrameAfterTerminal,
    /// A schema, batch, or terminal violated the required stream order.
    #[error("query stream frame violates schema/batch/terminal order")]
    InvalidFrameOrder,
    /// Pure contract validation failed.
    #[error("invalid query contract: {0}")]
    Contract(#[from] domain::QueryContractError),
    /// A public request identity did not satisfy the UUIDv7 contract.
    #[error("running query request identity is invalid")]
    RequestId,
    /// A required nested lifecycle message was absent.
    #[error("running query field `{0}` is missing")]
    Missing(&'static str),
    /// A lifecycle timestamp was outside Chrono's supported range.
    #[error("running query timestamp `{0}` is invalid")]
    Timestamp(&'static str),
}

impl TryFrom<proto::RunningQuerySummary> for domain::RunningQuerySummary {
    type Error = QueryConversionError;

    /// Decodes a SQL-free public running-query summary.
    ///
    /// # Errors
    /// Returns [`QueryConversionError`] for malformed request identities,
    /// unknown enums, missing progress, or invalid timestamps.
    fn try_from(value: proto::RunningQuerySummary) -> Result<Self, Self::Error> {
        Ok(Self {
            request_id: RequestId::parse(&value.request_id)
                .map_err(|_| QueryConversionError::RequestId)?,
            query_class: query_class(value.query_class)?,
            started_at: timestamp(value.started_at_unix_ms, "started_at_unix_ms")?,
            deadline: timestamp(value.deadline_unix_ms, "deadline_unix_ms")?,
            state: running_query_state(value.state)?,
            progress: value
                .progress
                .ok_or(QueryConversionError::Missing("progress"))?
                .into(),
            cancellation_requested: value.cancellation_requested,
        })
    }
}

impl From<domain::RunningQuerySummary> for proto::RunningQuerySummary {
    /// Encodes a SQL-free public running-query summary.
    fn from(value: domain::RunningQuerySummary) -> Self {
        Self {
            request_id: value.request_id.to_string(),
            query_class: match value.query_class {
                domain::QueryClass::Interactive => proto::QueryClass::Interactive as i32,
                domain::QueryClass::Analytical => proto::QueryClass::Analytical as i32,
            },
            started_at_unix_ms: unix_millis(value.started_at),
            deadline_unix_ms: unix_millis(value.deadline),
            state: match value.state {
                domain::RunningQueryLifecycleState::Admitted => {
                    proto::RunningQueryLifecycleState::Admitted as i32
                }
                domain::RunningQueryLifecycleState::Running => {
                    proto::RunningQueryLifecycleState::Running as i32
                }
                domain::RunningQueryLifecycleState::Cancelling => {
                    proto::RunningQueryLifecycleState::Cancelling as i32
                }
            },
            progress: Some(value.progress.into()),
            cancellation_requested: value.cancellation_requested,
        }
    }
}

impl From<domain::RunningQueryProgress> for proto::RunningQueryProgress {
    /// Encodes aggregate progress from the immutable participant cut.
    fn from(value: domain::RunningQueryProgress) -> Self {
        Self {
            completed_participants: value.completed_participants,
            total_participants: value.total_participants,
        }
    }
}

impl From<proto::RunningQueryProgress> for domain::RunningQueryProgress {
    /// Decodes aggregate progress from the immutable participant cut.
    fn from(value: proto::RunningQueryProgress) -> Self {
        Self {
            completed_participants: value.completed_participants,
            total_participants: value.total_participants,
        }
    }
}

impl TryFrom<proto::ListRunningQueriesResponse> for domain::ListRunningQueriesResponse {
    type Error = QueryConversionError;

    /// Decodes all tenant-visible running-query summaries.
    ///
    /// # Errors
    /// Returns [`QueryConversionError`] when any contained summary is invalid.
    fn try_from(value: proto::ListRunningQueriesResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            queries: value
                .queries
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        })
    }
}

impl From<domain::ListRunningQueriesResponse> for proto::ListRunningQueriesResponse {
    /// Encodes all tenant-visible running-query summaries.
    fn from(value: domain::ListRunningQueriesResponse) -> Self {
        Self {
            queries: value.queries.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<proto::GetRunningQueryRequest> for domain::GetRunningQueryRequest {
    type Error = QueryConversionError;

    /// Decodes the sole public identifier accepted by a status control.
    ///
    /// # Errors
    /// Returns [`QueryConversionError::RequestId`] for non-UUIDv7 values.
    fn try_from(value: proto::GetRunningQueryRequest) -> Result<Self, Self::Error> {
        Ok(Self {
            request_id: RequestId::parse(&value.request_id)
                .map_err(|_| QueryConversionError::RequestId)?,
        })
    }
}

impl From<domain::GetRunningQueryRequest> for proto::GetRunningQueryRequest {
    /// Encodes the sole public identifier accepted by a status control.
    fn from(value: domain::GetRunningQueryRequest) -> Self {
        Self {
            request_id: value.request_id.to_string(),
        }
    }
}

impl TryFrom<proto::CancelRunningQueryRequest> for domain::CancelRunningQueryRequest {
    type Error = QueryConversionError;

    /// Decodes the sole public identifier accepted by a cancellation control.
    ///
    /// # Errors
    /// Returns [`QueryConversionError::RequestId`] for non-UUIDv7 values.
    fn try_from(value: proto::CancelRunningQueryRequest) -> Result<Self, Self::Error> {
        Ok(Self {
            request_id: RequestId::parse(&value.request_id)
                .map_err(|_| QueryConversionError::RequestId)?,
        })
    }
}

impl From<domain::CancelRunningQueryRequest> for proto::CancelRunningQueryRequest {
    /// Encodes the sole public identifier accepted by a cancellation control.
    fn from(value: domain::CancelRunningQueryRequest) -> Self {
        Self {
            request_id: value.request_id.to_string(),
        }
    }
}

impl TryFrom<proto::CancelRunningQueryResponse> for domain::CancelRunningQueryResponse {
    type Error = QueryConversionError;

    /// Decodes an idempotent cancellation response.
    ///
    /// # Errors
    /// Returns [`QueryConversionError::RequestId`] for non-UUIDv7 values.
    fn try_from(value: proto::CancelRunningQueryResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            request_id: RequestId::parse(&value.request_id)
                .map_err(|_| QueryConversionError::RequestId)?,
            cancellation_started: value.cancellation_started,
        })
    }
}

impl From<domain::CancelRunningQueryResponse> for proto::CancelRunningQueryResponse {
    /// Encodes an idempotent cancellation response.
    fn from(value: domain::CancelRunningQueryResponse) -> Self {
        Self {
            request_id: value.request_id.to_string(),
            cancellation_started: value.cancellation_started,
        }
    }
}

impl TryFrom<proto::BifrostQueryRequest> for domain::BifrostQueryRequest {
    type Error = QueryConversionError;

    /// Validates and converts a public protobuf query request into its pure contract.
    ///
    /// # Errors
    /// Returns [`QueryConversionError`] when an enum is unspecified or the
    /// resulting request violates a query-contract invariant.
    fn try_from(value: proto::BifrostQueryRequest) -> Result<Self, Self::Error> {
        let request = Self {
            sql: value.sql,
            visibility: visibility(value.visibility)?,
            freshness: freshness(value.freshness)?,
            deadline_ms: value.deadline_ms,
        };
        request.validate()?;
        Ok(request)
    }
}

impl From<domain::BifrostQueryRequest> for proto::BifrostQueryRequest {
    /// Projects a validated pure query request onto its protobuf wire shape.
    fn from(value: domain::BifrostQueryRequest) -> Self {
        Self {
            sql: value.sql,
            visibility: match value.visibility {
                domain::VisibilityMode::PublishedOnly => {
                    proto::VisibilityMode::PublishedOnly as i32
                }
                domain::VisibilityMode::Fused => proto::VisibilityMode::Fused as i32,
            },
            freshness: match value.freshness {
                domain::FreshnessPolicy::Strict => proto::FreshnessPolicy::Strict as i32,
                domain::FreshnessPolicy::AllowDegraded => {
                    proto::FreshnessPolicy::AllowDegraded as i32
                }
            },
            deadline_ms: value.deadline_ms,
        }
    }
}

impl From<domain::QueryStreamFrame> for proto::QueryStreamFrame {
    /// Encodes one validated query-stream frame into the corresponding wire oneof.
    fn from(value: domain::QueryStreamFrame) -> Self {
        use proto::query_stream_frame::Frame;
        let frame = match value {
            domain::QueryStreamFrame::Schema(value) => Frame::Schema(proto::QuerySchemaFrame {
                schema_fingerprint: value.schema_fingerprint,
                arrow_ipc_schema: value.arrow_ipc_schema,
            }),
            domain::QueryStreamFrame::Batch(value) => Frame::Batch(proto::QueryBatchFrame {
                arrow_ipc_batch: value.arrow_ipc_batch,
            }),
            domain::QueryStreamFrame::Terminal(value) => {
                Frame::Terminal(proto::QueryTerminalFrame {
                    outcome: match value.outcome {
                        domain::QueryTerminalOutcome::Success => {
                            proto::QueryTerminalOutcome::Success as i32
                        }
                        domain::QueryTerminalOutcome::Degraded => {
                            proto::QueryTerminalOutcome::Degraded as i32
                        }
                        domain::QueryTerminalOutcome::Failed => {
                            proto::QueryTerminalOutcome::Failed as i32
                        }
                    },
                    freshness: match value.freshness {
                        domain::QueryFreshness::Complete => proto::QueryFreshness::Complete as i32,
                        domain::QueryFreshness::Degraded => proto::QueryFreshness::Degraded as i32,
                    },
                    row_count: value.row_count,
                    warnings: value.warnings.into_iter().map(proto_warning).collect(),
                    source_completion: value
                        .source_completion
                        .into_iter()
                        .map(proto_source_completion)
                        .collect(),
                    error: value.error.map(proto_terminal_error),
                })
            }
        };
        Self { frame: Some(frame) }
    }
}

/// Request-scoped converter for one public query response stream.
pub struct QueryStreamConverter {
    /// Visibility admitted from the originating request.
    visibility: domain::VisibilityMode,
    /// Rows decoded from every accepted batch frame.
    emitted_rows: u64,
    /// Whether the unique initial schema has been accepted.
    schema_seen: bool,
    /// Whether the unique terminal has closed the stream.
    terminal_seen: bool,
}

impl QueryStreamConverter {
    /// Constructs conversion state from the admitted request visibility.
    #[must_use]
    pub fn new(visibility: domain::VisibilityMode) -> Self {
        Self {
            visibility,
            emitted_rows: 0,
            schema_seen: false,
            terminal_seen: false,
        }
    }

    /// Converts one ordered frame and validates terminal source/row invariants.
    ///
    /// `batch_row_count` is required exactly for batch frames and comes from
    /// the Arrow decoder that already validates the batch payload.
    ///
    /// # Errors
    /// Returns [`QueryConversionError`] for malformed frames, missing or
    /// unexpected batch row counts, overflow, frames after terminal, or a
    /// terminal inconsistent with the admitted request and emitted batches.
    pub fn convert(
        &mut self,
        value: proto::QueryStreamFrame,
        batch_row_count: Option<u64>,
    ) -> Result<domain::QueryStreamFrame, QueryConversionError> {
        if self.terminal_seen {
            return Err(QueryConversionError::FrameAfterTerminal);
        }
        match value.frame.ok_or(QueryConversionError::MissingFrame)? {
            proto::query_stream_frame::Frame::Schema(frame) => {
                reject_batch_rows(batch_row_count)?;
                if self.schema_seen {
                    return Err(QueryConversionError::InvalidFrameOrder);
                }
                self.schema_seen = true;
                Ok(domain::QueryStreamFrame::Schema(domain::QuerySchemaFrame {
                    schema_fingerprint: frame.schema_fingerprint,
                    arrow_ipc_schema: frame.arrow_ipc_schema,
                }))
            }
            proto::query_stream_frame::Frame::Batch(frame) => {
                if !self.schema_seen {
                    return Err(QueryConversionError::InvalidFrameOrder);
                }
                let rows = batch_row_count.ok_or(QueryConversionError::MissingBatchRowCount)?;
                self.emitted_rows = self
                    .emitted_rows
                    .checked_add(rows)
                    .ok_or(QueryConversionError::RowCountOverflow)?;
                Ok(domain::QueryStreamFrame::Batch(domain::QueryBatchFrame {
                    arrow_ipc_batch: frame.arrow_ipc_batch,
                }))
            }
            proto::query_stream_frame::Frame::Terminal(frame) => {
                reject_batch_rows(batch_row_count)?;
                if !self.schema_seen {
                    return Err(QueryConversionError::InvalidFrameOrder);
                }
                let terminal = terminal(frame, self.visibility, self.emitted_rows)?;
                self.terminal_seen = true;
                Ok(domain::QueryStreamFrame::Terminal(terminal))
            }
        }
    }
}

/// Decodes a server-derived query class without accepting protobuf's zero value.
///
/// # Errors
/// Returns [`QueryConversionError::RequiredEnum`] for unknown or unspecified classes.
fn query_class(value: i32) -> Result<domain::QueryClass, QueryConversionError> {
    match proto::QueryClass::try_from(value)
        .map_err(|_| QueryConversionError::RequiredEnum("query_class"))?
    {
        proto::QueryClass::Interactive => Ok(domain::QueryClass::Interactive),
        proto::QueryClass::Analytical => Ok(domain::QueryClass::Analytical),
        proto::QueryClass::Unspecified => Err(QueryConversionError::RequiredEnum("query_class")),
    }
}

/// Decodes a public live lifecycle state without accepting protobuf's zero value.
///
/// # Errors
/// Returns [`QueryConversionError::RequiredEnum`] for unknown or unspecified states.
fn running_query_state(
    value: i32,
) -> Result<domain::RunningQueryLifecycleState, QueryConversionError> {
    match proto::RunningQueryLifecycleState::try_from(value)
        .map_err(|_| QueryConversionError::RequiredEnum("state"))?
    {
        proto::RunningQueryLifecycleState::Admitted => {
            Ok(domain::RunningQueryLifecycleState::Admitted)
        }
        proto::RunningQueryLifecycleState::Running => {
            Ok(domain::RunningQueryLifecycleState::Running)
        }
        proto::RunningQueryLifecycleState::Cancelling => {
            Ok(domain::RunningQueryLifecycleState::Cancelling)
        }
        proto::RunningQueryLifecycleState::Unspecified => {
            Err(QueryConversionError::RequiredEnum("state"))
        }
    }
}

/// Converts a non-negative Unix-millisecond timestamp into UTC.
///
/// # Errors
/// Returns [`QueryConversionError::Timestamp`] when the protobuf value does not
/// fit Chrono's supported timestamp range.
fn timestamp(
    value: u64,
    field: &'static str,
) -> Result<chrono::DateTime<chrono::Utc>, QueryConversionError> {
    let value = i64::try_from(value).map_err(|_| QueryConversionError::Timestamp(field))?;
    chrono::DateTime::from_timestamp_millis(value).ok_or(QueryConversionError::Timestamp(field))
}

/// Converts a UTC timestamp into the non-negative Unix-millisecond wire form.
///
/// # Panics
///
/// Panics when a server-generated public lifecycle timestamp predates the Unix
/// epoch, which is outside this protobuf contract's representable range.
fn unix_millis(value: chrono::DateTime<chrono::Utc>) -> u64 {
    u64::try_from(value.timestamp_millis())
        .expect("invariant: public lifecycle timestamps are after the Unix epoch")
}

/// Rejects row metadata supplied for a non-batch frame.
///
/// # Errors
/// Returns [`QueryConversionError::UnexpectedBatchRowCount`] when present.
fn reject_batch_rows(value: Option<u64>) -> Result<(), QueryConversionError> {
    if value.is_some() {
        Err(QueryConversionError::UnexpectedBatchRowCount)
    } else {
        Ok(())
    }
}

/// Converts a terminal with authoritative request and accumulated-row context.
///
/// # Errors
/// Returns [`QueryConversionError`] for unknown enums, invalid closed terminal
/// combinations, or a row count unequal to preceding decoded batches.
fn terminal(
    value: proto::QueryTerminalFrame,
    visibility: domain::VisibilityMode,
    emitted_rows: u64,
) -> Result<domain::QueryTerminalFrame, QueryConversionError> {
    let terminal = domain::QueryTerminalFrame {
        outcome: match proto::QueryTerminalOutcome::try_from(value.outcome)
            .map_err(|_| QueryConversionError::RequiredEnum("outcome"))?
        {
            proto::QueryTerminalOutcome::Success => domain::QueryTerminalOutcome::Success,
            proto::QueryTerminalOutcome::Degraded => domain::QueryTerminalOutcome::Degraded,
            proto::QueryTerminalOutcome::Failed => domain::QueryTerminalOutcome::Failed,
            proto::QueryTerminalOutcome::Unspecified => {
                return Err(QueryConversionError::RequiredEnum("outcome"))
            }
        },
        freshness: match proto::QueryFreshness::try_from(value.freshness)
            .map_err(|_| QueryConversionError::RequiredEnum("freshness"))?
        {
            proto::QueryFreshness::Complete => domain::QueryFreshness::Complete,
            proto::QueryFreshness::Degraded => domain::QueryFreshness::Degraded,
            proto::QueryFreshness::Unspecified => {
                return Err(QueryConversionError::RequiredEnum("freshness"))
            }
        },
        row_count: value.row_count,
        warnings: value
            .warnings
            .into_iter()
            .map(warning)
            .collect::<Result<_, _>>()?,
        source_completion: value
            .source_completion
            .into_iter()
            .map(source_completion)
            .collect::<Result<_, _>>()?,
        error: value.error.map(terminal_error).transpose()?,
    };
    terminal.validate(visibility)?;
    terminal.validate_emitted_rows(emitted_rows)?;
    Ok(terminal)
}

/// Maps one closed domain warning to its protobuf discriminant.
fn proto_warning(value: domain::QueryWarning) -> i32 {
    match value {
        domain::QueryWarning::LiveTailUnavailable => {
            proto::QueryWarning::LiveTailUnavailable as i32
        }
        domain::QueryWarning::StaleCutReplanned => proto::QueryWarning::StaleCutReplanned as i32,
    }
}

/// Maps one closed domain source completion to protobuf.
fn proto_source_completion(value: domain::SourceCompletion) -> proto::SourceCompletion {
    proto::SourceCompletion {
        source: match value.source {
            domain::QuerySource::Iceberg => proto::QuerySource::Iceberg as i32,
            domain::QuerySource::HotSealed => proto::QuerySource::HotSealed as i32,
            domain::QuerySource::LiveTail => proto::QuerySource::LiveTail as i32,
        },
        outcome: match value.outcome {
            domain::SourceCompletionOutcome::Complete => {
                proto::SourceCompletionOutcome::Complete as i32
            }
            domain::SourceCompletionOutcome::Unavailable => {
                proto::SourceCompletionOutcome::Unavailable as i32
            }
        },
    }
}

/// Maps one bounded domain terminal error to protobuf.
fn proto_terminal_error(value: domain::QueryTerminalError) -> proto::QueryTerminalError {
    use domain::QueryTerminalErrorCode as D;
    use proto::QueryTerminalErrorCode as P;
    proto::QueryTerminalError {
        code: match value.code {
            D::QueryTimeout => P::QueryTimeout as i32,
            D::QueryVisibilityUnavailable => P::QueryVisibilityUnavailable as i32,
            D::QueryTenantInvariant => P::QueryTenantInvariant as i32,
            D::QueryReconciliationInvariant => P::QueryReconciliationInvariant as i32,
            D::QueryPeerSecurity => P::QueryPeerSecurity as i32,
            D::QueryAuditUnavailable => P::QueryAuditUnavailable as i32,
            D::CatalogUnreachable => P::CatalogUnreachable as i32,
            D::StorageUnreachable => P::StorageUnreachable as i32,
            D::QueryExecutionFailed => P::QueryExecutionFailed as i32,
        },
        detail: value.detail.map(|detail| detail.as_str().to_owned()),
    }
}

/// Decodes one required warning discriminant.
///
/// # Errors
/// Returns [`QueryConversionError::RequiredEnum`] for zero or unknown values.
fn warning(value: i32) -> Result<domain::QueryWarning, QueryConversionError> {
    match proto::QueryWarning::try_from(value)
        .map_err(|_| QueryConversionError::RequiredEnum("warning"))?
    {
        proto::QueryWarning::LiveTailUnavailable => Ok(domain::QueryWarning::LiveTailUnavailable),
        proto::QueryWarning::StaleCutReplanned => Ok(domain::QueryWarning::StaleCutReplanned),
        proto::QueryWarning::Unspecified => Err(QueryConversionError::RequiredEnum("warning")),
    }
}

/// Decodes one required source-completion pair.
///
/// # Errors
/// Returns [`QueryConversionError::RequiredEnum`] for zero or unknown fields.
fn source_completion(
    value: proto::SourceCompletion,
) -> Result<domain::SourceCompletion, QueryConversionError> {
    let source = match proto::QuerySource::try_from(value.source)
        .map_err(|_| QueryConversionError::RequiredEnum("source"))?
    {
        proto::QuerySource::Iceberg => domain::QuerySource::Iceberg,
        proto::QuerySource::HotSealed => domain::QuerySource::HotSealed,
        proto::QuerySource::LiveTail => domain::QuerySource::LiveTail,
        proto::QuerySource::Unspecified => {
            return Err(QueryConversionError::RequiredEnum("source"))
        }
    };
    let outcome = match proto::SourceCompletionOutcome::try_from(value.outcome)
        .map_err(|_| QueryConversionError::RequiredEnum("source_outcome"))?
    {
        proto::SourceCompletionOutcome::Complete => domain::SourceCompletionOutcome::Complete,
        proto::SourceCompletionOutcome::Unavailable => domain::SourceCompletionOutcome::Unavailable,
        proto::SourceCompletionOutcome::Unspecified => {
            return Err(QueryConversionError::RequiredEnum("source_outcome"))
        }
    };
    Ok(domain::SourceCompletion { source, outcome })
}

/// Decodes one bounded terminal error and its optional scrubbed detail.
///
/// # Errors
/// Returns [`QueryConversionError`] for an unknown code or invalid detail.
fn terminal_error(
    value: proto::QueryTerminalError,
) -> Result<domain::QueryTerminalError, QueryConversionError> {
    use domain::QueryTerminalErrorCode as D;
    use proto::QueryTerminalErrorCode as P;
    let code = match P::try_from(value.code)
        .map_err(|_| QueryConversionError::RequiredEnum("error_code"))?
    {
        P::QueryTimeout => D::QueryTimeout,
        P::QueryVisibilityUnavailable => D::QueryVisibilityUnavailable,
        P::QueryTenantInvariant => D::QueryTenantInvariant,
        P::QueryReconciliationInvariant => D::QueryReconciliationInvariant,
        P::QueryPeerSecurity => D::QueryPeerSecurity,
        P::QueryAuditUnavailable => D::QueryAuditUnavailable,
        P::CatalogUnreachable => D::CatalogUnreachable,
        P::StorageUnreachable => D::StorageUnreachable,
        P::QueryExecutionFailed => D::QueryExecutionFailed,
        P::Unspecified => return Err(QueryConversionError::RequiredEnum("error_code")),
    };
    Ok(domain::QueryTerminalError {
        code,
        detail: value
            .detail
            .map(domain::QueryErrorDetail::new)
            .transpose()?,
    })
}

/// Decodes required request visibility.
///
/// # Errors
/// Returns [`QueryConversionError::RequiredEnum`] for zero or unknown values.
fn visibility(value: i32) -> Result<domain::VisibilityMode, QueryConversionError> {
    match proto::VisibilityMode::try_from(value)
        .map_err(|_| QueryConversionError::RequiredEnum("visibility"))?
    {
        proto::VisibilityMode::PublishedOnly => Ok(domain::VisibilityMode::PublishedOnly),
        proto::VisibilityMode::Fused => Ok(domain::VisibilityMode::Fused),
        proto::VisibilityMode::Unspecified => Err(QueryConversionError::RequiredEnum("visibility")),
    }
}

/// Decodes required request freshness.
///
/// # Errors
/// Returns [`QueryConversionError::RequiredEnum`] for zero or unknown values.
fn freshness(value: i32) -> Result<domain::FreshnessPolicy, QueryConversionError> {
    match proto::FreshnessPolicy::try_from(value)
        .map_err(|_| QueryConversionError::RequiredEnum("freshness"))?
    {
        proto::FreshnessPolicy::Strict => Ok(domain::FreshnessPolicy::Strict),
        proto::FreshnessPolicy::AllowDegraded => Ok(domain::FreshnessPolicy::AllowDegraded),
        proto::FreshnessPolicy::Unspecified => Err(QueryConversionError::RequiredEnum("freshness")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Required enum zero values fail before reaching a runtime owner.
    #[test]
    fn unspecified_request_enum_is_rejected() {
        let request = proto::BifrostQueryRequest {
            sql: "SELECT 1".into(),
            visibility: 0,
            freshness: proto::FreshnessPolicy::AllowDegraded as i32,
            deadline_ms: None,
        };
        assert!(matches!(
            domain::BifrostQueryRequest::try_from(request),
            Err(QueryConversionError::RequiredEnum("visibility"))
        ));
    }

    /// Missing frame oneofs are rejected.
    #[test]
    fn missing_stream_frame_is_rejected() {
        let mut converter = QueryStreamConverter::new(domain::VisibilityMode::PublishedOnly);
        assert!(matches!(
            converter.convert(proto::QueryStreamFrame { frame: None }, None),
            Err(QueryConversionError::MissingFrame)
        ));
    }

    /// Unspecified nested enums are rejected.
    #[test]
    fn unspecified_terminal_enum_is_rejected() {
        let frame = proto::QueryStreamFrame {
            frame: Some(proto::query_stream_frame::Frame::Terminal(
                proto::QueryTerminalFrame {
                    outcome: 0,
                    freshness: proto::QueryFreshness::Complete as i32,
                    row_count: 0,
                    warnings: vec![],
                    source_completion: vec![],
                    error: None,
                },
            )),
        };
        let mut converter = QueryStreamConverter::new(domain::VisibilityMode::PublishedOnly);
        prime_schema(&mut converter);
        assert!(matches!(
            converter.convert(frame, None),
            Err(QueryConversionError::RequiredEnum("outcome"))
        ));
    }

    /// Public requests and frames round-trip their closed wire representation.
    #[test]
    fn public_query_messages_round_trip() {
        let request = domain::BifrostQueryRequest {
            sql: "SELECT 1".into(),
            visibility: domain::VisibilityMode::Fused,
            freshness: domain::FreshnessPolicy::Strict,
            deadline_ms: Some(100),
        };
        assert_eq!(
            domain::BifrostQueryRequest::try_from(proto::BifrostQueryRequest::from(
                request.clone()
            ))
            .expect("request round-trips"),
            request
        );
        let frame = domain::QueryStreamFrame::Schema(domain::QuerySchemaFrame {
            schema_fingerprint: "schema-1".into(),
            arrow_ipc_schema: vec![1, 2, 3],
        });
        let mut converter = QueryStreamConverter::new(domain::VisibilityMode::PublishedOnly);
        assert_eq!(
            converter
                .convert(proto::QueryStreamFrame::from(frame.clone()), None)
                .expect("frame round-trips"),
            frame
        );
    }

    /// Invalid terminal combinations fail at the protobuf boundary.
    #[test]
    fn invalid_terminal_matrix_is_rejected() {
        let sealed = vec![
            proto::SourceCompletion {
                source: proto::QuerySource::Iceberg as i32,
                outcome: proto::SourceCompletionOutcome::Complete as i32,
            },
            proto::SourceCompletion {
                source: proto::QuerySource::HotSealed as i32,
                outcome: proto::SourceCompletionOutcome::Complete as i32,
            },
        ];
        for terminal in [
            proto::QueryTerminalFrame {
                outcome: proto::QueryTerminalOutcome::Failed as i32,
                freshness: proto::QueryFreshness::Complete as i32,
                row_count: 0,
                warnings: vec![],
                source_completion: sealed.clone(),
                error: None,
            },
            proto::QueryTerminalFrame {
                outcome: proto::QueryTerminalOutcome::Success as i32,
                freshness: proto::QueryFreshness::Degraded as i32,
                row_count: 0,
                warnings: vec![],
                source_completion: sealed.clone(),
                error: None,
            },
            proto::QueryTerminalFrame {
                outcome: proto::QueryTerminalOutcome::Success as i32,
                freshness: proto::QueryFreshness::Complete as i32,
                row_count: 0,
                warnings: vec![proto::QueryWarning::LiveTailUnavailable as i32],
                source_completion: sealed.clone(),
                error: None,
            },
        ] {
            let frame = proto::QueryStreamFrame {
                frame: Some(proto::query_stream_frame::Frame::Terminal(terminal)),
            };
            let mut converter = QueryStreamConverter::new(domain::VisibilityMode::PublishedOnly);
            prime_schema(&mut converter);
            assert!(matches!(
                converter.convert(frame, None),
                Err(QueryConversionError::Contract(_))
            ));
        }
    }

    /// Authoritative PublishedOnly context rejects an injected live source.
    #[test]
    fn published_only_rejects_injected_live_tail() {
        let terminal = valid_terminal(true, 0);
        let mut converter = QueryStreamConverter::new(domain::VisibilityMode::PublishedOnly);
        prime_schema(&mut converter);
        assert!(matches!(
            converter.convert(terminal, None),
            Err(QueryConversionError::Contract(_))
        ));
    }

    /// Authoritative Fused context rejects a terminal missing live-tail state.
    #[test]
    fn fused_rejects_missing_live_tail() {
        let terminal = valid_terminal(false, 0);
        let mut converter = QueryStreamConverter::new(domain::VisibilityMode::Fused);
        prime_schema(&mut converter);
        assert!(matches!(
            converter.convert(terminal, None),
            Err(QueryConversionError::Contract(_))
        ));
    }

    /// Terminal row count must equal accumulated decoded batch rows.
    #[test]
    fn terminal_rejects_mismatched_accumulated_rows() {
        let batch = proto::QueryStreamFrame {
            frame: Some(proto::query_stream_frame::Frame::Batch(
                proto::QueryBatchFrame {
                    arrow_ipc_batch: vec![1],
                },
            )),
        };
        let mut converter = QueryStreamConverter::new(domain::VisibilityMode::PublishedOnly);
        prime_schema(&mut converter);
        converter.convert(batch, Some(2)).expect("batch converts");
        assert!(matches!(
            converter.convert(valid_terminal(false, 3), None),
            Err(QueryConversionError::Contract(_))
        ));
    }

    /// Builds a valid success terminal for the selected source set.
    fn valid_terminal(include_live: bool, row_count: u64) -> proto::QueryStreamFrame {
        let mut source_completion = vec![
            proto::SourceCompletion {
                source: proto::QuerySource::Iceberg as i32,
                outcome: proto::SourceCompletionOutcome::Complete as i32,
            },
            proto::SourceCompletion {
                source: proto::QuerySource::HotSealed as i32,
                outcome: proto::SourceCompletionOutcome::Complete as i32,
            },
        ];
        if include_live {
            source_completion.push(proto::SourceCompletion {
                source: proto::QuerySource::LiveTail as i32,
                outcome: proto::SourceCompletionOutcome::Complete as i32,
            });
        }
        proto::QueryStreamFrame {
            frame: Some(proto::query_stream_frame::Frame::Terminal(
                proto::QueryTerminalFrame {
                    outcome: proto::QueryTerminalOutcome::Success as i32,
                    freshness: proto::QueryFreshness::Complete as i32,
                    row_count,
                    warnings: vec![],
                    source_completion,
                    error: None,
                },
            )),
        }
    }

    /// Advances one converter through its required initial schema frame.
    fn prime_schema(converter: &mut QueryStreamConverter) {
        converter
            .convert(
                proto::QueryStreamFrame {
                    frame: Some(proto::query_stream_frame::Frame::Schema(
                        proto::QuerySchemaFrame {
                            schema_fingerprint: "schema-1".into(),
                            arrow_ipc_schema: vec![1],
                        },
                    )),
                },
                None,
            )
            .expect("schema converts");
    }

    /// Schema must be first and unique; no frames may follow terminal.
    #[test]
    fn stream_ordering_is_enforced() {
        let batch = || proto::QueryStreamFrame {
            frame: Some(proto::query_stream_frame::Frame::Batch(
                proto::QueryBatchFrame {
                    arrow_ipc_batch: vec![1],
                },
            )),
        };
        let schema = || proto::QueryStreamFrame {
            frame: Some(proto::query_stream_frame::Frame::Schema(
                proto::QuerySchemaFrame {
                    schema_fingerprint: "schema-1".into(),
                    arrow_ipc_schema: vec![1],
                },
            )),
        };
        let mut converter = QueryStreamConverter::new(domain::VisibilityMode::PublishedOnly);
        assert!(matches!(
            converter.convert(valid_terminal(false, 0), None),
            Err(QueryConversionError::InvalidFrameOrder)
        ));
        assert!(matches!(
            converter.convert(batch(), Some(1)),
            Err(QueryConversionError::InvalidFrameOrder)
        ));
        converter.convert(schema(), None).expect("first schema");
        assert!(matches!(
            converter.convert(schema(), None),
            Err(QueryConversionError::InvalidFrameOrder)
        ));
        converter
            .convert(valid_terminal(false, 0), None)
            .expect("terminal converts");
        assert!(matches!(
            converter.convert(batch(), Some(1)),
            Err(QueryConversionError::FrameAfterTerminal)
        ));
    }

    /// Public lifecycle summaries and cancellation controls preserve one request ID.
    #[test]
    fn running_query_contract_round_trips() {
        let request_id = RequestId::now_v7();
        let started_at = chrono::DateTime::from_timestamp_millis(1_725_000_000_123)
            .expect("fixed running-query timestamp is valid");
        let expected = domain::RunningQuerySummary {
            request_id: request_id.clone(),
            query_class: domain::QueryClass::Interactive,
            started_at,
            deadline: started_at + chrono::Duration::seconds(30),
            state: domain::RunningQueryLifecycleState::Running,
            progress: domain::RunningQueryProgress {
                completed_participants: 1,
                total_participants: 2,
            },
            cancellation_requested: false,
        };
        let actual = domain::RunningQuerySummary::try_from(proto::RunningQuerySummary::from(
            expected.clone(),
        ))
        .expect("valid running-query summary round-trips");
        assert_eq!(actual, expected);
        let list = domain::ListRunningQueriesResponse {
            queries: vec![expected],
        };
        let actual = domain::ListRunningQueriesResponse::try_from(
            proto::ListRunningQueriesResponse::from(list.clone()),
        )
        .expect("valid running-query list round-trips");
        assert_eq!(actual, list);
        let status = domain::GetRunningQueryRequest {
            request_id: request_id.clone(),
        };
        let actual = domain::GetRunningQueryRequest::try_from(proto::GetRunningQueryRequest::from(
            status.clone(),
        ))
        .expect("valid status request round-trips");
        assert_eq!(actual, status);
        let cancellation_request = domain::CancelRunningQueryRequest {
            request_id: request_id.clone(),
        };
        let actual = domain::CancelRunningQueryRequest::try_from(
            proto::CancelRunningQueryRequest::from(cancellation_request.clone()),
        )
        .expect("valid cancellation request round-trips");
        assert_eq!(actual, cancellation_request);
        let cancellation = domain::CancelRunningQueryResponse {
            request_id: request_id.clone(),
            cancellation_started: true,
        };
        let actual = domain::CancelRunningQueryResponse::try_from(
            proto::CancelRunningQueryResponse::from(cancellation.clone()),
        )
        .expect("valid cancellation response round-trips");
        assert_eq!(actual, cancellation);

        let tenant_id = wyrd_spec::DataTenantId::new_v7();
        let query_id = domain::QueryId::new(uuid::Uuid::from_u128(11));
        let leader = domain::OracleRoleFence {
            node_id: domain::NodeId::new(uuid::Uuid::from_u128(12)),
            role: domain::ClusterRole::Oracle,
            fencing_token: 13,
        };
        let admission = domain::AdmitOracleLifecycleRequest {
            tenant_id,
            request_id: request_id.clone(),
            query_id,
            query_class: domain::QueryClass::Interactive,
            deadline: started_at + chrono::Duration::seconds(30),
            cut_fingerprint: "sha256:cut".to_owned(),
            leader: leader.clone(),
            participants: vec![leader.clone()],
        };
        let actual = domain::AdmitOracleLifecycleRequest::try_from(
            proto::AdmitOracleLifecycleRequest::from(admission.clone()),
        )
        .expect("private admission round-trips");
        assert_eq!(actual, admission);
        let admission_response = domain::AdmitOracleLifecycleResponse { accepted: true };
        assert_eq!(
            domain::AdmitOracleLifecycleResponse::from(proto::AdmitOracleLifecycleResponse::from(
                admission_response
            )),
            admission_response
        );

        let follower = domain::ReportOracleFollowerLifecycleRequest {
            tenant_id,
            request_id: request_id.clone(),
            query_id,
            cut_fingerprint: "sha256:cut".to_owned(),
            follower: leader,
            outcome: domain::QueryTerminalOutcome::Success,
        };
        let actual = domain::ReportOracleFollowerLifecycleRequest::try_from(
            proto::ReportOracleFollowerLifecycleRequest::from(follower.clone()),
        )
        .expect("private follower report round-trips");
        assert_eq!(actual, follower);
        let follower_response = domain::ReportOracleFollowerLifecycleResponse {
            completed_participants: 1,
        };
        assert_eq!(
            domain::ReportOracleFollowerLifecycleResponse::from(
                proto::ReportOracleFollowerLifecycleResponse::from(follower_response)
            ),
            follower_response
        );

        let private_cancel = domain::CancelOracleLifecycleRequest {
            tenant_id,
            request_id,
            query_id,
            cut_fingerprint: "sha256:cut".to_owned(),
        };
        let actual = domain::CancelOracleLifecycleRequest::try_from(
            proto::CancelOracleLifecycleRequest::from(private_cancel.clone()),
        )
        .expect("private cancellation round-trips");
        assert_eq!(actual, private_cancel);
        let private_cancel_response = domain::CancelOracleLifecycleResponse {
            cancellation_started: true,
        };
        assert_eq!(
            domain::CancelOracleLifecycleResponse::from(
                proto::CancelOracleLifecycleResponse::from(private_cancel_response)
            ),
            private_cancel_response
        );
    }
}
