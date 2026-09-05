//! Exact final managed-column planning and materialization for typed OTLP slices.

use std::sync::Arc;

use arrow::array::{
    ArrayRef, BooleanArray, FixedSizeBinaryArray, Int32Array, NullArray, RecordBatch, StringArray,
    TimestampMicrosecondArray,
};
use arrow::buffer::{BooleanBuffer, Buffer, NullBuffer, OffsetBuffer, ScalarBuffer};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use wyrd_runtime::principal::Principal;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::managed_columns::{
    CARD_UID, DATA_TENANT_ID, PRINCIPAL_ID, RUN_ID, WYRD_BATCH_ID, WYRD_EVENT_TIME,
    WYRD_INGESTED_AT, WYRD_REQUEST_ID, WYRD_ROW_ORDINAL,
};

use crate::contracts::{ScribeError, projected_source_schema_fingerprint};
use crate::schema::SchemaFingerprint;

use super::fixed_ipc::{FixedIpcColumnPlan, FixedIpcPlan};

/// Maximum final flat fields accepted by the shared fixed IPC encoder.
const MAX_FINAL_FIELDS: usize = 256;
/// Number of server-owned columns appended after the retained OTLP `run_id`.
const MANAGED_COLUMNS: usize = 8;
/// Largest textual UUID form accepted by `uuid` and therefore by `RequestId`.
const MAX_REQUEST_ID_BYTES: usize = 45;
/// Empty fixed fact used for uninitialized inline plan entries.
const EMPTY_FACT: FixedIpcColumnPlan = FixedIpcColumnPlan {
    null_count: 0,
    validity_bytes: 0,
    offsets_bytes: 0,
    values_bytes: 0,
};

/// Authenticated immutable context shared by OTLP preflight and projection.
///
/// Borrowed identity and fixed values keep planning independent of payload ownership.
/// All signal operations use the same schema authority and managed row values.
pub(crate) struct OtlpProjection<'a> {
    /// Authenticated principal supplying tenant and principal identity.
    principal: &'a Principal,
    /// Catalog-authoritative user schema fingerprint.
    expected_fingerprint: SchemaFingerprint,
    /// Validated correlation identifier retained without copying.
    request_id: &'a RequestId,
    /// Durable identity shared by every row in this batch.
    batch_id: uuid::Uuid,
    /// Single receipt timestamp used by admission and materialization.
    receipt_micros: i64,
}

impl<'a> OtlpProjection<'a> {
    /// Captures the existing authenticated projection context without allocation.
    pub(crate) fn new(
        principal: &'a Principal,
        expected_fingerprint: SchemaFingerprint,
        request_id: &'a RequestId,
        batch_id: uuid::Uuid,
        receipt_micros: i64,
    ) -> Self {
        Self {
            principal,
            expected_fingerprint,
            request_id,
            batch_id,
            receipt_micros,
        }
    }

    /// Plans authoritative managed columns for one signal's accepted rows.
    ///
    /// # Errors
    ///
    /// Returns schema, identity, or checked-size validation failures.
    pub(super) fn managed(
        &self,
        schema: Arc<Schema>,
        rows: usize,
    ) -> Result<OtlpManagedProjection, ScribeError> {
        OtlpManagedProjection::plan(
            schema,
            self.expected_fingerprint,
            rows,
            self.principal,
            self.request_id,
            self.batch_id,
            self.receipt_micros,
        )
    }
}

/// Owns the fixed authenticated identity reused by OTLP projection unit fixtures.
#[cfg(test)]
pub(crate) struct OtlpTestContext {
    /// Fixed principal and tenant shared by sizing and materialization.
    principal: Principal,
    /// Fixed request identity with production validation.
    request_id: RequestId,
}

#[cfg(test)]
impl OtlpTestContext {
    /// Constructs deterministic valid context for IO-free projection tests.
    ///
    /// # Panics
    ///
    /// Panics only if the literal fixture identifiers violate their contracts.
    pub(crate) fn new() -> Self {
        Self {
            principal: Principal::new(
                wyrd_runtime::principal::PrincipalId::new(uuid::Uuid::from_u128(1)),
                wyrd_runtime::principal::PrincipalKind::User,
                wyrd_spec::DataTenantId::new(
                    uuid::Uuid::parse_str("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b01")
                        .expect("fixed UUIDv7"),
                )
                .expect("fixed tenant"),
                Vec::new(),
                wyrd_runtime::permission::PermissionSet::new(),
            ),
            request_id: RequestId::parse("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00")
                .expect("fixed request id"),
        }
    }

    /// Borrows the same authenticated context for a canonical signal schema.
    pub(crate) fn projection(&self, schema: &Schema) -> OtlpProjection<'_> {
        OtlpProjection::new(
            &self.principal,
            crate::contracts::projected_source_schema_fingerprint(schema),
            &self.request_id,
            uuid::Uuid::from_u128(3),
            1_700_000_000_000_000,
        )
    }
}

/// Combined final Arrow backing and IPC sizing result for one OTLP slice.
pub(crate) struct OtlpManagedMaterialPlan {
    /// Accepted rows represented by the measured projection.
    pub(crate) rows: usize,
    /// Exact final fixed IPC plan consumed by WAL slice encoding.
    pub(crate) ipc_plan: FixedIpcPlan,
    /// Exact Arrow public array memory reported by the finished record batch.
    pub(crate) arrow_bytes: usize,
}

impl OtlpManagedMaterialPlan {
    /// Returns the exact simultaneous final Arrow plus IPC material charge.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::DecodedPayloadTooLarge`] when the combined size
    /// overflows or exceeds `limit`.
    pub(crate) fn admitted_bytes(&self, limit: usize) -> Result<usize, ScribeError> {
        let bytes = self
            .arrow_bytes
            .checked_add(self.ipc_plan.encoded_bytes())
            .ok_or(ScribeError::DecodedPayloadTooLarge {
                bytes: usize::MAX,
                limit,
            })?;
        if bytes > limit {
            return Err(ScribeError::DecodedPayloadTooLarge { bytes, limit });
        }
        Ok(bytes)
    }
}

/// One finished OTLP projection paired with its pre-material IPC plan.
///
/// Keeping these values move-only and adjacent prevents WAL encoding from
/// recounting the already-materialized batch or losing the admission facts.
pub(crate) struct OtlpManagedBatch {
    /// Canonical final rows whose buffers were checked against the plan.
    pub(crate) rows: RecordBatch,
    /// Immutable plan counted before any scalable managed allocation.
    pub(crate) ipc_plan: FixedIpcPlan,
}

/// Fixed inline textual storage used by immutable managed values.
///
/// The representation prevents request- or identity-dependent planner heap
/// allocation while preserving the validated wire spelling of `RequestId`.
struct InlineText<const N: usize> {
    /// Initialized prefix copied from the validated identifier.
    bytes: [u8; N],
    /// Number of initialized bytes in `bytes`.
    len: usize,
}

impl<const N: usize> InlineText<N> {
    /// Copies one bounded UTF-8 identifier into fixed inline storage.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] when the identifier exceeds its
    /// contract's fixed textual bound.
    fn copy(value: &str) -> Result<Self, ScribeError> {
        if value.len() > N {
            return Err(ScribeError::InvalidFrame);
        }
        let mut bytes = [0_u8; N];
        bytes[..value.len()].copy_from_slice(value.as_bytes());
        Ok(Self {
            bytes,
            len: value.len(),
        })
    }

    /// Borrows the initialized UTF-8 bytes without allocation.
    #[must_use]
    fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

/// Fixed authenticated identifiers stamped into one finished OTLP slice.
struct ManagedIdentity {
    /// Exact principal identifier stamped on every row.
    principal: InlineText<36>,
    /// Exact request identifier stamped on every row.
    request: InlineText<MAX_REQUEST_ID_BYTES>,
    /// Exact authenticated tenant identifier stamped on every row.
    tenant: InlineText<36>,
}

impl ManagedIdentity {
    /// Captures authenticated identifiers without growable planner storage.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] when a validated identifier does
    /// not fit its fixed textual contract or cannot be represented as UTF-8.
    fn capture(principal: &Principal, request_id: &RequestId) -> Result<Self, ScribeError> {
        let mut principal_text = [0_u8; 36];
        principal
            .id
            .as_uuid()
            .hyphenated()
            .encode_lower(&mut principal_text);
        let principal_id = InlineText::copy(
            std::str::from_utf8(&principal_text).map_err(|_| ScribeError::InvalidFrame)?,
        )?;
        let request_id = InlineText::copy(request_id.as_str())?;
        let mut tenant_text = [0_u8; 36];
        principal
            .tenant_id
            .as_uuid()
            .hyphenated()
            .encode_lower(&mut tenant_text);
        let tenant_id = InlineText::copy(
            std::str::from_utf8(&tenant_text).map_err(|_| ScribeError::InvalidFrame)?,
        )?;
        Ok(Self {
            principal: principal_id,
            request: request_id,
            tenant: tenant_id,
        })
    }
}

/// Owns immutable final managed values and the canonical physical schema.
///
/// Planning validates the source fingerprint and placeholder schema before it
/// allocates managed strings or the final schema. Materialization retains user
/// arrays by `Arc`, drops only the mapper's `card_uid`/`principal_id`
/// placeholders, and constructs the eight server-owned arrays exactly once.
/// Schema state is bounded by [`MAX_FINAL_FIELDS`], retains source `Field` Arcs,
/// and belongs to fixed planner state; row-scaled Arrow ownership is charged by
/// `OtlpManagedMaterialPlan::arrow_bytes`.
pub(crate) struct OtlpManagedProjection {
    /// Exact mapper schema required again when materializing the base batch.
    source_schema: Arc<Schema>,
    /// Canonical final physical schema in persisted column order.
    final_schema: Arc<Schema>,
    /// Number of source columns retained, including nullable `run_id`.
    retained_source_columns: usize,
    /// Final server-owned physical buffer facts in canonical order.
    managed_facts: [FixedIpcColumnPlan; MANAGED_COLUMNS],
    /// Number of rows governed by this immutable projection.
    rows: usize,
    /// Exact principal identifier stamped on every row.
    principal_id: InlineText<36>,
    /// Exact request identifier stamped on every row.
    request_id: InlineText<MAX_REQUEST_ID_BYTES>,
    /// Exact authenticated tenant identifier stamped on every row.
    tenant_id: InlineText<36>,
    /// Stable client batch identity stamped as sixteen raw bytes.
    batch_id: [u8; 16],
    /// One server-captured receipt instant used for both managed timestamps.
    receipt_micros: i64,
}

impl OtlpManagedProjection {
    /// Plans the canonical final schema and exact server-owned buffers.
    ///
    /// The source mapper must end with nullable UTF-8 `run_id`, `card_uid`, and
    /// `principal_id` placeholders. Only `run_id` survives as user correlation;
    /// the other two are replaced by authoritative managed columns.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::FingerprintMismatch`] before managed allocation
    /// when the source user schema differs. Returns [`ScribeError::InvalidFrame`]
    /// for zero/oversized rows, a noncanonical placeholder tail, `card_ref`, or
    /// final field overflow. Returns [`ScribeError::DecodedPayloadTooLarge`] on
    /// checked buffer arithmetic overflow.
    pub(crate) fn plan(
        source_schema: Arc<Schema>,
        expected_fingerprint: SchemaFingerprint,
        rows: usize,
        principal: &Principal,
        request_id: &RequestId,
        batch_id: uuid::Uuid,
        receipt_micros: i64,
    ) -> Result<Self, ScribeError> {
        if rows == 0 || rows >= i32::MAX as usize {
            return Err(ScribeError::InvalidFrame);
        }
        let retained_source_columns =
            validate_source_schema(source_schema.as_ref(), expected_fingerprint)?;
        let final_schema = build_final_schema(source_schema.as_ref(), retained_source_columns)?;
        let identity = ManagedIdentity::capture(principal, request_id)?;
        let managed_facts = managed_facts(rows, &identity)?;

        Ok(Self {
            source_schema,
            final_schema,
            retained_source_columns,
            managed_facts,
            rows,
            principal_id: identity.principal,
            request_id: identity.request,
            tenant_id: identity.tenant,
            batch_id: *batch_id.as_bytes(),
            receipt_micros,
        })
    }

    /// Borrows the canonical final physical schema.
    #[must_use]
    #[cfg(test)]
    pub(crate) fn schema(&self) -> &Arc<Schema> {
        &self.final_schema
    }

    /// Combines source and managed facts into exact final Arrow and IPC sizes.
    ///
    /// The iterator must describe every mapper source column, including the two
    /// discarded placeholders. The method retains facts through `run_id`, then
    /// appends the eight immutable managed facts without growable descriptors.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] for source fact count mismatch and
    /// propagates checked Arrow/IPC planning failures.
    pub(crate) fn material_plan<I>(
        &self,
        source_facts: I,
    ) -> Result<OtlpManagedMaterialPlan, ScribeError>
    where
        I: ExactSizeIterator<Item = FixedIpcColumnPlan>,
    {
        if source_facts.len() != self.source_schema.fields().len() {
            return Err(ScribeError::InvalidFrame);
        }
        let mut final_facts = [EMPTY_FACT; MAX_FINAL_FIELDS];
        let mut arrow_bytes = 0_usize;
        for (index, fact) in source_facts.enumerate() {
            if index < self.retained_source_columns {
                final_facts[index] = fact;
                arrow_bytes = add_array_bytes(
                    arrow_bytes,
                    fact,
                    self.final_schema.field(index).data_type(),
                )?;
            }
        }
        for (managed_index, fact) in self.managed_facts.iter().copied().enumerate() {
            let index = self.retained_source_columns + managed_index;
            final_facts[index] = fact;
            arrow_bytes = add_array_bytes(
                arrow_bytes,
                fact,
                self.final_schema.field(index).data_type(),
            )?;
        }
        let final_count = self.retained_source_columns + MANAGED_COLUMNS;
        let ipc_plan = FixedIpcPlan::count_schema(
            self.final_schema.as_ref(),
            self.rows,
            final_facts[..final_count].iter().copied(),
        )?;
        Ok(OtlpManagedMaterialPlan {
            rows: self.rows,
            ipc_plan,
            arrow_bytes,
        })
    }

    /// Appends exact authoritative managed arrays to retained mapper columns.
    ///
    /// The base batch must stop at `run_id`; mapper placeholder arrays must not
    /// be materialized merely to be discarded. User arrays through `run_id`
    /// are retained by `Arc` without copying.
    /// Every new physical buffer is allocated at its planned logical length and
    /// checked for capacity divergence before the final batch is constructed.
    /// The consumed material plan is returned beside the rows so WAL encoding
    /// cannot replace the pre-material count with a post-allocation recount.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] when the retained base schema/row
    /// count differs, a concrete allocation exceeds its plan, or Arrow rejects
    /// the canonical final batch. Returns a typed overflow error for ordinals.
    pub(crate) fn finish(
        &self,
        base: RecordBatch,
        material: OtlpManagedMaterialPlan,
    ) -> Result<OtlpManagedBatch, ScribeError> {
        let (base_schema, base_columns, base_rows) = base.into_parts();
        if base_rows != self.rows
            || base_columns.len() != self.retained_source_columns
            || base_schema
                .fields()
                .iter()
                .zip(self.source_schema.fields().iter())
                .any(|(actual, expected)| actual != expected)
        {
            return Err(ScribeError::InvalidFrame);
        }
        let final_count = self.retained_source_columns + MANAGED_COLUMNS;
        let mut columns = Vec::with_capacity(final_count);
        columns.extend(base_columns);
        columns.push(Arc::new(null_text(self.rows)?) as ArrayRef);
        columns.push(Arc::new(repeated_text(self.rows, self.principal_id.as_bytes())?) as ArrayRef);
        columns.push(Arc::new(repeated_text(self.rows, self.request_id.as_bytes())?) as ArrayRef);
        columns.push(Arc::new(repeated_timestamp(self.rows, self.receipt_micros)?) as ArrayRef);
        columns.push(Arc::new(repeated_timestamp(self.rows, self.receipt_micros)?) as ArrayRef);
        columns.push(Arc::new(repeated_batch_id(self.rows, self.batch_id)?) as ArrayRef);
        columns.push(Arc::new(row_ordinals(self.rows)?) as ArrayRef);
        columns.push(Arc::new(repeated_text(self.rows, self.tenant_id.as_bytes())?) as ArrayRef);
        if columns.len() != final_count || columns.capacity() != final_count {
            return Err(ScribeError::InvalidFrame);
        }
        let rows = RecordBatch::try_new(Arc::clone(&self.final_schema), columns)
            .map_err(|_| ScribeError::InvalidFrame)?;
        if rows.get_array_memory_size() != material.arrow_bytes {
            return Err(ScribeError::InvalidFrame);
        }
        Ok(OtlpManagedBatch {
            rows,
            ipc_plan: material.ipc_plan,
        })
    }
}

/// Validates the mapper tail and returns the number of retained source fields.
///
/// # Errors
///
/// Returns [`ScribeError::FingerprintMismatch`] when the user schema differs
/// from catalog authority, or [`ScribeError::InvalidFrame`] when placeholder
/// order or managed-name isolation is not canonical.
fn validate_source_schema(
    source_schema: &Schema,
    expected_fingerprint: SchemaFingerprint,
) -> Result<usize, ScribeError> {
    if projected_source_schema_fingerprint(source_schema) != expected_fingerprint {
        return Err(ScribeError::FingerprintMismatch {
            table: "resolved ingress table".to_owned(),
        });
    }
    let source_fields = source_schema.fields();
    if source_fields.len() < 3
        || source_fields
            .iter()
            .any(|field| field.name() == wyrd_spec::vala::CARD_REF)
    {
        return Err(ScribeError::InvalidFrame);
    }
    let placeholder_start = source_fields.len() - 3;
    validate_placeholder(&source_fields[placeholder_start], RUN_ID)?;
    validate_placeholder(&source_fields[placeholder_start + 1], CARD_UID)?;
    validate_placeholder(&source_fields[placeholder_start + 2], PRINCIPAL_ID)?;
    if source_fields[..placeholder_start].iter().any(|field| {
        matches!(
            field.name().as_str(),
            RUN_ID
                | CARD_UID
                | PRINCIPAL_ID
                | WYRD_REQUEST_ID
                | WYRD_EVENT_TIME
                | WYRD_INGESTED_AT
                | WYRD_BATCH_ID
                | WYRD_ROW_ORDINAL
                | DATA_TENANT_ID
        )
    }) {
        return Err(ScribeError::InvalidFrame);
    }
    Ok(placeholder_start + 1)
}

/// Builds the canonical final schema from retained mapper fields.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when the fixed field bound or exact
/// descriptor capacity is violated, and a typed overflow error on arithmetic
/// overflow.
fn build_final_schema(
    source_schema: &Schema,
    retained_source_columns: usize,
) -> Result<Arc<Schema>, ScribeError> {
    let final_count = retained_source_columns
        .checked_add(MANAGED_COLUMNS)
        .ok_or_else(material_overflow)?;
    if final_count > MAX_FINAL_FIELDS {
        return Err(ScribeError::InvalidFrame);
    }
    let mut final_fields = Vec::with_capacity(final_count);
    let retained_fields = source_schema
        .fields()
        .get(..retained_source_columns)
        .ok_or(ScribeError::InvalidFrame)?;
    final_fields.extend(retained_fields.iter().map(Arc::clone));
    final_fields.extend([
        Arc::new(Field::new(CARD_UID, DataType::Utf8, true)),
        Arc::new(Field::new(PRINCIPAL_ID, DataType::Utf8, false)),
        Arc::new(Field::new(WYRD_REQUEST_ID, DataType::Utf8, false)),
        Arc::new(Field::new(
            WYRD_EVENT_TIME,
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        )),
        Arc::new(Field::new(
            WYRD_INGESTED_AT,
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        )),
        Arc::new(Field::new(
            WYRD_BATCH_ID,
            DataType::FixedSizeBinary(16),
            false,
        )),
        Arc::new(Field::new(WYRD_ROW_ORDINAL, DataType::Int32, false)),
        Arc::new(Field::new(DATA_TENANT_ID, DataType::Utf8, false)),
    ]);
    if final_fields.len() != final_count || final_fields.capacity() != final_count {
        return Err(ScribeError::InvalidFrame);
    }
    Ok(Arc::new(Schema::new(final_fields)))
}

/// Counts exact physical facts for authoritative managed columns.
///
/// # Errors
///
/// Returns a checked overflow error when offsets or repeated values cannot fit.
fn managed_facts(
    rows: usize,
    identity: &ManagedIdentity,
) -> Result<[FixedIpcColumnPlan; MANAGED_COLUMNS], ScribeError> {
    Ok([
        FixedIpcColumnPlan {
            null_count: rows,
            validity_bytes: bitmap_bytes(rows)?,
            offsets_bytes: offsets_bytes(rows)?,
            values_bytes: 0,
        },
        text_fact(rows, identity.principal.as_bytes().len())?,
        text_fact(rows, identity.request.as_bytes().len())?,
        fixed_fact(rows, 8)?,
        fixed_fact(rows, 8)?,
        fixed_fact(rows, 16)?,
        fixed_fact(rows, 4)?,
        text_fact(rows, identity.tenant.as_bytes().len())?,
    ])
}

/// Validates one nullable UTF-8 mapper placeholder field.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when name, type, or nullability differs.
fn validate_placeholder(field: &Field, name: &str) -> Result<(), ScribeError> {
    if field.name() != name || field.data_type() != &DataType::Utf8 || !field.is_nullable() {
        return Err(ScribeError::InvalidFrame);
    }
    Ok(())
}

/// Returns exact facts for one repeated non-null UTF-8 value.
///
/// # Errors
///
/// Returns a checked overflow error when offsets or values cannot fit.
fn text_fact(rows: usize, value_bytes: usize) -> Result<FixedIpcColumnPlan, ScribeError> {
    Ok(FixedIpcColumnPlan {
        null_count: 0,
        validity_bytes: 0,
        offsets_bytes: offsets_bytes(rows)?,
        values_bytes: rows
            .checked_mul(value_bytes)
            .ok_or_else(material_overflow)?,
    })
}

/// Returns exact facts for one non-null fixed-width managed column.
///
/// # Errors
///
/// Returns a checked overflow error when values cannot fit.
fn fixed_fact(rows: usize, width: usize) -> Result<FixedIpcColumnPlan, ScribeError> {
    Ok(FixedIpcColumnPlan {
        null_count: 0,
        validity_bytes: 0,
        offsets_bytes: 0,
        values_bytes: rows.checked_mul(width).ok_or_else(material_overflow)?,
    })
}

/// Adds one column's public Arrow array owner and exact buffer capacities.
///
/// # Errors
///
/// Returns a checked overflow error when the aggregate cannot fit.
fn add_array_bytes(
    total: usize,
    fact: FixedIpcColumnPlan,
    data_type: &DataType,
) -> Result<usize, ScribeError> {
    total
        .checked_add(array_owner_bytes(data_type)?)
        .and_then(|value| value.checked_add(fact.validity_bytes))
        .and_then(|value| value.checked_add(fact.offsets_bytes))
        .and_then(|value| value.checked_add(fact.values_bytes))
        .ok_or_else(material_overflow)
}

/// Returns the public concrete Arrow array owner size for one scalar type.
///
/// Arrow's `get_array_memory_size` adds the concrete array owner to buffer
/// capacities. All primitive aliases share one `PrimitiveArray<T>` layout,
/// and all byte-array aliases share one `GenericByteArray<T>` layout.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] for a type outside the fixed IPC
/// encoder's canonical scalar subset.
fn array_owner_bytes(data_type: &DataType) -> Result<usize, ScribeError> {
    match data_type {
        DataType::Null => Ok(std::mem::size_of::<NullArray>()),
        DataType::Boolean => Ok(std::mem::size_of::<BooleanArray>()),
        DataType::Binary | DataType::LargeBinary | DataType::Utf8 | DataType::LargeUtf8 => {
            Ok(std::mem::size_of::<StringArray>())
        }
        DataType::FixedSizeBinary(width) if *width > 0 => {
            Ok(std::mem::size_of::<FixedSizeBinaryArray>())
        }
        DataType::Int8
        | DataType::Int16
        | DataType::Int32
        | DataType::Int64
        | DataType::UInt8
        | DataType::UInt16
        | DataType::UInt32
        | DataType::UInt64
        | DataType::Float16
        | DataType::Float32
        | DataType::Float64
        | DataType::Date32
        | DataType::Date64
        | DataType::Time32(_)
        | DataType::Time64(_)
        | DataType::Timestamp(_, _)
        | DataType::Duration(_)
        | DataType::Interval(_)
        | DataType::Decimal128(_, _)
        | DataType::Decimal256(_, _) => Ok(std::mem::size_of::<Int32Array>()),
        _ => Err(ScribeError::InvalidFrame),
    }
}

/// Constructs an all-null UTF-8 array with exact offsets and validity storage.
///
/// # Errors
///
/// Returns a checked allocation/capacity error.
fn null_text(rows: usize) -> Result<StringArray, ScribeError> {
    let offset_count = rows.checked_add(1).ok_or_else(material_overflow)?;
    let offsets = vec![0_i32; offset_count];
    let validity_bytes = bitmap_bytes(rows)?;
    let validity = vec![0_u8; validity_bytes];
    ensure_capacity(&offsets, offset_count)?;
    ensure_capacity(&validity, validity_bytes)?;
    let nulls = NullBuffer::new(BooleanBuffer::new(Buffer::from(validity), 0, rows));
    Ok(StringArray::new(
        OffsetBuffer::new(ScalarBuffer::from(offsets)),
        Buffer::from(Vec::<u8>::new()),
        Some(nulls),
    ))
}

/// Constructs one repeated non-null UTF-8 array at exact capacities.
///
/// # Errors
///
/// Returns a checked overflow, offset conversion, or capacity error.
fn repeated_text(rows: usize, value: &[u8]) -> Result<StringArray, ScribeError> {
    let values_bytes = rows
        .checked_mul(value.len())
        .ok_or_else(material_overflow)?;
    let mut offsets = Vec::with_capacity(rows + 1);
    let mut values = Vec::with_capacity(values_bytes);
    offsets.push(0_i32);
    for row in 0..rows {
        values.extend_from_slice(value);
        let end = (row + 1)
            .checked_mul(value.len())
            .ok_or_else(material_overflow)?;
        offsets.push(i32::try_from(end).map_err(|_| ScribeError::InvalidFrame)?);
    }
    ensure_capacity(&offsets, rows + 1)?;
    ensure_capacity(&values, values_bytes)?;
    Ok(StringArray::new(
        OffsetBuffer::new(ScalarBuffer::from(offsets)),
        Buffer::from(values),
        None,
    ))
}

/// Constructs one repeated UTC microsecond timestamp at exact capacity.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when allocation capacity diverges.
fn repeated_timestamp(
    rows: usize,
    receipt_micros: i64,
) -> Result<TimestampMicrosecondArray, ScribeError> {
    let mut values = Vec::with_capacity(rows);
    values.resize(rows, receipt_micros);
    ensure_capacity(&values, rows)?;
    Ok(TimestampMicrosecondArray::new(ScalarBuffer::from(values), None).with_timezone("UTC"))
}

/// Constructs repeated sixteen-byte batch identity storage at exact capacity.
///
/// # Errors
///
/// Returns a checked overflow, capacity, or Arrow physical-layout error.
fn repeated_batch_id(rows: usize, batch_id: [u8; 16]) -> Result<FixedSizeBinaryArray, ScribeError> {
    let bytes = rows.checked_mul(16).ok_or_else(material_overflow)?;
    let mut values = Vec::with_capacity(bytes);
    for _ in 0..rows {
        values.extend_from_slice(&batch_id);
    }
    ensure_capacity(&values, bytes)?;
    FixedSizeBinaryArray::try_new(16, Buffer::from(values), None)
        .map_err(|_| ScribeError::InvalidFrame)
}

/// Constructs stable zero-based row ordinals at exact capacity.
///
/// # Errors
///
/// Returns [`ScribeError::TooManyRows`] when an ordinal exceeds `i32` and
/// [`ScribeError::InvalidFrame`] when allocation capacity diverges.
fn row_ordinals(rows: usize) -> Result<Int32Array, ScribeError> {
    let mut values = Vec::with_capacity(rows);
    for ordinal in 0..rows {
        values.push(
            i32::try_from(ordinal).map_err(|_| ScribeError::TooManyRows {
                rows: u64::try_from(rows).unwrap_or(u64::MAX),
                limit: (i32::MAX - 1) as u64,
            })?,
        );
    }
    ensure_capacity(&values, rows)?;
    Ok(Int32Array::new(ScalarBuffer::from(values), None))
}

/// Verifies one exact-capacity vector did not grow or over-reserve.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when logical length or capacity differs.
fn ensure_capacity<T>(values: &Vec<T>, expected: usize) -> Result<(), ScribeError> {
    if values.len() != expected || values.capacity() != expected {
        return Err(ScribeError::InvalidFrame);
    }
    Ok(())
}

/// Returns exact 32-bit offsets bytes for `rows` UTF-8 values.
///
/// # Errors
///
/// Returns a checked overflow error when the buffer cannot fit.
fn offsets_bytes(rows: usize) -> Result<usize, ScribeError> {
    rows.checked_add(1)
        .and_then(|count| count.checked_mul(4))
        .ok_or_else(material_overflow)
}

/// Returns exact validity bitmap bytes for `rows` logical values.
///
/// # Errors
///
/// Returns a checked overflow error when the bitmap cannot fit.
fn bitmap_bytes(rows: usize) -> Result<usize, ScribeError> {
    rows.checked_add(7)
        .map(|value| value / 8)
        .ok_or_else(material_overflow)
}

/// Creates the private stable overflow result for managed planning.
#[must_use]
fn material_overflow() -> ScribeError {
    ScribeError::DecodedPayloadTooLarge {
        bytes: usize::MAX,
        limit: usize::MAX - 1,
    }
}

#[cfg(test)]
mod tests {
    use arrow::array::{Array, Int64Array};
    use wyrd_runtime::permission::PermissionSet;
    use wyrd_runtime::principal::{PrincipalId, PrincipalKind};
    use wyrd_spec::DataTenantId;

    use super::*;

    /// Constructs one deterministic user principal for managed stamping tests.
    fn principal() -> Principal {
        Principal::new(
            PrincipalId::new(uuid::Uuid::from_u128(1)),
            PrincipalKind::User,
            DataTenantId::new_v7(),
            Vec::new(),
            PermissionSet::new(),
        )
    }

    /// Constructs the mapper schema with its required correlation placeholders.
    fn source_schema() -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("value", DataType::Int64, false),
            Field::new(RUN_ID, DataType::Utf8, true),
            Field::new(CARD_UID, DataType::Utf8, true),
            Field::new(PRINCIPAL_ID, DataType::Utf8, true),
        ]))
    }

    /// Builds one base mapper batch with exact nullable placeholders.
    fn source_batch(schema: &Schema) -> RecordBatch {
        let retained_schema = Arc::new(Schema::new(
            schema.fields()[..2]
                .iter()
                .map(Arc::clone)
                .collect::<Vec<_>>(),
        ));
        let run_id = StringArray::new(
            OffsetBuffer::new(ScalarBuffer::from(vec![0_i32, 3_i32, 3_i32])),
            Buffer::from(vec![b'r', b'u', b'n']),
            Some(NullBuffer::new(BooleanBuffer::new(
                Buffer::from(vec![1_u8]),
                0,
                2,
            ))),
        );
        RecordBatch::try_new(
            retained_schema,
            vec![
                Arc::new(Int64Array::from(vec![7_i64, 8_i64])) as ArrayRef,
                Arc::new(run_id),
            ],
        )
        .expect("valid source batch")
    }

    /// Returns exact source facts for the deterministic two-row mapper batch.
    fn source_facts() -> [FixedIpcColumnPlan; 4] {
        [
            FixedIpcColumnPlan {
                null_count: 0,
                validity_bytes: 0,
                offsets_bytes: 0,
                values_bytes: 16,
            },
            FixedIpcColumnPlan {
                null_count: 1,
                validity_bytes: 1,
                offsets_bytes: 12,
                values_bytes: 3,
            },
            FixedIpcColumnPlan {
                null_count: 2,
                validity_bytes: 1,
                offsets_bytes: 12,
                values_bytes: 0,
            },
            FixedIpcColumnPlan {
                null_count: 2,
                validity_bytes: 1,
                offsets_bytes: 12,
                values_bytes: 0,
            },
        ]
    }

    /// Proves final schema order, source retention, and authoritative values.
    #[test]
    fn managed_projection_preserves_canonical_order_and_source_arrays() {
        let source = source_schema();
        let fingerprint = projected_source_schema_fingerprint(source.as_ref());
        let request_id = RequestId::now_v7();
        let projection = OtlpManagedProjection::plan(
            Arc::clone(&source),
            fingerprint,
            2,
            &principal(),
            &request_id,
            uuid::Uuid::from_u128(3),
            42,
        )
        .expect("managed plan");
        let base = source_batch(&source);
        let retained = Arc::clone(base.column(0));
        let material = projection
            .material_plan(source_facts().into_iter())
            .expect("material plan");
        let final_batch = projection
            .finish(base, material)
            .expect("managed batch")
            .rows;

        assert!(Arc::ptr_eq(&retained, final_batch.column(0)));
        assert_eq!(final_batch.schema_ref(), projection.schema());
        assert_eq!(
            final_batch
                .schema()
                .fields()
                .iter()
                .map(|field| field.name().as_str())
                .collect::<Vec<_>>(),
            vec![
                "value",
                RUN_ID,
                CARD_UID,
                PRINCIPAL_ID,
                WYRD_REQUEST_ID,
                WYRD_EVENT_TIME,
                WYRD_INGESTED_AT,
                WYRD_BATCH_ID,
                WYRD_ROW_ORDINAL,
                DATA_TENANT_ID,
            ]
        );
        assert_eq!(final_batch.column(2).logical_null_count(), 2);
        let event = final_batch
            .column(5)
            .as_any()
            .downcast_ref::<TimestampMicrosecondArray>()
            .expect("event timestamp");
        assert_eq!(event.values(), &[42, 42]);
    }

    /// Proves source fingerprint mismatch refuses before managed construction.
    #[test]
    fn managed_projection_rejects_source_fingerprint_mismatch() {
        let source = source_schema();
        let error = OtlpManagedProjection::plan(
            source,
            SchemaFingerprint([0_u8; 32]),
            2,
            &principal(),
            &RequestId::now_v7(),
            uuid::Uuid::from_u128(3),
            42,
        )
        .err()
        .expect("fingerprint refusal");

        assert!(matches!(error, ScribeError::FingerprintMismatch { .. }));
    }

    /// Proves combined Arrow and IPC planning uses retained plus managed facts.
    #[test]
    fn managed_material_plan_is_exact_for_finished_batch() {
        let source = source_schema();
        let projection = OtlpManagedProjection::plan(
            Arc::clone(&source),
            projected_source_schema_fingerprint(source.as_ref()),
            2,
            &principal(),
            &RequestId::now_v7(),
            uuid::Uuid::from_u128(3),
            42,
        )
        .expect("managed plan");
        let material = projection
            .material_plan(source_facts().into_iter())
            .expect("combined material plan");
        let planned_arrow = material.arrow_bytes;
        let exact_limit = material
            .admitted_bytes(usize::MAX)
            .expect("combined exact bytes");
        assert_eq!(
            material
                .admitted_bytes(exact_limit)
                .expect("exact C accepted"),
            exact_limit
        );
        assert!(matches!(
            material.admitted_bytes(exact_limit - 1),
            Err(ScribeError::DecodedPayloadTooLarge { .. })
        ));
        let managed = projection
            .finish(source_batch(&source), material)
            .expect("managed batch");
        let encoded = managed.ipc_plan.encode(&managed.rows).expect("fixed IPC");

        assert_eq!(encoded.len(), managed.ipc_plan.encoded_bytes());
        assert_eq!(planned_arrow, managed.rows.get_array_memory_size());
    }
}
