//! Registry resolution of the Cards an unbound writer attributes evidence to.
//!
//! A Card-bound principal attributes only within its signed scope, which Scribe
//! checks without IO. A principal bound to no Card may attribute to any
//! registered observation-target Card in its tenant, so Gate resolves the
//! distinct `card_ref` values a frame names against the tenant registry before
//! dispatch and hands Scribe the resolved members as that frame's scope.

use std::collections::HashSet;
use std::io::Cursor;
use std::str::FromStr;

use arrow::array::{Array, StringArray};
use arrow::ipc::reader::StreamReader;
use wyrd_runtime::Principal;
use wyrd_runtime::principal::CardAttribution;
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::{CardRef, CardRefScope};
use wyrd_spec::vala::CARD_REF;
use wyrd_sql::WyrdPostgres;
use wyrd_sql::queries::cards::get_card_by_ref;
use wyrd_tonic::otlp::common::v1::KeyValue;
use wyrd_tonic::otlp::logs_service::ExportLogsServiceRequest;
use wyrd_tonic::otlp::metrics::v1::{Metric, metric};
use wyrd_tonic::otlp::metrics_service::ExportMetricsServiceRequest;
use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;

use super::auth::AuthContext;
use super::error::IngestError;
use crate::tables::signal::card_ref_text;

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
/// in which case an unbound writer's `card_ref` resolves to nothing and is
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
    /// `cards` collects the frame's distinct references and runs only for an
    /// unbound principal. Returns `None` for a Card-bound principal, whose
    /// signed scope governs, and when the frame names no Card. Otherwise every reference that names a
    /// registered observation-target Card in the caller's tenant becomes one
    /// scope member carrying its registry UID; an unregistered Card or one of
    /// a non-observation kind is left out, so the downstream scope check
    /// refuses it exactly like a reference outside a signed scope.
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
        cards: impl FnOnce() -> Vec<CardRef>,
    ) -> Result<Option<CardRefScope>, IngestError> {
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
        let members = self.resolve(auth.tenant, cards).await?;
        Ok(Some(match members.split_first() {
            Some((root, rest)) => CardRefScope::from_root_and_members(root, rest.to_vec()),
            None => CardRefScope::default(),
        }))
    }

    /// Reads each Card in one tenant-scoped transaction, keeping the
    /// observation targets with their registry UIDs.
    ///
    /// # Errors
    ///
    /// Returns [`IngestError::Internal`] when the connection or a lookup fails
    /// for any reason other than the Card being absent.
    async fn resolve(
        &self,
        tenant: DataTenantId,
        cards: Vec<CardRef>,
    ) -> Result<Vec<CardRef>, IngestError> {
        let mut conn = self
            .postgres
            .tenant_conn(tenant)
            .await
            .map_err(|error| registry_unavailable(&error))?;
        let mut members = Vec::with_capacity(cards.len());
        for card in cards {
            if !card.kind.is_observation_target() {
                continue;
            }
            let Some(space) = card.space.as_ref() else {
                continue;
            };
            match get_card_by_ref(
                &mut conn,
                card.kind.clone(),
                space,
                &card.name,
                &card.version,
            )
            .await
            {
                Ok(row) => members.push(CardRef {
                    uid: Some(row.card_uid),
                    ..card
                }),
                Err(WyrdError::RegistryCardNotFound { .. }) => {}
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

/// Collects the distinct parseable references of a native frame's `card_ref`
/// column.
///
/// Only the correlation column is projected out of the stream. A frame that
/// does not decode, lacks the column, or carries non-UTF-8 values yields no
/// references here; Scribe's full decode refuses it on its own terms.
#[must_use]
pub fn native_card_refs(ipc: &[u8]) -> Vec<CardRef> {
    let Ok(reader) = StreamReader::try_new(Cursor::new(ipc), None) else {
        return Vec::new();
    };
    let Ok(column) = reader.schema().index_of(CARD_REF) else {
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

/// Collects the distinct `wyrd.card_ref` values of a trace export's spans.
#[must_use]
pub fn span_card_refs(request: &ExportTraceServiceRequest) -> Vec<CardRef> {
    parse_distinct(
        request
            .resource_spans
            .iter()
            .flat_map(|resource| &resource.scope_spans)
            .flat_map(|scope| &scope.spans)
            .filter_map(|span| card_ref_text(&span.attributes)),
    )
}

/// Collects the distinct `wyrd.card_ref` values of a log export's records.
#[must_use]
pub fn log_card_refs(request: &ExportLogsServiceRequest) -> Vec<CardRef> {
    parse_distinct(
        request
            .resource_logs
            .iter()
            .flat_map(|resource| &resource.scope_logs)
            .flat_map(|scope| &scope.log_records)
            .filter_map(|record| card_ref_text(&record.attributes)),
    )
}

/// Collects the distinct `wyrd.card_ref` values of a metrics export's data
/// points.
#[must_use]
pub fn metric_card_refs(request: &ExportMetricsServiceRequest) -> Vec<CardRef> {
    parse_distinct(
        request
            .resource_metrics
            .iter()
            .flat_map(|resource| &resource.scope_metrics)
            .flat_map(|scope| &scope.metrics)
            .flat_map(point_card_refs),
    )
}

/// The `wyrd.card_ref` text of each data point of one metric.
fn point_card_refs(metric: &Metric) -> Vec<&str> {
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
    attributes.into_iter().filter_map(card_ref_text).collect()
}

/// Parses distinct reference texts, dropping any that do not parse.
///
/// A malformed reference resolves to nothing and is refused downstream.
pub fn parse_distinct<'a>(texts: impl IntoIterator<Item = &'a str>) -> Vec<CardRef> {
    let mut seen = HashSet::new();
    texts
        .into_iter()
        .filter(|text| seen.insert(*text))
        .filter_map(|text| CardRef::from_str(text).ok())
        .collect()
}

/// Maps a registry failure to an internal ingest error without leaking SQL.
fn registry_unavailable(error: &dyn std::fmt::Display) -> IngestError {
    tracing::error!(%error, "card registry unavailable for ingest attribution");
    IngestError::Internal("card registry unavailable".to_owned())
}

#[cfg(test)]
mod tests {
    //! Native `card_ref` collection.

    use std::sync::Arc;

    use arrow::array::{ArrayRef, Int64Array};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::ipc::writer::StreamWriter;
    use arrow::record_batch::RecordBatch;

    use super::*;

    /// One IPC stream with a `value` column and, optionally, `card_ref`.
    ///
    /// # Panics
    /// Panics when the static batch cannot be encoded.
    fn frame(cards: Option<Vec<Option<&str>>>) -> Vec<u8> {
        let mut fields = vec![Field::new("value", DataType::Int64, false)];
        let rows = cards.as_ref().map_or(1, Vec::len);
        let mut columns: Vec<ArrayRef> = vec![Arc::new(Int64Array::from(vec![1; rows]))];
        if let Some(cards) = cards {
            fields.push(Field::new(CARD_REF, DataType::Utf8, true));
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

    /// Distinct parseable references are collected once each; nulls,
    /// malformed text, and a frame without the column contribute nothing.
    ///
    /// # Panics
    /// Panics when the collected references differ from the expectation.
    #[test]
    fn native_card_refs_are_distinct_and_parseable() {
        let model = "default/Model/observed-model@1.0.0";
        let collected = native_card_refs(&frame(Some(vec![
            Some(model),
            None,
            Some(model),
            Some("not a card"),
        ])));

        assert_eq!(collected, vec![CardRef::from_str(model).expect("parses")]);
        assert!(native_card_refs(&frame(None)).is_empty());
        assert!(native_card_refs(b"not arrow").is_empty());
    }
}
