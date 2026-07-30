//! Validated conversions between protobuf and pure Bifrost query contracts.

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
}
