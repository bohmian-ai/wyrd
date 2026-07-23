//! Cross-table domain derivations: pure, IO-free transforms that turn
//! one source table's committed rows into one or more target tables' rows.
//!
//! A [`DomainDerivation`] is the pure kernel of the derivation runtime. It reads
//! a source [`RecordBatch`] (already read back from the source Iceberg table by
//! the runtime) and emits a [`DerivedBatch`] per target-table / tenant partition.
//! It performs no IO, holds no async, and touches no catalog — the runtime owns
//! reading the source delta, leasing, writing each target through the group-commit
//! coordinator, and advancing the watermark.
//!
//! The only derivation shipped today is [`GenAiFromSpans`]: it projects the
//! `gen_ai.*` OpenTelemetry span attributes stored on `traces.spans` into the
//! three `genai.*` fact tables (`messages`, `embeddings`, `tool_calls`).
//!
//! ## Exactly-once
//!
//! Each emitted [`DerivedBatch`] carries the `source_wyrd_batch_id` it was derived
//! from and the target-table identity. The runtime keys the target append on a
//! deterministic `derived_batch_id = uuidv5(DERIVATION_UID, target_uid, source_uid,
//! source_wyrd_batch_id)`. Re-deriving the same source batch produces the SAME
//! `derived_batch_id`, so the group-commit coordinator's `vala.olap_commits` dedup
//! lands it as a replay no-op. This module owns the deterministic key derivation
//! ([`DomainDerivation::derived_batch_id`]); the coordinator owns the dedup.

use std::collections::BTreeMap;
use std::sync::Arc;

use arrow::array::{
    Array, BooleanArray, FixedSizeBinaryArray, FixedSizeBinaryBuilder, Float64Array, Int64Array,
    RecordBatch, StringArray, StringViewArray, TimestampMicrosecondArray,
};
use arrow::datatypes::{Schema, SchemaRef};
use uuid::Uuid;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::vala::system_columns::DATA_TENANT_ID;

use crate::tables::TableError;
use crate::tables::genai::{EmbeddingsTable, MemoryTable, MessagesTable, ToolCallsTable};
use crate::tables::traces::SpansTable;
use crate::tables::{DomainTable, fields};

/// One target-table row set produced by a derivation, tagged with the tenant it
/// belongs to and the source commit it was derived from.
///
/// The runtime writes this through the target table's group-commit coordinator
/// bound to `data_tenant_id`, keyed on the `derived_batch_id` the derivation
/// computes from `source_wyrd_batch_id`.
#[derive(Debug, Clone)]
pub struct DerivedBatch {
    /// Target table namespace (e.g. `"genai"`).
    pub target_namespace: &'static str,
    /// Target table name (e.g. `"messages"`).
    pub target_name: &'static str,
    /// The data tenant every row in `batch` belongs to. All rows in one
    /// `DerivedBatch` share a tenant so the target coordinator can stamp a single
    /// `data_tenant_id` and never cross tenants.
    pub data_tenant_id: DataTenantId,
    /// The source `wyrd_batch_id` these rows were derived from — the exactly-once
    /// discriminator folded into `derived_batch_id`.
    pub source_wyrd_batch_id: [u8; 16],
    /// The target rows (user + nullable correlation columns; system columns are
    /// stamped by the coordinator).
    pub batch: RecordBatch,
}

/// A pure, IO-free transform from one source domain table to one or more target
/// domain tables.
///
/// Implementors declare the source/target identity and a stable `DERIVATION_UID`
/// (the uuidv5 namespace seed that makes `derived_batch_id` deterministic), then
/// implement [`derive`](DomainDerivation::derive) as a pure function over a source
/// [`RecordBatch`].
pub trait DomainDerivation: Send + Sync + 'static {
    /// Source table namespace.
    const SOURCE_NAMESPACE: &'static str;
    /// Source table name.
    const SOURCE_NAME: &'static str;
    /// Stable derivation identity seed. Folded into every `derived_batch_id` so
    /// the exactly-once key is namespaced to THIS derivation and cannot collide
    /// with an unrelated derivation that happens to read the same source batch.
    const DERIVATION_UID: Uuid;

    /// Transform one source `RecordBatch` into per-target, per-tenant derived
    /// batches.
    ///
    /// PURE and IO-free. `source_wyrd_batch_id` is the commit the batch was read
    /// from; it is stamped onto every produced [`DerivedBatch`] so the runtime can
    /// compute the exactly-once key.
    ///
    /// # Errors
    /// Returns [`TableError`] when the source batch is missing a required column
    /// or has an unexpected physical type.
    fn derive(
        &self,
        source: &RecordBatch,
        source_wyrd_batch_id: [u8; 16],
    ) -> Result<Vec<DerivedBatch>, TableError>;

    /// Deterministic exactly-once key for one derived append:
    /// `uuidv5(DERIVATION_UID, target_uid || source_uid || source_wyrd_batch_id)`.
    ///
    /// `target_uid` / `source_uid` are the tables' opaque 16-byte identities. Two
    /// re-derivations of the same source batch into the same target produce the
    /// SAME key, so the coordinator dedups the second as a replay.
    #[must_use]
    fn derived_batch_id(
        target_uid: &[u8; 16],
        source_uid: &[u8; 16],
        source_wyrd_batch_id: &[u8; 16],
    ) -> [u8; 16] {
        let mut name = Vec::with_capacity(48);
        name.extend_from_slice(target_uid);
        name.extend_from_slice(source_uid);
        name.extend_from_slice(source_wyrd_batch_id);
        *Uuid::new_v5(&Self::DERIVATION_UID, &name).as_bytes()
    }
}

/// The `traces.spans` → `genai.{messages,embeddings,tool_calls}` derivation.
///
/// Reads the `gen_ai.*` OpenTelemetry semantic-convention attributes off each
/// span's `attributes` JSON blob and routes the span to a target by
/// `gen_ai.operation.name`: an embedding operation lands in `genai.embeddings`,
/// an `execute_tool` operation in `genai.tool_calls`, everything else (chat,
/// `generate_content`, …) in `genai.messages`.
pub struct GenAiFromSpans;

impl GenAiFromSpans {
    /// The uuidv5 namespace seed for this derivation. A fixed, arbitrary UUID —
    /// never regenerate it; changing it would re-key every derived append and
    /// break exactly-once against already-committed derived rows.
    const UID: Uuid = Uuid::from_bytes([
        0x9a, 0x1d, 0x77, 0x0c, 0x2b, 0x84, 0x5e, 0x63, 0xb1, 0x0f, 0x3c, 0x21, 0x8d, 0x4e, 0x67,
        0xf2,
    ]);

    /// Semantic version of this derivation's transform. Hashed into the
    /// `transform_fingerprint` stored on `vala.olap_derivations`. Bump this
    /// whenever the mapping semantics, output schema, or target routing change
    /// — any redeploy under a changed constant is rejected as `DriftRejected`
    /// so the runtime cannot silently resume the old watermark under new
    /// semantics.
    pub const CONTRACT_VERSION: &'static str = "genai_from_spans/v1";
}

impl DomainDerivation for GenAiFromSpans {
    const SOURCE_NAMESPACE: &'static str = SpansTable::NAMESPACE;
    const SOURCE_NAME: &'static str = SpansTable::NAME;
    const DERIVATION_UID: Uuid = Self::UID;

    fn derive(
        &self,
        source: &RecordBatch,
        source_wyrd_batch_id: [u8; 16],
    ) -> Result<Vec<DerivedBatch>, TableError> {
        let cols = SpanColumns::from_batch(source)?;
        let nrows = source.num_rows();

        // Partition target rows by (target kind, tenant). Each partition becomes
        // one DerivedBatch bound to that tenant.
        let mut messages: BTreeMap<DataTenantId, Vec<MessageRow>> = BTreeMap::new();
        let mut embeddings: BTreeMap<DataTenantId, Vec<EmbeddingRow>> = BTreeMap::new();
        let mut tool_calls: BTreeMap<DataTenantId, Vec<ToolCallRow>> = BTreeMap::new();
        let mut memory: BTreeMap<DataTenantId, Vec<MemoryRow>> = BTreeMap::new();

        for i in 0..nrows {
            // A row without gen_ai attributes is not a GenAI span; skip it.
            let Some(attrs) = cols.attributes(i) else {
                continue;
            };
            if !attrs.keys().any(|k| k.starts_with("gen_ai.")) {
                continue;
            }
            let tenant = cols.tenant(i)?;
            let base = cols.base(i);
            let operation = attrs
                .get("gen_ai.operation.name")
                .and_then(|v| v.as_str())
                .unwrap_or_default();

            match classify(operation) {
                GenAiKind::Embeddings => {
                    embeddings
                        .entry(tenant)
                        .or_default()
                        .push(EmbeddingRow::from_span(&base, &attrs));
                }
                GenAiKind::ToolCall => {
                    tool_calls
                        .entry(tenant)
                        .or_default()
                        .push(ToolCallRow::from_span(&base, &attrs));
                }
                GenAiKind::Memory => {
                    memory
                        .entry(tenant)
                        .or_default()
                        .push(MemoryRow::from_span(&base, &attrs));
                }
                GenAiKind::Message => {
                    messages
                        .entry(tenant)
                        .or_default()
                        .push(MessageRow::from_span(&base, &attrs));
                }
            }
        }

        let mut out = Vec::new();
        for (tenant, rows) in messages {
            out.push(DerivedBatch {
                target_namespace: MessagesTable::NAMESPACE,
                target_name: MessagesTable::NAME,
                data_tenant_id: tenant,
                source_wyrd_batch_id,
                batch: MessageRow::to_batch(&rows)?,
            });
        }
        for (tenant, rows) in embeddings {
            out.push(DerivedBatch {
                target_namespace: EmbeddingsTable::NAMESPACE,
                target_name: EmbeddingsTable::NAME,
                data_tenant_id: tenant,
                source_wyrd_batch_id,
                batch: EmbeddingRow::to_batch(&rows)?,
            });
        }
        for (tenant, rows) in tool_calls {
            out.push(DerivedBatch {
                target_namespace: ToolCallsTable::NAMESPACE,
                target_name: ToolCallsTable::NAME,
                data_tenant_id: tenant,
                source_wyrd_batch_id,
                batch: ToolCallRow::to_batch(&rows)?,
            });
        }
        for (tenant, rows) in memory {
            out.push(DerivedBatch {
                target_namespace: MemoryTable::NAMESPACE,
                target_name: MemoryTable::NAME,
                data_tenant_id: tenant,
                source_wyrd_batch_id,
                batch: MemoryRow::to_batch(&rows)?,
            });
        }
        Ok(out)
    }
}

/// Which target the span routes to, decided by `gen_ai.operation.name`.
enum GenAiKind {
    Message,
    Embeddings,
    ToolCall,
    Memory,
}

fn classify(operation: &str) -> GenAiKind {
    match operation {
        "embeddings" | "retrieval" => GenAiKind::Embeddings,
        "execute_tool" => GenAiKind::ToolCall,
        "search_memory"
        | "create_memory"
        | "update_memory"
        | "upsert_memory"
        | "delete_memory"
        | "create_memory_store"
        | "delete_memory_store" => GenAiKind::Memory,
        _ => GenAiKind::Message,
    }
}

/// Attribute-column reader that tolerates both `Utf8` and `Utf8View`.
///
/// The `attributes` column is declared `Utf8View` in the table schema
/// (`SpansTable`), but Parquet has no `Utf8View` physical type — round-tripping
/// through Iceberg storage surfaces it as `Utf8` on read. Support both so the
/// derivation runs on freshly-committed-and-scanned data.
enum AttributesArray<'a> {
    View(&'a StringViewArray),
    Utf8(&'a StringArray),
}

impl AttributesArray<'_> {
    fn is_null(&self, i: usize) -> bool {
        match self {
            AttributesArray::View(a) => a.is_null(i),
            AttributesArray::Utf8(a) => a.is_null(i),
        }
    }

    fn value(&self, i: usize) -> &str {
        match self {
            AttributesArray::View(a) => a.value(i),
            AttributesArray::Utf8(a) => a.value(i),
        }
    }
}

/// The physical span columns the derivation reads. Downcast once per batch.
struct SpanColumns<'a> {
    trace_id: &'a FixedSizeBinaryArray,
    span_id: &'a FixedSizeBinaryArray,
    parent_span_id: Option<&'a FixedSizeBinaryArray>,
    start_time: &'a TimestampMicrosecondArray,
    end_time: &'a TimestampMicrosecondArray,
    duration_ms: &'a Int64Array,
    status: &'a StringArray,
    service_name: &'a StringArray,
    attributes: Option<AttributesArray<'a>>,
    data_tenant_id: &'a StringArray,
}

impl<'a> SpanColumns<'a> {
    fn from_batch(batch: &'a RecordBatch) -> Result<Self, TableError> {
        Ok(Self {
            trace_id: fixed_bin(batch, "trace_id")?,
            span_id: fixed_bin(batch, "span_id")?,
            parent_span_id: fixed_bin_opt(batch, "parent_span_id"),
            start_time: ts_us(batch, "start_time")?,
            end_time: ts_us(batch, "end_time")?,
            duration_ms: int64(batch, "duration_ms")?,
            status: string(batch, "status")?,
            service_name: string(batch, "service_name")?,
            attributes: attributes_col(batch),
            data_tenant_id: string(batch, DATA_TENANT_ID)?,
        })
    }

    fn tenant(&self, i: usize) -> Result<DataTenantId, TableError> {
        let raw = self.data_tenant_id.value(i);
        let uuid = Uuid::parse_str(raw)
            .map_err(|e| TableError::Internal(format!("spans.{DATA_TENANT_ID} not a uuid: {e}")))?;
        DataTenantId::try_from(uuid)
            .map_err(|e| TableError::Internal(format!("spans.{DATA_TENANT_ID} invalid: {e}")))
    }

    fn attributes(&self, i: usize) -> Option<serde_json::Map<String, serde_json::Value>> {
        let arr = self.attributes.as_ref()?;
        if arr.is_null(i) {
            return None;
        }
        match serde_json::from_str::<serde_json::Value>(arr.value(i)) {
            Ok(serde_json::Value::Object(map)) => Some(map),
            _ => None,
        }
    }

    fn base(&self, i: usize) -> SpanBase {
        SpanBase {
            trace_id: {
                let mut b = [0u8; 16];
                b.copy_from_slice(self.trace_id.value(i));
                b
            },
            span_id: {
                let mut b = [0u8; 8];
                b.copy_from_slice(self.span_id.value(i));
                b
            },
            parent_span_id: self.parent_span_id.and_then(|a| {
                if a.is_null(i) {
                    None
                } else {
                    let mut b = [0u8; 8];
                    b.copy_from_slice(a.value(i));
                    Some(b)
                }
            }),
            start_time_us: self.start_time.value(i),
            end_time_us: self.end_time.value(i),
            duration_ms: self.duration_ms.value(i),
            status: self.status.value(i).to_owned(),
            service_name: self.service_name.value(i).to_owned(),
        }
    }
}

/// The non-gen_ai span identity/timing columns shared by every target row.
struct SpanBase {
    trace_id: [u8; 16],
    span_id: [u8; 8],
    parent_span_id: Option<[u8; 8]>,
    start_time_us: i64,
    end_time_us: i64,
    duration_ms: i64,
    status: String,
    service_name: String,
}

type Attrs = serde_json::Map<String, serde_json::Value>;

fn attr_str(attrs: &Attrs, key: &str) -> Option<String> {
    attrs.get(key).and_then(|v| v.as_str()).map(str::to_owned)
}

fn attr_i64(attrs: &Attrs, key: &str) -> Option<i64> {
    attrs.get(key).and_then(serde_json::Value::as_i64)
}

fn attr_bool(attrs: &Attrs, key: &str) -> Option<bool> {
    attrs.get(key).and_then(serde_json::Value::as_bool)
}

fn attr_f64(attrs: &Attrs, key: &str) -> Option<f64> {
    attrs.get(key).and_then(serde_json::Value::as_f64)
}

fn attr_json(attrs: &Attrs, key: &str) -> Option<String> {
    attrs.get(key).map(|v| match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    })
}

/// A projected `genai.messages` row.
///
/// Covers every column in `MessagesTable` that the OTLP projector can fill
/// from `gen_ai.*` / `openai.*` attributes. Columns not populated here are
/// all nullable; the coordinator fills them with NULL via name-based alignment.
struct MessageRow {
    base: SpanBase,
    provider_name: String,
    operation_name: String,
    request_model: String,
    response_model: Option<String>,
    conversation_id: Option<String>,
    response_id: Option<String>,
    response_finish_reasons: Option<String>,
    response_time_to_first_chunk_seconds: Option<f64>,
    request_temperature: Option<f64>,
    request_top_p: Option<f64>,
    request_top_k: Option<i64>,
    request_max_tokens: Option<i64>,
    request_frequency_penalty: Option<f64>,
    request_presence_penalty: Option<f64>,
    request_seed: Option<i64>,
    request_choice_count: Option<i64>,
    request_stop_sequences: Option<String>,
    request_stream: Option<bool>,
    request_encoding_formats: Option<String>,
    usage_input_tokens: Option<i64>,
    usage_output_tokens: Option<i64>,
    usage_cache_creation_input_tokens: Option<i64>,
    usage_cache_read_input_tokens: Option<i64>,
    usage_reasoning_output_tokens: Option<i64>,
    output_type: Option<String>,
    request_reasoning_level: Option<String>,
    conversation_compacted: Option<bool>,
    input_messages: Option<String>,
    output_messages: Option<String>,
    system_instructions: Option<String>,
    openai_api_type: Option<String>,
    openai_request_service_tier: Option<String>,
    openai_response_service_tier: Option<String>,
    openai_response_system_fingerprint: Option<String>,
    error_type: Option<String>,
}

impl MessageRow {
    fn from_span(base: &SpanBase, attrs: &Attrs) -> Self {
        Self {
            base: base.clone_shallow(),
            provider_name: attr_str(attrs, "gen_ai.provider.name").unwrap_or_default(),
            operation_name: attr_str(attrs, "gen_ai.operation.name").unwrap_or_default(),
            request_model: attr_str(attrs, "gen_ai.request.model").unwrap_or_default(),
            response_model: attr_str(attrs, "gen_ai.response.model"),
            conversation_id: attr_str(attrs, "gen_ai.conversation.id"),
            response_id: attr_str(attrs, "gen_ai.response.id"),
            response_finish_reasons: attr_json(attrs, "gen_ai.response.finish_reasons"),
            response_time_to_first_chunk_seconds: attr_f64(
                attrs,
                "gen_ai.response.time_to_first_chunk",
            ),
            request_temperature: attr_f64(attrs, "gen_ai.request.temperature"),
            request_top_p: attr_f64(attrs, "gen_ai.request.top_p"),
            request_top_k: attr_i64(attrs, "gen_ai.request.top_k"),
            request_max_tokens: attr_i64(attrs, "gen_ai.request.max_tokens"),
            request_frequency_penalty: attr_f64(attrs, "gen_ai.request.frequency_penalty"),
            request_presence_penalty: attr_f64(attrs, "gen_ai.request.presence_penalty"),
            request_seed: attr_i64(attrs, "gen_ai.request.seed"),
            request_choice_count: attr_i64(attrs, "gen_ai.request.choice_count"),
            request_stop_sequences: attr_json(attrs, "gen_ai.request.stop_sequences"),
            request_stream: attr_bool(attrs, "gen_ai.request.stream"),
            request_encoding_formats: attr_json(attrs, "gen_ai.request.encoding_formats"),
            usage_input_tokens: attr_i64(attrs, "gen_ai.usage.input_tokens"),
            usage_output_tokens: attr_i64(attrs, "gen_ai.usage.output_tokens"),
            usage_cache_creation_input_tokens: attr_i64(
                attrs,
                "gen_ai.usage.cache_creation_input_tokens",
            ),
            usage_cache_read_input_tokens: attr_i64(attrs, "gen_ai.usage.cache_read_input_tokens"),
            usage_reasoning_output_tokens: attr_i64(attrs, "gen_ai.usage.reasoning_output_tokens"),
            output_type: attr_str(attrs, "gen_ai.output.type"),
            request_reasoning_level: attr_str(attrs, "gen_ai.request.reasoning.level"),
            conversation_compacted: attr_bool(attrs, "gen_ai.conversation.compacted"),
            input_messages: attr_json(attrs, "gen_ai.input.messages"),
            output_messages: attr_json(attrs, "gen_ai.output.messages"),
            system_instructions: attr_json(attrs, "gen_ai.system_instructions"),
            openai_api_type: attr_str(attrs, "openai.api.type"),
            openai_request_service_tier: attr_str(attrs, "openai.request.service_tier"),
            openai_response_service_tier: attr_str(attrs, "openai.response.service_tier"),
            openai_response_system_fingerprint: attr_str(
                attrs,
                "openai.response.system_fingerprint",
            ),
            error_type: attr_str(attrs, "error.type"),
        }
    }

    fn schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            fields::fixed_binary("trace_id", 16, false),
            fields::fixed_binary("span_id", 8, false),
            fields::fixed_binary("parent_span_id", 8, true),
            fields::ts_us_utc("start_time", false),
            fields::ts_us_utc("end_time", false),
            fields::int64("duration_ms", false),
            fields::utf8("status", false),
            fields::utf8("service_name", false),
            fields::utf8("provider_name", false),
            fields::utf8("operation_name", false),
            fields::utf8("request_model", false),
            fields::utf8("response_model", true),
            fields::utf8("conversation_id", true),
            fields::utf8("response_id", true),
            fields::utf8_view("response_finish_reasons", true),
            fields::float64("response_time_to_first_chunk_seconds", true),
            fields::float64("request_temperature", true),
            fields::float64("request_top_p", true),
            fields::int64("request_top_k", true),
            fields::int64("request_max_tokens", true),
            fields::float64("request_frequency_penalty", true),
            fields::float64("request_presence_penalty", true),
            fields::int64("request_seed", true),
            fields::int64("request_choice_count", true),
            fields::utf8_view("request_stop_sequences", true),
            fields::boolean("request_stream", true),
            fields::utf8_view("request_encoding_formats", true),
            fields::int64("usage_input_tokens", true),
            fields::int64("usage_output_tokens", true),
            fields::int64("usage_cache_creation_input_tokens", true),
            fields::int64("usage_cache_read_input_tokens", true),
            fields::int64("usage_reasoning_output_tokens", true),
            fields::utf8("output_type", true),
            fields::utf8("request_reasoning_level", true),
            fields::boolean("conversation_compacted", true),
            fields::utf8_view("input_messages", true),
            fields::utf8_view("output_messages", true),
            fields::utf8_view("system_instructions", true),
            fields::utf8("openai_api_type", true),
            fields::utf8("openai_request_service_tier", true),
            fields::utf8("openai_response_service_tier", true),
            fields::utf8("openai_response_system_fingerprint", true),
            fields::utf8("error_type", true),
        ]))
    }

    fn to_batch(rows: &[Self]) -> Result<RecordBatch, TableError> {
        let mut columns = Self::base_columns(rows);
        columns.extend(Self::request_columns(rows));
        columns.extend(Self::usage_columns(rows));
        columns.extend(Self::metadata_columns(rows));

        RecordBatch::try_new(Self::schema(), columns)
            .map_err(|e| TableError::Internal(format!("build genai.messages batch: {e}")))
    }

    fn base_columns(rows: &[Self]) -> Vec<Arc<dyn Array>> {
        let trace_id = fixed_bin16_col(rows.iter().map(|r| r.base.trace_id));
        let span_id = fixed_bin8_col(rows.iter().map(|r| Some(r.base.span_id)));
        let parent_span_id = fixed_bin8_col(rows.iter().map(|r| r.base.parent_span_id));
        let start_time = ts_col(rows.iter().map(|r| r.base.start_time_us));
        let end_time = ts_col(rows.iter().map(|r| r.base.end_time_us));
        let duration_ms = Arc::new(Int64Array::from_iter_values(
            rows.iter().map(|r| r.base.duration_ms),
        )) as Arc<dyn Array>;
        let status = str_col(rows.iter().map(|r| Some(r.base.status.clone())));
        let service_name = str_col(rows.iter().map(|r| Some(r.base.service_name.clone())));
        let provider_name = str_col(rows.iter().map(|r| Some(r.provider_name.clone())));
        let operation_name = str_col(rows.iter().map(|r| Some(r.operation_name.clone())));
        let request_model = str_col(rows.iter().map(|r| Some(r.request_model.clone())));
        let response_model = str_col(rows.iter().map(|r| r.response_model.clone()));
        let conversation_id = str_col(rows.iter().map(|r| r.conversation_id.clone()));
        let response_id = str_col(rows.iter().map(|r| r.response_id.clone()));
        let response_finish_reasons =
            str_view_col(rows.iter().map(|r| r.response_finish_reasons.clone()));

        vec![
            trace_id,
            span_id,
            parent_span_id,
            start_time,
            end_time,
            duration_ms,
            status,
            service_name,
            provider_name,
            operation_name,
            request_model,
            response_model,
            conversation_id,
            response_id,
            response_finish_reasons,
        ]
    }

    fn request_columns(rows: &[Self]) -> Vec<Arc<dyn Array>> {
        let response_time_to_first_chunk_seconds =
            f64_col(rows.iter().map(|r| r.response_time_to_first_chunk_seconds));
        let request_temperature = f64_col(rows.iter().map(|r| r.request_temperature));
        let request_top_p = f64_col(rows.iter().map(|r| r.request_top_p));
        let request_top_k = i64_col(rows.iter().map(|r| r.request_top_k));
        let request_max_tokens = i64_col(rows.iter().map(|r| r.request_max_tokens));
        let request_frequency_penalty = f64_col(rows.iter().map(|r| r.request_frequency_penalty));
        let request_presence_penalty = f64_col(rows.iter().map(|r| r.request_presence_penalty));
        let request_seed = i64_col(rows.iter().map(|r| r.request_seed));
        let request_choice_count = i64_col(rows.iter().map(|r| r.request_choice_count));
        let request_stop_sequences =
            str_view_col(rows.iter().map(|r| r.request_stop_sequences.clone()));
        let request_stream = bool_col(rows.iter().map(|r| r.request_stream));
        let request_encoding_formats =
            str_view_col(rows.iter().map(|r| r.request_encoding_formats.clone()));

        vec![
            response_time_to_first_chunk_seconds,
            request_temperature,
            request_top_p,
            request_top_k,
            request_max_tokens,
            request_frequency_penalty,
            request_presence_penalty,
            request_seed,
            request_choice_count,
            request_stop_sequences,
            request_stream,
            request_encoding_formats,
        ]
    }

    fn usage_columns(rows: &[Self]) -> Vec<Arc<dyn Array>> {
        let usage_input_tokens = i64_col(rows.iter().map(|r| r.usage_input_tokens));
        let usage_output_tokens = i64_col(rows.iter().map(|r| r.usage_output_tokens));
        let usage_cache_creation_input_tokens =
            i64_col(rows.iter().map(|r| r.usage_cache_creation_input_tokens));
        let usage_cache_read_input_tokens =
            i64_col(rows.iter().map(|r| r.usage_cache_read_input_tokens));
        let usage_reasoning_output_tokens =
            i64_col(rows.iter().map(|r| r.usage_reasoning_output_tokens));

        vec![
            usage_input_tokens,
            usage_output_tokens,
            usage_cache_creation_input_tokens,
            usage_cache_read_input_tokens,
            usage_reasoning_output_tokens,
        ]
    }

    fn metadata_columns(rows: &[Self]) -> Vec<Arc<dyn Array>> {
        let output_type = str_col(rows.iter().map(|r| r.output_type.clone()));
        let request_reasoning_level =
            str_col(rows.iter().map(|r| r.request_reasoning_level.clone()));
        let conversation_compacted = bool_col(rows.iter().map(|r| r.conversation_compacted));
        let input_messages = str_view_col(rows.iter().map(|r| r.input_messages.clone()));
        let output_messages = str_view_col(rows.iter().map(|r| r.output_messages.clone()));
        let system_instructions = str_view_col(rows.iter().map(|r| r.system_instructions.clone()));
        let openai_api_type = str_col(rows.iter().map(|r| r.openai_api_type.clone()));
        let openai_request_service_tier =
            str_col(rows.iter().map(|r| r.openai_request_service_tier.clone()));
        let openai_response_service_tier =
            str_col(rows.iter().map(|r| r.openai_response_service_tier.clone()));
        let openai_response_system_fingerprint = str_col(
            rows.iter()
                .map(|r| r.openai_response_system_fingerprint.clone()),
        );
        let error_type = str_col(rows.iter().map(|r| r.error_type.clone()));

        vec![
            output_type,
            request_reasoning_level,
            conversation_compacted,
            input_messages,
            output_messages,
            system_instructions,
            openai_api_type,
            openai_request_service_tier,
            openai_response_service_tier,
            openai_response_system_fingerprint,
            error_type,
        ]
    }
}

/// A projected `genai.embeddings` row.
struct EmbeddingRow {
    base: SpanBase,
    provider_name: String,
    operation_name: String,
    request_model: String,
    response_model: Option<String>,
    embeddings_dimension_count: Option<i64>,
    data_source_id: Option<String>,
    usage_input_tokens: Option<i64>,
    usage_output_tokens: Option<i64>,
    retrieval_top_k: Option<i64>,
    retrieval_query_text: Option<String>,
    error_type: Option<String>,
}

impl EmbeddingRow {
    fn from_span(base: &SpanBase, attrs: &Attrs) -> Self {
        Self {
            base: base.clone_shallow(),
            provider_name: attr_str(attrs, "gen_ai.provider.name").unwrap_or_default(),
            operation_name: attr_str(attrs, "gen_ai.operation.name").unwrap_or_default(),
            request_model: attr_str(attrs, "gen_ai.request.model").unwrap_or_default(),
            response_model: attr_str(attrs, "gen_ai.response.model"),
            embeddings_dimension_count: attr_i64(attrs, "gen_ai.embeddings.dimension.count"),
            data_source_id: attr_str(attrs, "gen_ai.data_source.id"),
            usage_input_tokens: attr_i64(attrs, "gen_ai.usage.input_tokens"),
            usage_output_tokens: attr_i64(attrs, "gen_ai.usage.output_tokens"),
            retrieval_top_k: attr_i64(attrs, "gen_ai.retrieval.top_k"),
            retrieval_query_text: attr_str(attrs, "gen_ai.retrieval.query.text"),
            error_type: attr_str(attrs, "error.type"),
        }
    }

    fn schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            fields::fixed_binary("trace_id", 16, false),
            fields::fixed_binary("span_id", 8, false),
            fields::ts_us_utc("start_time", false),
            fields::ts_us_utc("end_time", false),
            fields::int64("duration_ms", false),
            fields::utf8("status", false),
            fields::utf8("service_name", false),
            fields::utf8("provider_name", false),
            fields::utf8("operation_name", false),
            fields::utf8("request_model", false),
            fields::utf8("response_model", true),
            fields::int64("embeddings_dimension_count", true),
            fields::utf8("data_source_id", true),
            fields::int64("usage_input_tokens", true),
            fields::int64("usage_output_tokens", true),
            fields::int64("retrieval_top_k", true),
            fields::utf8("retrieval_query_text", true),
            fields::utf8("error_type", true),
        ]))
    }

    fn to_batch(rows: &[Self]) -> Result<RecordBatch, TableError> {
        let trace_id = fixed_bin16_col(rows.iter().map(|r| r.base.trace_id));
        let span_id = fixed_bin8_col(rows.iter().map(|r| Some(r.base.span_id)));
        let start_time = ts_col(rows.iter().map(|r| r.base.start_time_us));
        let end_time = ts_col(rows.iter().map(|r| r.base.end_time_us));
        let duration_ms = Arc::new(Int64Array::from_iter_values(
            rows.iter().map(|r| r.base.duration_ms),
        )) as Arc<dyn Array>;
        let status = str_col(rows.iter().map(|r| Some(r.base.status.clone())));
        let service_name = str_col(rows.iter().map(|r| Some(r.base.service_name.clone())));
        let provider_name = str_col(rows.iter().map(|r| Some(r.provider_name.clone())));
        let operation_name = str_col(rows.iter().map(|r| Some(r.operation_name.clone())));
        let request_model = str_col(rows.iter().map(|r| Some(r.request_model.clone())));
        let response_model = str_col(rows.iter().map(|r| r.response_model.clone()));
        let dim = i64_col(rows.iter().map(|r| r.embeddings_dimension_count));
        let data_source_id = str_col(rows.iter().map(|r| r.data_source_id.clone()));
        let usage_input_tokens = i64_col(rows.iter().map(|r| r.usage_input_tokens));
        let usage_output_tokens = i64_col(rows.iter().map(|r| r.usage_output_tokens));
        let retrieval_top_k = i64_col(rows.iter().map(|r| r.retrieval_top_k));
        let retrieval_query_text = str_col(rows.iter().map(|r| r.retrieval_query_text.clone()));
        let error_type = str_col(rows.iter().map(|r| r.error_type.clone()));

        RecordBatch::try_new(
            Self::schema(),
            vec![
                trace_id,
                span_id,
                start_time,
                end_time,
                duration_ms,
                status,
                service_name,
                provider_name,
                operation_name,
                request_model,
                response_model,
                dim,
                data_source_id,
                usage_input_tokens,
                usage_output_tokens,
                retrieval_top_k,
                retrieval_query_text,
                error_type,
            ],
        )
        .map_err(|e| TableError::Internal(format!("build genai.embeddings batch: {e}")))
    }
}

/// A projected `genai.tool_calls` row.
struct ToolCallRow {
    base: SpanBase,
    conversation_id: Option<String>,
    provider_name: String,
    operation_name: String,
    tool_name: Option<String>,
    tool_type: Option<String>,
    tool_call_id: Option<String>,
    tool_description: Option<String>,
    tool_call_arguments: Option<String>,
    tool_call_result: Option<String>,
    mcp_session_id: Option<String>,
    mcp_method_name: Option<String>,
    mcp_protocol_version: Option<String>,
    mcp_resource_uri: Option<String>,
    error_type: Option<String>,
}

impl ToolCallRow {
    fn from_span(base: &SpanBase, attrs: &Attrs) -> Self {
        Self {
            base: base.clone_shallow(),
            conversation_id: attr_str(attrs, "gen_ai.conversation.id"),
            provider_name: attr_str(attrs, "gen_ai.provider.name").unwrap_or_default(),
            operation_name: attr_str(attrs, "gen_ai.operation.name").unwrap_or_default(),
            tool_name: attr_str(attrs, "gen_ai.tool.name"),
            tool_type: attr_str(attrs, "gen_ai.tool.type"),
            tool_call_id: attr_str(attrs, "gen_ai.tool.call.id"),
            tool_description: attr_str(attrs, "gen_ai.tool.description"),
            tool_call_arguments: attr_json(attrs, "gen_ai.tool.call.arguments"),
            tool_call_result: attr_json(attrs, "gen_ai.tool.call.result"),
            mcp_session_id: attr_str(attrs, "mcp.session.id"),
            mcp_method_name: attr_str(attrs, "mcp.method.name"),
            mcp_protocol_version: attr_str(attrs, "mcp.protocol.version"),
            mcp_resource_uri: attr_str(attrs, "mcp.resource.uri"),
            error_type: attr_str(attrs, "error.type"),
        }
    }

    fn schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            fields::fixed_binary("trace_id", 16, false),
            fields::fixed_binary("span_id", 8, false),
            fields::fixed_binary("parent_span_id", 8, true),
            fields::utf8("conversation_id", true),
            fields::ts_us_utc("start_time", false),
            fields::ts_us_utc("end_time", false),
            fields::int64("duration_ms", false),
            fields::utf8("status", false),
            fields::utf8("service_name", false),
            fields::utf8("provider_name", false),
            fields::utf8("operation_name", false),
            fields::utf8("tool_name", true),
            fields::utf8("tool_type", true),
            fields::utf8("tool_call_id", true),
            fields::utf8("tool_description", true),
            fields::utf8_view("tool_call_arguments", true),
            fields::utf8_view("tool_call_result", true),
            fields::utf8("mcp_session_id", true),
            fields::utf8("mcp_method_name", true),
            fields::utf8("mcp_protocol_version", true),
            fields::utf8("mcp_resource_uri", true),
            fields::utf8("error_type", true),
        ]))
    }

    fn to_batch(rows: &[Self]) -> Result<RecordBatch, TableError> {
        let trace_id = fixed_bin16_col(rows.iter().map(|r| r.base.trace_id));
        let span_id = fixed_bin8_col(rows.iter().map(|r| Some(r.base.span_id)));
        let parent_span_id = fixed_bin8_col(rows.iter().map(|r| r.base.parent_span_id));
        let conversation_id = str_col(rows.iter().map(|r| r.conversation_id.clone()));
        let start_time = ts_col(rows.iter().map(|r| r.base.start_time_us));
        let end_time = ts_col(rows.iter().map(|r| r.base.end_time_us));
        let duration_ms = Arc::new(Int64Array::from_iter_values(
            rows.iter().map(|r| r.base.duration_ms),
        )) as Arc<dyn Array>;
        let status = str_col(rows.iter().map(|r| Some(r.base.status.clone())));
        let service_name = str_col(rows.iter().map(|r| Some(r.base.service_name.clone())));
        let provider_name = str_col(rows.iter().map(|r| Some(r.provider_name.clone())));
        let operation_name = str_col(rows.iter().map(|r| Some(r.operation_name.clone())));
        let tool_name = str_col(rows.iter().map(|r| r.tool_name.clone()));
        let tool_type = str_col(rows.iter().map(|r| r.tool_type.clone()));
        let tool_call_id = str_col(rows.iter().map(|r| r.tool_call_id.clone()));
        let tool_description = str_col(rows.iter().map(|r| r.tool_description.clone()));
        let tool_call_arguments = str_view_col(rows.iter().map(|r| r.tool_call_arguments.clone()));
        let tool_call_result = str_view_col(rows.iter().map(|r| r.tool_call_result.clone()));
        let mcp_session_id = str_col(rows.iter().map(|r| r.mcp_session_id.clone()));
        let mcp_method_name = str_col(rows.iter().map(|r| r.mcp_method_name.clone()));
        let mcp_protocol_version = str_col(rows.iter().map(|r| r.mcp_protocol_version.clone()));
        let mcp_resource_uri = str_col(rows.iter().map(|r| r.mcp_resource_uri.clone()));
        let error_type = str_col(rows.iter().map(|r| r.error_type.clone()));

        RecordBatch::try_new(
            Self::schema(),
            vec![
                trace_id,
                span_id,
                parent_span_id,
                conversation_id,
                start_time,
                end_time,
                duration_ms,
                status,
                service_name,
                provider_name,
                operation_name,
                tool_name,
                tool_type,
                tool_call_id,
                tool_description,
                tool_call_arguments,
                tool_call_result,
                mcp_session_id,
                mcp_method_name,
                mcp_protocol_version,
                mcp_resource_uri,
                error_type,
            ],
        )
        .map_err(|e| TableError::Internal(format!("build genai.tool_calls batch: {e}")))
    }
}

/// A projected `genai.memory` row.
struct MemoryRow {
    base: SpanBase,
    provider_name: String,
    operation_name: String,
    memory_store_id: Option<String>,
    memory_record_id: Option<String>,
    memory_record_count: Option<i64>,
    memory_query_text: Option<String>,
    memory_records: Option<String>,
    error_type: Option<String>,
}

impl MemoryRow {
    fn from_span(base: &SpanBase, attrs: &Attrs) -> Self {
        Self {
            base: base.clone_shallow(),
            provider_name: attr_str(attrs, "gen_ai.provider.name").unwrap_or_default(),
            operation_name: attr_str(attrs, "gen_ai.operation.name").unwrap_or_default(),
            memory_store_id: attr_str(attrs, "gen_ai.memory.store.id"),
            memory_record_id: attr_str(attrs, "gen_ai.memory.record.id"),
            memory_record_count: attr_i64(attrs, "gen_ai.memory.record.count"),
            memory_query_text: attr_str(attrs, "gen_ai.memory.query.text"),
            memory_records: attr_json(attrs, "gen_ai.memory.records"),
            error_type: attr_str(attrs, "error.type"),
        }
    }

    fn schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            fields::fixed_binary("trace_id", 16, false),
            fields::fixed_binary("span_id", 8, false),
            fields::fixed_binary("parent_span_id", 8, true),
            fields::ts_us_utc("start_time", false),
            fields::ts_us_utc("end_time", false),
            fields::int64("duration_ms", false),
            fields::utf8("status", false),
            fields::utf8("service_name", false),
            fields::utf8("provider_name", false),
            fields::utf8("operation_name", false),
            fields::utf8("memory_store_id", true),
            fields::utf8("memory_record_id", true),
            fields::int64("memory_record_count", true),
            fields::utf8("memory_query_text", true),
            fields::utf8_view("memory_records", true),
            fields::utf8("error_type", true),
        ]))
    }

    fn to_batch(rows: &[Self]) -> Result<RecordBatch, TableError> {
        let trace_id = fixed_bin16_col(rows.iter().map(|r| r.base.trace_id));
        let span_id = fixed_bin8_col(rows.iter().map(|r| Some(r.base.span_id)));
        let parent_span_id = fixed_bin8_col(rows.iter().map(|r| r.base.parent_span_id));
        let start_time = ts_col(rows.iter().map(|r| r.base.start_time_us));
        let end_time = ts_col(rows.iter().map(|r| r.base.end_time_us));
        let duration_ms = Arc::new(Int64Array::from_iter_values(
            rows.iter().map(|r| r.base.duration_ms),
        )) as Arc<dyn Array>;
        let status = str_col(rows.iter().map(|r| Some(r.base.status.clone())));
        let service_name = str_col(rows.iter().map(|r| Some(r.base.service_name.clone())));
        let provider_name = str_col(rows.iter().map(|r| Some(r.provider_name.clone())));
        let operation_name = str_col(rows.iter().map(|r| Some(r.operation_name.clone())));
        let memory_store_id = str_col(rows.iter().map(|r| r.memory_store_id.clone()));
        let memory_record_id = str_col(rows.iter().map(|r| r.memory_record_id.clone()));
        let memory_record_count = i64_col(rows.iter().map(|r| r.memory_record_count));
        let memory_query_text = str_col(rows.iter().map(|r| r.memory_query_text.clone()));
        let memory_records = str_view_col(rows.iter().map(|r| r.memory_records.clone()));
        let error_type = str_col(rows.iter().map(|r| r.error_type.clone()));

        RecordBatch::try_new(
            Self::schema(),
            vec![
                trace_id,
                span_id,
                parent_span_id,
                start_time,
                end_time,
                duration_ms,
                status,
                service_name,
                provider_name,
                operation_name,
                memory_store_id,
                memory_record_id,
                memory_record_count,
                memory_query_text,
                memory_records,
                error_type,
            ],
        )
        .map_err(|e| TableError::Internal(format!("build genai.memory batch: {e}")))
    }
}

impl SpanBase {
    fn clone_shallow(&self) -> Self {
        Self {
            trace_id: self.trace_id,
            span_id: self.span_id,
            parent_span_id: self.parent_span_id,
            start_time_us: self.start_time_us,
            end_time_us: self.end_time_us,
            duration_ms: self.duration_ms,
            status: self.status.clone(),
            service_name: self.service_name.clone(),
        }
    }
}

// ── column downcast helpers (read side) ──────────────────────────────────────

fn fixed_bin<'a>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<&'a FixedSizeBinaryArray, TableError> {
    batch
        .column_by_name(name)
        .ok_or_else(|| TableError::Internal(format!("spans missing {name}")))?
        .as_any()
        .downcast_ref::<FixedSizeBinaryArray>()
        .ok_or_else(|| TableError::Internal(format!("spans {name} not FixedSizeBinary")))
}

fn fixed_bin_opt<'a>(batch: &'a RecordBatch, name: &str) -> Option<&'a FixedSizeBinaryArray> {
    batch
        .column_by_name(name)
        .and_then(|c| c.as_any().downcast_ref::<FixedSizeBinaryArray>())
}

fn ts_us<'a>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<&'a TimestampMicrosecondArray, TableError> {
    batch
        .column_by_name(name)
        .ok_or_else(|| TableError::Internal(format!("spans missing {name}")))?
        .as_any()
        .downcast_ref::<TimestampMicrosecondArray>()
        .ok_or_else(|| TableError::Internal(format!("spans {name} not TimestampMicrosecond")))
}

fn int64<'a>(batch: &'a RecordBatch, name: &str) -> Result<&'a Int64Array, TableError> {
    batch
        .column_by_name(name)
        .ok_or_else(|| TableError::Internal(format!("spans missing {name}")))?
        .as_any()
        .downcast_ref::<Int64Array>()
        .ok_or_else(|| TableError::Internal(format!("spans {name} not Int64")))
}

fn string<'a>(batch: &'a RecordBatch, name: &str) -> Result<&'a StringArray, TableError> {
    batch
        .column_by_name(name)
        .ok_or_else(|| TableError::Internal(format!("spans missing {name}")))?
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| TableError::Internal(format!("spans {name} not Utf8")))
}

fn attributes_col(batch: &RecordBatch) -> Option<AttributesArray<'_>> {
    let col = batch.column_by_name("attributes")?;
    if let Some(view) = col.as_any().downcast_ref::<StringViewArray>() {
        return Some(AttributesArray::View(view));
    }
    col.as_any()
        .downcast_ref::<StringArray>()
        .map(AttributesArray::Utf8)
}

// ── column builders (write side) ─────────────────────────────────────────────

fn fixed_bin16_col(vals: impl Iterator<Item = [u8; 16]>) -> Arc<dyn Array> {
    let mut b = FixedSizeBinaryBuilder::new(16);
    for v in vals {
        b.append_value(v).expect("16-byte fixed value");
    }
    Arc::new(b.finish())
}

fn fixed_bin8_col(vals: impl Iterator<Item = Option<[u8; 8]>>) -> Arc<dyn Array> {
    let mut b = FixedSizeBinaryBuilder::new(8);
    for v in vals {
        match v {
            Some(bytes) => b.append_value(bytes).expect("8-byte fixed value"),
            None => b.append_null(),
        }
    }
    Arc::new(b.finish())
}

fn ts_col(vals: impl Iterator<Item = i64>) -> Arc<dyn Array> {
    Arc::new(TimestampMicrosecondArray::from_iter_values(vals).with_timezone("UTC".to_string()))
}

fn str_col(vals: impl Iterator<Item = Option<String>>) -> Arc<dyn Array> {
    Arc::new(vals.collect::<StringArray>())
}

fn str_view_col(vals: impl Iterator<Item = Option<String>>) -> Arc<dyn Array> {
    Arc::new(vals.collect::<StringViewArray>())
}

fn i64_col(vals: impl Iterator<Item = Option<i64>>) -> Arc<dyn Array> {
    Arc::new(vals.collect::<Int64Array>())
}

fn bool_col(vals: impl Iterator<Item = Option<bool>>) -> Arc<dyn Array> {
    Arc::new(vals.collect::<BooleanArray>())
}

fn f64_col(vals: impl Iterator<Item = Option<f64>>) -> Arc<dyn Array> {
    Arc::new(vals.collect::<Float64Array>())
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::TimestampMicrosecondArray;

    /// Build a one-row spans batch carrying a `gen_ai` chat span with token usage.
    fn spans_batch(tenant: DataTenantId, attributes: &serde_json::Value) -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![
            fields::fixed_binary("trace_id", 16, false),
            fields::fixed_binary("span_id", 8, false),
            fields::fixed_binary("parent_span_id", 8, true),
            fields::utf8("name", false),
            fields::utf8("kind", false),
            fields::ts_us_utc("start_time", false),
            fields::ts_us_utc("end_time", false),
            fields::int64("duration_ms", false),
            fields::utf8("status", false),
            fields::utf8_view("attributes", true),
            fields::utf8("service_name", false),
            fields::utf8(DATA_TENANT_ID, false),
        ]));
        let trace = fixed_bin16_col(std::iter::once([7u8; 16]));
        let span = fixed_bin8_col(std::iter::once(Some([3u8; 8])));
        let parent = fixed_bin8_col(std::iter::once(None));
        let name = str_col(std::iter::once(Some("chat".to_owned())));
        let kind = str_col(std::iter::once(Some("client".to_owned())));
        let start = ts_col(std::iter::once(1_000i64));
        let end = ts_col(std::iter::once(2_000i64));
        let dur = Arc::new(Int64Array::from_iter_values(std::iter::once(1i64))) as Arc<dyn Array>;
        let status = str_col(std::iter::once(Some("ok".to_owned())));
        let attrs = str_view_col(std::iter::once(Some(attributes.to_string())));
        let service = str_col(std::iter::once(Some("svc".to_owned())));
        let tid = str_col(std::iter::once(Some(tenant.to_string())));
        RecordBatch::try_new(
            schema,
            vec![
                trace, span, parent, name, kind, start, end, dur, status, attrs, service, tid,
            ],
        )
        .expect("spans batch")
    }

    /// Gate: `genai_projector_maps_gen_ai_attributes`.
    ///
    /// A `gen_ai` chat span projects to exactly one `genai.messages` `DerivedBatch`
    /// carrying the correct provider/model and non-null token counts, bound to the
    /// span's own tenant.
    #[test]
    fn genai_projector_maps_gen_ai_attributes() {
        let tenant = DataTenantId::new_v7();
        let batch = spans_batch(
            tenant,
            &serde_json::json!({
                "gen_ai.provider.name": "openai",
                "gen_ai.operation.name": "chat",
                "gen_ai.request.model": "gpt-4o",
                "gen_ai.usage.input_tokens": 11,
                "gen_ai.usage.output_tokens": 22,
            }),
        );
        let src_batch_id = [9u8; 16];
        let out = GenAiFromSpans
            .derive(&batch, src_batch_id)
            .expect("derive ok");

        assert_eq!(out.len(), 1, "one message-target partition");
        let derived = &out[0];
        assert_eq!(derived.target_namespace, "genai");
        assert_eq!(derived.target_name, "messages");
        assert_eq!(derived.data_tenant_id, tenant);
        assert_eq!(derived.source_wyrd_batch_id, src_batch_id);

        let b = &derived.batch;
        let prov = b
            .column_by_name("provider_name")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(prov.value(0), "openai");
        let model = b
            .column_by_name("request_model")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(model.value(0), "gpt-4o");
        let in_tok = b
            .column_by_name("usage_input_tokens")
            .unwrap()
            .as_any()
            .downcast_ref::<Int64Array>()
            .expect("usage_input_tokens is physically int64");
        assert!(!in_tok.is_null(0), "input tokens must be non-null");
        assert_eq!(in_tok.value(0), 11);
        let out_tok = b
            .column_by_name("usage_output_tokens")
            .unwrap()
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
        assert_eq!(out_tok.value(0), 22);

        // Timestamps round-trip as microsecond UTC.
        let start = b
            .column_by_name("start_time")
            .unwrap()
            .as_any()
            .downcast_ref::<TimestampMicrosecondArray>()
            .unwrap();
        assert_eq!(start.value(0), 1_000);
    }

    /// Gate: `genai_reproject_is_replay_noop`.
    ///
    /// Re-deriving the SAME source batch yields the SAME `derived_batch_id`, so the
    /// coordinator dedups the second append as a replay. This proves the
    /// exactly-once key is a pure function of (derivation, target, source, source
    /// batch id) and does not drift across re-derivations.
    #[test]
    fn genai_reproject_is_replay_noop() {
        let target_uid = [1u8; 16];
        let source_uid = [2u8; 16];
        let src_batch_id = [3u8; 16];

        let first = GenAiFromSpans::derived_batch_id(&target_uid, &source_uid, &src_batch_id);
        let second = GenAiFromSpans::derived_batch_id(&target_uid, &source_uid, &src_batch_id);
        assert_eq!(
            first, second,
            "same inputs → same derived_batch_id (replay)"
        );

        // A different source batch produces a different key (no accidental dedup).
        let other = GenAiFromSpans::derived_batch_id(&target_uid, &source_uid, &[4u8; 16]);
        assert_ne!(first, other, "different source batch → different key");

        // A different target produces a different key (targets do not collide).
        let other_target = GenAiFromSpans::derived_batch_id(&[9u8; 16], &source_uid, &src_batch_id);
        assert_ne!(first, other_target, "different target → different key");
    }

    /// Gate: `genai_token_columns_are_int64`.
    ///
    /// The flipped token columns build as physical `Int64` (not `UInt32`), so a
    /// strict Int64 downcast succeeds on the way back.
    #[test]
    fn genai_token_columns_are_int64() {
        let tenant = DataTenantId::new_v7();
        let batch = spans_batch(
            tenant,
            &serde_json::json!({
                "gen_ai.provider.name": "openai",
                "gen_ai.operation.name": "chat",
                "gen_ai.request.model": "gpt-4o",
                "gen_ai.usage.input_tokens": 7,
            }),
        );
        let out = GenAiFromSpans.derive(&batch, [0u8; 16]).expect("derive ok");
        let b = &out[0].batch;
        let col = b.column_by_name("usage_input_tokens").unwrap();
        let arr = col
            .as_any()
            .downcast_ref::<Int64Array>()
            .expect("usage_input_tokens round-trips as int64");
        assert_eq!(arr.value(0), 7);
    }

    /// Gate: `genai_openai_and_mcp_namespace_populate_typed_columns`.
    ///
    /// `openai.*` and `mcp.*` attributes live in their own namespaces (NOT under
    /// `gen_ai.`). A message span carrying `openai.api.type` /
    /// `openai.request.service_tier` fills the messages typed columns; an
    /// `execute_tool` span carrying `mcp.session.id` fills the `tool_calls` column.
    #[test]
    fn genai_openai_and_mcp_namespace_populate_typed_columns() {
        let tenant = DataTenantId::new_v7();
        let msg = spans_batch(
            tenant,
            &serde_json::json!({
                "gen_ai.provider.name": "openai",
                "gen_ai.operation.name": "chat",
                "gen_ai.request.model": "gpt-4o",
                "openai.api.type": "responses",
                "openai.request.service_tier": "default",
            }),
        );
        let out = GenAiFromSpans.derive(&msg, [0u8; 16]).expect("derive ok");
        let b = &out[0].batch;
        assert_eq!(b.target_name_col("openai_api_type"), "responses");
        assert_eq!(b.target_name_col("openai_request_service_tier"), "default");

        let tool = spans_batch(
            tenant,
            &serde_json::json!({
                "gen_ai.provider.name": "openai",
                "gen_ai.operation.name": "execute_tool",
                "gen_ai.tool.name": "search",
                "mcp.session.id": "sess-123",
            }),
        );
        let out = GenAiFromSpans.derive(&tool, [0u8; 16]).expect("derive ok");
        let b = &out[0].batch;
        assert_eq!(out[0].target_name, "tool_calls");
        assert_eq!(b.target_name_col("mcp_session_id"), "sess-123");
    }

    /// Gate: `genai_memory_operation_routes_to_memory_table`.
    ///
    /// A span with `gen_ai.operation.name=search_memory` and memory attributes
    /// routes to a `genai.memory` `DerivedBatch` with the memory columns populated.
    #[test]
    fn genai_memory_operation_routes_to_memory_table() {
        let tenant = DataTenantId::new_v7();
        let batch = spans_batch(
            tenant,
            &serde_json::json!({
                "gen_ai.provider.name": "openai",
                "gen_ai.operation.name": "search_memory",
                "gen_ai.memory.store.id": "store-1",
                "gen_ai.memory.record.count": 3,
                "gen_ai.memory.query.text": "what did we decide",
                "gen_ai.memory.records": [{"id": "r1"}],
            }),
        );
        let out = GenAiFromSpans.derive(&batch, [0u8; 16]).expect("derive ok");
        assert_eq!(out.len(), 1, "one memory-target partition");
        let derived = &out[0];
        assert_eq!(derived.target_namespace, "genai");
        assert_eq!(derived.target_name, "memory");
        assert_eq!(derived.data_tenant_id, tenant);

        let b = &derived.batch;
        assert_eq!(b.target_name_col("memory_store_id"), "store-1");
        let count = b
            .column_by_name("memory_record_count")
            .unwrap()
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
        assert_eq!(count.value(0), 3);
        let records = b
            .column_by_name("memory_records")
            .unwrap()
            .as_any()
            .downcast_ref::<StringViewArray>()
            .unwrap();
        assert!(!records.is_null(0), "memory_records must be populated");
    }

    /// Gate: `genai_retrieval_operation_routes_to_embeddings_table`.
    ///
    /// A span with `gen_ai.operation.name=retrieval` routes to a `genai.embeddings`
    /// `DerivedBatch` with the retrieval columns (`data_source_id`,
    /// `retrieval_top_k`) populated on the typed row — not dumped into a
    /// `genai.messages` `extra` blob. The embeddings table is the only row type
    /// that declares those retrieval columns.
    #[test]
    fn genai_retrieval_operation_routes_to_embeddings_table() {
        let tenant = DataTenantId::new_v7();
        let batch = spans_batch(
            tenant,
            &serde_json::json!({
                "gen_ai.provider.name": "openai",
                "gen_ai.operation.name": "retrieval",
                "gen_ai.request.model": "text-embedding-3-large",
                "gen_ai.data_source.id": "vector-store-7",
                "gen_ai.retrieval.top_k": 8,
            }),
        );
        let out = GenAiFromSpans.derive(&batch, [0u8; 16]).expect("derive ok");
        assert_eq!(out.len(), 1, "one embeddings-target partition");
        let derived = &out[0];
        assert_eq!(derived.target_namespace, "genai");
        assert_eq!(
            derived.target_name, "embeddings",
            "retrieval span must land on the embeddings table, not messages"
        );
        assert_eq!(derived.data_tenant_id, tenant);

        let b = &derived.batch;
        assert!(
            b.column_by_name("extra").is_none(),
            "embeddings row has no extra blob; retrieval attrs must be typed columns"
        );
        assert_eq!(b.target_name_col("data_source_id"), "vector-store-7");
        let top_k = b
            .column_by_name("retrieval_top_k")
            .unwrap()
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
        assert!(!top_k.is_null(0), "retrieval_top_k must be populated");
        assert_eq!(top_k.value(0), 8);
    }

    /// Build a one-row `traces.spans` batch with a caller-controlled `span_id`
    /// so a multi-row batch can be constructed by concatenation without
    /// producing duplicate span identifiers (which downstream dedup would
    /// collapse).
    fn spans_batch_with_span_id(
        tenant: DataTenantId,
        span_id: [u8; 8],
        attributes: &serde_json::Value,
    ) -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![
            fields::fixed_binary("trace_id", 16, false),
            fields::fixed_binary("span_id", 8, false),
            fields::fixed_binary("parent_span_id", 8, true),
            fields::utf8("name", false),
            fields::utf8("kind", false),
            fields::ts_us_utc("start_time", false),
            fields::ts_us_utc("end_time", false),
            fields::int64("duration_ms", false),
            fields::utf8("status", false),
            fields::utf8_view("attributes", true),
            fields::utf8("service_name", false),
            fields::utf8(DATA_TENANT_ID, false),
        ]));
        let trace = fixed_bin16_col(std::iter::once([7u8; 16]));
        let span = fixed_bin8_col(std::iter::once(Some(span_id)));
        let parent = fixed_bin8_col(std::iter::once(None));
        let name = str_col(std::iter::once(Some("chat".to_owned())));
        let kind = str_col(std::iter::once(Some("client".to_owned())));
        let start = ts_col(std::iter::once(1_000i64));
        let end = ts_col(std::iter::once(2_000i64));
        let dur = Arc::new(Int64Array::from_iter_values(std::iter::once(1i64))) as Arc<dyn Array>;
        let status = str_col(std::iter::once(Some("ok".to_owned())));
        let attrs = str_view_col(std::iter::once(Some(attributes.to_string())));
        let service = str_col(std::iter::once(Some("svc".to_owned())));
        let tid = str_col(std::iter::once(Some(tenant.to_string())));
        RecordBatch::try_new(
            schema,
            vec![
                trace, span, parent, name, kind, start, end, dur, status, attrs, service, tid,
            ],
        )
        .expect("spans batch")
    }

    /// A `DataFusion` scan of one logical source batch can return multiple
    /// physical `RecordBatch` chunks. `process_batches` concatenates them with
    /// `arrow::compute::concat_batches` and calls `derive` exactly once per
    /// source batch, then writes each target with the deterministic
    /// `derived_batch_id = f(target, source, source_batch_id)`. This test
    /// concatenates a chunked source and asserts every row survives:
    /// row counts per target match the row counts in the chunks.
    #[test]
    fn genai_derive_over_concatenated_chunks_preserves_every_row() {
        use arrow::compute::concat_batches;

        let tenant = DataTenantId::new_v7();
        // 12 spans split across 3 target kinds: 4 chat -> messages,
        // 4 embeddings -> embeddings, 4 execute_tool -> tool_calls.
        // Every span carries a distinct span_id so no downstream dedup can
        // collapse rows accidentally.
        let mut per_row: Vec<RecordBatch> = Vec::with_capacity(12);
        for i in 0..12u8 {
            let op = match i % 3 {
                0 => "chat",
                1 => "embeddings",
                _ => "execute_tool",
            };
            let mut attrs = serde_json::json!({
                "gen_ai.provider.name": "openai",
                "gen_ai.operation.name": op,
                "gen_ai.request.model": "gpt-4o",
            });
            if op == "execute_tool" {
                attrs
                    .as_object_mut()
                    .unwrap()
                    .insert("gen_ai.tool.name".to_owned(), serde_json::json!("search"));
            }
            per_row.push(spans_batch_with_span_id(tenant, [i + 1; 8], &attrs));
        }
        let schema = per_row[0].schema();

        // Physical scan shape: 3 chunks of 4 rows each. Mirrors DataFusion
        // splitting a scan into multiple RecordBatches per source batch.
        let chunk_a = concat_batches(&schema, &per_row[0..4]).expect("chunk a");
        let chunk_b = concat_batches(&schema, &per_row[4..8]).expect("chunk b");
        let chunk_c = concat_batches(&schema, &per_row[8..12]).expect("chunk c");
        let chunks = [chunk_a, chunk_b, chunk_c];

        // Concat physical chunks into one logical batch, then one derive call.
        let logical = concat_batches(&schema, &chunks).expect("concat logical");
        assert_eq!(
            logical.num_rows(),
            12,
            "concat_batches sums the physical chunks; nothing is dropped"
        );

        let derived = GenAiFromSpans
            .derive(&logical, [0xAAu8; 16])
            .expect("derive over concatenated batch");

        let count_target = |name: &str| -> usize {
            derived
                .iter()
                .filter(|d| d.target_name == name)
                .map(|d| d.batch.num_rows())
                .sum()
        };
        let messages = count_target(MessagesTable::NAME);
        let embeddings = count_target(EmbeddingsTable::NAME);
        let tool_calls = count_target(ToolCallsTable::NAME);

        assert_eq!(messages, 4, "4 chat spans project to 4 messages rows");
        assert_eq!(
            embeddings, 4,
            "4 embeddings spans project to 4 embeddings rows"
        );
        assert_eq!(
            tool_calls, 4,
            "4 execute_tool spans project to 4 tool_calls rows"
        );
        assert_eq!(
            messages + embeddings + tool_calls,
            12,
            "concat-then-derive preserves every source row across chunk boundaries"
        );
    }

    /// Concat-then-derive is semantically equivalent to derive on a single
    /// batch of the same rows: same per-target partition count, same
    /// per-target row totals. So chunking is transparent — the derivation's
    /// output does not depend on how the physical scan is split.
    #[test]
    fn genai_derive_on_concat_equals_derive_on_single_batch() {
        use arrow::compute::concat_batches;

        let tenant = DataTenantId::new_v7();
        let mut per_row: Vec<RecordBatch> = Vec::with_capacity(6);
        for i in 0..6u8 {
            per_row.push(spans_batch_with_span_id(
                tenant,
                [i + 1; 8],
                &serde_json::json!({
                    "gen_ai.provider.name": "openai",
                    "gen_ai.operation.name": "chat",
                    "gen_ai.request.model": format!("m-{i}"),
                }),
            ));
        }
        let schema = per_row[0].schema();

        // Ground truth: one physical batch of 6 rows.
        let single = concat_batches(&schema, &per_row).expect("concat single");
        let single_derived = GenAiFromSpans
            .derive(&single, [0xBBu8; 16])
            .expect("derive single");

        // Chunked path: split into 2 chunks of 3 rows, then concat before derive.
        let chunk_a = concat_batches(&schema, &per_row[0..3]).expect("a");
        let chunk_b = concat_batches(&schema, &per_row[3..6]).expect("b");
        let chunks = [chunk_a, chunk_b];
        let recombined = concat_batches(&schema, &chunks).expect("recombine");
        let chunked_derived = GenAiFromSpans
            .derive(&recombined, [0xBBu8; 16])
            .expect("derive chunked");

        assert_eq!(
            single_derived.len(),
            chunked_derived.len(),
            "same number of DerivedBatch partitions"
        );
        let sum_rows = |v: &[DerivedBatch]| -> usize { v.iter().map(|d| d.batch.num_rows()).sum() };
        assert_eq!(sum_rows(&single_derived), 6, "single-path preserves 6 rows");
        assert_eq!(
            sum_rows(&single_derived),
            sum_rows(&chunked_derived),
            "concat-then-derive is semantically equivalent to derive on a single batch"
        );
    }

    /// Small read helper for the tests above: value of a Utf8 column at row 0.
    trait Utf8Col {
        fn target_name_col(&self, name: &str) -> &str;
    }
    impl Utf8Col for RecordBatch {
        fn target_name_col(&self, name: &str) -> &str {
            self.column_by_name(name)
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
                .value(0)
        }
    }
}
