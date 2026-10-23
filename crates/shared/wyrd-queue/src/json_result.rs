//! Bounded positional JSON projection of one complete Bifrost query result.
//!
//! [`JsonResultBuilder`] turns an accepted Arrow schema, its record batches,
//! and the validated terminal into the single `{columns, rows, terminal}`
//! value agent and UI surfaces return. The server's MCP `bifrost.query` tool
//! and the TypeScript SDK's bounded collection both build through it, so the
//! two surfaces produce the same value for the same stream by construction.
//! Stream validation, cancellation, and settlement stay with each caller.

use std::sync::Arc;

use arrow::datatypes::Schema;
use arrow::json::writer::{EncoderOptions, make_encoder};
use arrow::record_batch::RecordBatch;
use serde_json::Value as JsonValue;
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::api::QueryTerminalFrame;

use crate::variant::WyrdJsonEncoderFactory;

/// Rows returned when a caller names no row ceiling.
pub const DEFAULT_MAX_ROWS: u32 = 1_000;

/// Largest row ceiling a bounded caller may ask for.
///
/// Deliberately far below the server's own million-row sync ceiling: a
/// bounded result is one JSON value an agent or a page has to hold whole, so
/// the binding constraint is the reader, not the engine.
pub const MAX_ROWS_CEILING: u32 = 10_000;

/// Serialized result bytes returned when a caller names no byte ceiling.
pub const DEFAULT_MAX_BYTES: usize = 4 * 1024 * 1024;

/// Largest byte ceiling a bounded caller may ask for, well under the server's 256 MiB.
pub const MAX_BYTES_CEILING: usize = 16 * 1024 * 1024;

/// Compact-JSON bytes the result object costs before any content is charged.
///
/// `{"columns":`, `,"rows":`, the row array's `[` and `]`, `,"terminal":`, and
/// the closing `}` are structural and always present, so they are charged once
/// up front and the columns, each row plus its separating comma, and the
/// terminal are charged as they are produced.
pub const STRUCTURED_OVERHEAD_BYTES: usize = 11 + 8 + 2 + 12 + 1;

/// A requested result ceiling outside its admitted range.
///
/// Admission is a request refusal, not a result overflow: each surface projects
/// it through its own invalid-input envelope before reading any rows.
#[derive(Debug)]
pub struct RefusedCeiling {
    /// Fixed reason naming the offending ceiling.
    pub reason: &'static str,
    /// The offending value and its upper bound.
    pub details: JsonValue,
}

/// Admit the row and byte ceilings one bounded caller requested.
///
/// This is the single range policy every bounded surface applies before
/// collecting: rows in `1..=MAX_ROWS_CEILING` and bytes in
/// `1..=MAX_BYTES_CEILING`. Out-of-range values are refused, never clamped.
///
/// # Errors
///
/// Returns [`RefusedCeiling`] when either ceiling is zero or above its maximum.
pub fn admit_ceilings(max_rows: u32, max_bytes: usize) -> Result<(), RefusedCeiling> {
    if max_rows == 0 || max_rows > MAX_ROWS_CEILING {
        return Err(RefusedCeiling {
            reason: "max_rows is outside the allowed range",
            details: serde_json::json!({ "max_rows": max_rows, "ceiling": MAX_ROWS_CEILING }),
        });
    }
    if max_bytes == 0 || max_bytes > MAX_BYTES_CEILING {
        return Err(RefusedCeiling {
            reason: "max_bytes is outside the allowed range",
            details: serde_json::json!({ "max_bytes": max_bytes, "ceiling": MAX_BYTES_CEILING }),
        });
    }
    Ok(())
}

/// Accumulates one bounded result and charges its exact compact-JSON size.
///
/// The builder owns the two caller ceilings and the running byte count, so a
/// result that would cross either ceiling is refused before the offending
/// value is retained: overflow is always [`BifrostError::QueryResultTooLarge`],
/// never a truncated success.
#[derive(Debug)]
pub struct JsonResultBuilder {
    /// Row ceiling the caller asked for. Exceeding it fails; nothing truncates.
    max_rows: usize,
    /// Exact compact-JSON byte ceiling for `{columns, rows, terminal}`.
    max_bytes: usize,
    /// Exact bytes charged so far, starting at the fixed structural overhead.
    bytes: usize,
    /// Projected columns of the accepted schema, in engine order.
    columns: JsonValue,
    /// Positional rows retained so far.
    rows: Vec<JsonValue>,
}

impl JsonResultBuilder {
    /// Start an empty result bounded by `max_rows` rows and `max_bytes` bytes.
    #[must_use]
    pub fn new(max_rows: usize, max_bytes: usize) -> Self {
        Self {
            max_rows,
            max_bytes,
            bytes: STRUCTURED_OVERHEAD_BYTES,
            columns: JsonValue::Array(Vec::new()),
            rows: Vec::new(),
        }
    }

    /// Project the accepted Arrow schema as ordered `{name, data_type, nullable}`.
    ///
    /// The schema is accepted exactly once, so columns occur once in the result
    /// no matter how many batches follow. Duplicate column names are preserved
    /// rather than deduplicated, which is why rows are positional arrays: an
    /// object row model cannot represent a result the engine can legally
    /// produce. `data_type` is arrow-rs's `DataType` display.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryResultTooLarge`] when the columns exceed
    /// the byte ceiling, and [`WyrdError::Internal`] when they cannot be
    /// measured.
    pub fn accept_schema(&mut self, schema: &Schema) -> Result<(), WyrdError> {
        let columns = schema
            .fields()
            .iter()
            .map(|field| {
                serde_json::json!({
                    "name": field.name(),
                    "data_type": field.data_type().to_string(),
                    "nullable": field.is_nullable(),
                })
            })
            .collect::<JsonValue>();
        self.charge(&columns)?;
        self.columns = columns;
        Ok(())
    }

    /// Append one decoded batch's rows as positional JSON arrays.
    ///
    /// Each value uses Arrow's own JSON encoding through
    /// [`WyrdJsonEncoderFactory`]; a null slot is JSON `null`. Each row and its
    /// separating comma are charged before the row is retained.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryResultTooLarge`] when the batch would carry
    /// the result past either ceiling, and [`WyrdError::Internal`] when a
    /// column has no Arrow JSON encoder or encodes unreadable JSON.
    pub fn accept_batch(&mut self, batch: &RecordBatch) -> Result<(), WyrdError> {
        if self.rows.len().saturating_add(batch.num_rows()) > self.max_rows {
            return Err(BifrostError::QueryResultTooLarge.into());
        }
        let schema = batch.schema();
        let options =
            EncoderOptions::default().with_encoder_factory(Arc::new(WyrdJsonEncoderFactory));
        let mut encoders = Vec::with_capacity(batch.num_columns());
        for (field, array) in schema.fields().iter().zip(batch.columns()) {
            encoders.push(
                make_encoder(field, array.as_ref(), &options).map_err(|error| {
                    internal(format!("cannot encode column {}: {error}", field.name()))
                })?,
            );
        }
        let mut buffer = Vec::new();
        for index in 0..batch.num_rows() {
            let mut row = Vec::with_capacity(encoders.len());
            for encoder in &mut encoders {
                if encoder.is_null(index) {
                    row.push(JsonValue::Null);
                    continue;
                }
                buffer.clear();
                encoder.encode(index, &mut buffer);
                row.push(serde_json::from_slice(&buffer).map_err(|error| {
                    internal(format!("produced an unreadable Arrow JSON value: {error}"))
                })?);
            }
            let row = JsonValue::Array(row);
            if !self.rows.is_empty() {
                self.charge_bytes(1)?;
            }
            self.charge(&row)?;
            self.rows.push(row);
        }
        Ok(())
    }

    /// Charge the settled terminal and return the complete result.
    ///
    /// The terminal is projected as the evidence a reader can act on; the
    /// Arrow end-of-stream delta and the error slot are deliberately absent
    /// because the caller has already consumed the framing and refused a
    /// failed terminal.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryResultTooLarge`] when the terminal exceeds
    /// the byte ceiling, and [`WyrdError::Internal`] when it cannot be
    /// measured.
    pub fn finish(mut self, terminal: &QueryTerminalFrame) -> Result<JsonValue, WyrdError> {
        let terminal = serde_json::json!({
            "outcome": terminal.outcome,
            "query_class": terminal.query_class,
            "row_count": terminal.row_count,
            "warnings": terminal.warnings,
            "source_completion": terminal.source_completion,
        });
        self.charge(&terminal)?;
        Ok(serde_json::json!({
            "columns": self.columns,
            "rows": self.rows,
            "terminal": terminal,
        }))
    }

    /// Charge one value's exact compact-JSON length against the budget.
    ///
    /// The value is serialized into a temporary buffer purely to measure it, so
    /// the caller can refuse it before retaining it.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdError::Internal`] when the value cannot be serialized, and
    /// [`BifrostError::QueryResultTooLarge`] when charging it exceeds the ceiling.
    fn charge(&mut self, value: &JsonValue) -> Result<(), WyrdError> {
        let measured = serde_json::to_vec(value)
            .map_err(|error| internal(format!("could not measure its own result: {error}")))?;
        self.charge_bytes(measured.len())
    }

    /// Charge an exact byte count, refusing overflow of the count or ceiling.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryResultTooLarge`] when the running count
    /// would overflow or would exceed the caller's byte ceiling.
    fn charge_bytes(&mut self, amount: usize) -> Result<(), WyrdError> {
        let charged = self
            .bytes
            .checked_add(amount)
            .ok_or(BifrostError::QueryResultTooLarge)?;
        if charged > self.max_bytes {
            return Err(BifrostError::QueryResultTooLarge.into());
        }
        self.bytes = charged;
        Ok(())
    }
}

/// Internal projection failure naming the bounded result as its source.
fn internal(message: String) -> WyrdError {
    WyrdError::Internal {
        message: format!("bounded query result {message}"),
        details: serde_json::json!({}),
    }
}
