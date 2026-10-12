//! Registry resolution of the Cards an unbound writer attributes evidence to.
//!
//! A Card-bound principal attributes only within its signed scope, which Scribe
//! checks without IO. A principal bound to no Card may attribute to any
//! registered observation-target Card in its tenant, so Gate looks the
//! distinct `card_uid` values a frame names up in the tenant registry before
//! dispatch and hands Scribe the registered members as that frame's scope.

use std::collections::HashSet;
use std::io::Cursor;
use std::str::FromStr;

use arrow::array::{Array, StringArray};
use arrow::ipc::reader::StreamReader;
use wyrd_runtime::Principal;
use wyrd_runtime::principal::CardAttribution;
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;
use wyrd_spec::vala::CARD_UID;
use wyrd_sql::WyrdPostgres;
use wyrd_sql::queries::cards::get_card_by_uid;
use wyrd_tonic::otlp::common::v1::KeyValue;
use wyrd_tonic::otlp::logs_service::ExportLogsServiceRequest;
use wyrd_tonic::otlp::metrics::v1::{Metric, metric};
use wyrd_tonic::otlp::metrics_service::ExportMetricsServiceRequest;
use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;

use super::auth::AuthContext;
use super::error::IngestError;
use crate::tables::signal::card_uid_text;

/// Most distinct Cards one frame may attribute to through registry
/// resolution.
///
/// It matches the member cap of a signed Card scope, so an unbound writer can
/// name no more Cards per frame than a Card-bound one, and bounds the registry
/// reads one frame can cause.
pub const MAX_ATTRIBUTED_CARDS: usize = 32;

/// Tenant registry reader resolving unbound writers' Card attribution.
///
/// Gate holds one per process; it is absent where no registry is composed,
/// in which case an unbound writer's `card_uid` resolves to nothing and is
/// refused exactly as an unauthorized reference.
#[derive(Clone)]
pub struct CardRegistry {
    /// The Wyrd registry database every lookup reads tenant-scoped.
    postgres: WyrdPostgres,
}

impl CardRegistry {
    /// Builds the reader over the Wyrd registry database.
    #[must_use]
    pub const fn new(postgres: WyrdPostgres) -> Self {
        Self { postgres }
    }

    /// Resolves the Cards `auth` attributes a frame to, when it is unbound.
    ///
    /// `cards` collects the frame's distinct Card UIDs and runs only for an
    /// unbound principal. Returns `None` for a Card-bound principal, whose
    /// signed scope governs, and when the frame names no Card. Otherwise it
    /// returns the UIDs that name a registered observation-target Card in the
    /// caller's tenant; an unregistered UID or a Card of a non-observation
    /// kind is left out, so the downstream check refuses it exactly like a
    /// UID outside a signed scope.
    ///
    /// # Errors
    ///
    /// Returns [`IngestError::RequestValidation`] when the frame names more
    /// than [`MAX_ATTRIBUTED_CARDS`] distinct Cards, and
    /// [`IngestError::Internal`] when the tenant registry cannot be read. No
    /// partial resolution is returned.
    pub async fn attributed_cards(
        &self,
        auth: &AuthContext,
        cards: impl FnOnce() -> Vec<CardUid>,
    ) -> Result<Option<Vec<CardUid>>, IngestError> {
        if !is_unbound(&auth.principal) {
            return Ok(None);
        }
        let cards = cards();
        if cards.is_empty() {
            return Ok(None);
        }
        if cards.len() > MAX_ATTRIBUTED_CARDS {
            return Err(IngestError::RequestValidation(format!(
                "a frame may attribute at most {MAX_ATTRIBUTED_CARDS} distinct Cards"
            )));
        }
        Ok(Some(self.observation_targets(auth.tenant, cards).await?))
    }

    /// Reads each Card by UID on one tenant-scoped connection, keeping the
    /// UIDs of observation targets.
    ///
    /// # Errors
    ///
    /// Returns [`IngestError::Internal`] when the connection or a lookup fails
    /// for any reason other than the Card being absent.
    async fn observation_targets(
        &self,
        tenant: DataTenantId,
        cards: Vec<CardUid>,
    ) -> Result<Vec<CardUid>, IngestError> {
        let mut conn = self
            .postgres
            .tenant_conn(tenant)
            .await
            .map_err(|error| registry_unavailable(&error))?;
        let mut members = Vec::with_capacity(cards.len());
        for uid in cards {
            match get_card_by_uid(&mut conn, &uid).await {
                Ok(row) if row.kind.is_observation_target() => members.push(uid),
                Ok(_) | Err(WyrdError::RegistryCardNotFound { .. }) => {}
                Err(error) => return Err(registry_unavailable(&error)),
            }
        }
        Ok(members)
    }
}

/// Whether `principal` attributes through registry resolution rather than a
/// signed scope.
fn is_unbound(principal: &Principal) -> bool {
    principal.card_attribution() == CardAttribution::AnyRegistered
}

/// Collects the distinct parseable UIDs of a native frame's `card_uid`
/// column.
///
/// Only the correlation column is projected out of the stream. A frame that
/// does not decode, lacks the column, or carries non-UTF-8 values yields no
/// UIDs here; Scribe's full decode refuses it on its own terms.
#[must_use]
pub fn native_card_uids(ipc: &[u8]) -> Vec<CardUid> {
    let Ok(reader) = StreamReader::try_new(Cursor::new(ipc), None) else {
        return Vec::new();
    };
    let Ok(column) = reader.schema().index_of(CARD_UID) else {
        return Vec::new();
    };
    let Ok(reader) = StreamReader::try_new(Cursor::new(ipc), Some(vec![column])) else {
        return Vec::new();
    };
    let mut texts = HashSet::new();
    for batch in reader.flatten() {
        let Some(values) = batch.column(0).as_any().downcast_ref::<StringArray>() else {
            return Vec::new();
        };
        texts.extend(values.iter().flatten().map(str::to_owned));
    }
    parse_distinct(texts.iter().map(String::as_str))
}

/// Collects the distinct `wyrd.card_uid` values of a trace export's spans.
#[must_use]
pub fn span_card_uids(request: &ExportTraceServiceRequest) -> Vec<CardUid> {
    parse_distinct(
        request
            .resource_spans
            .iter()
            .flat_map(|resource| &resource.scope_spans)
            .flat_map(|scope| &scope.spans)
            .filter_map(|span| card_uid_text(&span.attributes)),
    )
}

/// Collects the distinct `wyrd.card_uid` values of a log export's records.
#[must_use]
pub fn log_card_uids(request: &ExportLogsServiceRequest) -> Vec<CardUid> {
    parse_distinct(
        request
            .resource_logs
            .iter()
            .flat_map(|resource| &resource.scope_logs)
            .flat_map(|scope| &scope.log_records)
            .filter_map(|record| card_uid_text(&record.attributes)),
    )
}

/// Collects the distinct `wyrd.card_uid` values of a metrics export's data
/// points.
#[must_use]
pub fn metric_card_uids(request: &ExportMetricsServiceRequest) -> Vec<CardUid> {
    parse_distinct(
        request
            .resource_metrics
            .iter()
            .flat_map(|resource| &resource.scope_metrics)
            .flat_map(|scope| &scope.metrics)
            .flat_map(point_card_uids),
    )
}

/// The `wyrd.card_uid` text of each data point of one metric.
fn point_card_uids(metric: &Metric) -> Vec<&str> {
    let attributes: Vec<&[KeyValue]> = match &metric.data {
        Some(metric::Data::Gauge(gauge)) => gauge
            .data_points
            .iter()
            .map(|p| &p.attributes[..])
            .collect(),
        Some(metric::Data::Sum(sum)) => sum.data_points.iter().map(|p| &p.attributes[..]).collect(),
        Some(metric::Data::Histogram(histogram)) => histogram
            .data_points
            .iter()
            .map(|p| &p.attributes[..])
            .collect(),
        Some(metric::Data::ExponentialHistogram(histogram)) => histogram
            .data_points
            .iter()
            .map(|p| &p.attributes[..])
            .collect(),
        Some(metric::Data::Summary(summary)) => summary
            .data_points
            .iter()
            .map(|p| &p.attributes[..])
            .collect(),
        None => Vec::new(),
    };
    attributes.into_iter().filter_map(card_uid_text).collect()
}

/// Parses distinct UID texts, dropping any that do not parse.
///
/// A malformed UID resolves to nothing and is refused downstream.
pub fn parse_distinct<'a>(texts: impl IntoIterator<Item = &'a str>) -> Vec<CardUid> {
    let mut seen = HashSet::new();
    texts
        .into_iter()
        .filter(|text| seen.insert(*text))
        .filter_map(|text| CardUid::from_str(text).ok())
        .collect()
}

/// Maps a registry failure to an internal ingest error without leaking SQL.
fn registry_unavailable(error: &dyn std::fmt::Display) -> IngestError {
    tracing::error!(%error, "card registry unavailable for ingest attribution");
    IngestError::Internal("card registry unavailable".to_owned())
}

#[cfg(test)]
mod tests {
    //! Native `card_uid` collection.

    use std::sync::Arc;

    use arrow::array::{ArrayRef, Int64Array};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::ipc::writer::StreamWriter;
    use arrow::record_batch::RecordBatch;

    use super::*;

    /// One IPC stream with a `value` column and, optionally, `card_uid`.
    ///
    /// # Panics
    /// Panics when the static batch cannot be encoded.
    fn frame(cards: Option<Vec<Option<&str>>>) -> Vec<u8> {
        let mut fields = vec![Field::new("value", DataType::Int64, false)];
        let rows = cards.as_ref().map_or(1, Vec::len);
        let mut columns: Vec<ArrayRef> = vec![Arc::new(Int64Array::from(vec![1; rows]))];
        if let Some(cards) = cards {
            fields.push(Field::new(CARD_UID, DataType::Utf8, true));
            columns.push(Arc::new(StringArray::from(cards)));
        }
        let batch = RecordBatch::try_new(Arc::new(Schema::new(fields)), columns)
            .expect("static batch is valid");
        let mut bytes = Vec::new();
        let mut writer = StreamWriter::try_new(&mut bytes, &batch.schema()).expect("writer opens");
        writer.write(&batch).expect("batch writes");
        writer.finish().expect("stream finishes");
        drop(writer);
        bytes
    }

    /// Distinct parseable UIDs are collected once each; nulls, malformed
    /// text, and a frame without the column contribute nothing.
    ///
    /// # Panics
    /// Panics when the collected references differ from the expectation.
    #[test]
    fn native_card_uids_are_distinct_and_parseable() {
        let model = "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b01";
        let collected = native_card_uids(&frame(Some(vec![
            Some(model),
            None,
            Some(model),
            Some("not a card"),
        ])));

        assert_eq!(collected, vec![CardUid::from_str(model).expect("parses")]);
        assert!(native_card_uids(&frame(None)).is_empty());
        assert!(native_card_uids(b"not arrow").is_empty());
    }
}
