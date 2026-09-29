//! The scoped observation surface: one invocation, Card-scoped views, three emits.
//!
//! [`Run`] is one application invocation. It is opened locally — no network IO,
//! no server-side Run resource — and carries the `run_id` and the observed
//! subject `CardRef` that every observation sends as Bifrost row correlation.
//! [`Run::for_card`] returns an immutable sibling view over the same invocation,
//! so concurrent views never disturb each other's subject.
//!
//! The Drift and Eval projections live here rather than in
//! [`crate::bifrost`]: the facade, the producer pool, and `wyrd-queue` stay
//! generic plumbing that buffers JSON rows against a described schema and never
//! interprets a Verifier kind. Every SDK projects this one Rust surface, so the
//! three languages cannot drift into three projections.

pub mod drift;
pub mod eval;
pub mod lifecycle;
#[cfg(test)]
mod tests;

use arrow_schema::DataType;
use serde::Serialize;
use serde_json::{Value, json};
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::ids::{RunId, SessionId};

use crate::bifrost::{Correlation, WriterTable};
use crate::state::WyrdState;

pub use eval::EvalObservationOptions;
pub use lifecycle::{DRIFT_OBSERVATIONS_TABLE, EVAL_OBSERVATIONS_TABLE};

/// The only namespace `observe.record` may write.
///
/// `vala.datasets` is the caller-owned registration namespace; every other
/// Bifrost namespace is reserved for server-managed tables. Refusing the rest
/// here fails the call before admission instead of spending a describe and a
/// queue slot on a table Gate would reject.
const DATASETS_PREFIX: &str = "vala.datasets.";

/// One application invocation, optionally scoped to a registered Card.
///
/// Cloning a run is cloning its view: the `run_id` and the state-owned Bifrost
/// writer are shared, the subject is not.
#[derive(Debug, Clone)]
pub struct Run {
    /// The hydrated graph and the one Bifrost lifetime this run emits through.
    state: WyrdState,
    /// The invocation identity every observation of every view correlates to.
    run_id: RunId,
    /// The exact Card this view observes; the root Service until `for_card`.
    subject: CardRef,
}

impl Run {
    /// Open a run over `state`, targeting its root Service Card.
    pub(crate) fn new(state: WyrdState) -> Self {
        let subject = state.root_ref().clone();
        Self {
            state,
            run_id: RunId::new(),
            subject,
        }
    }

    /// The invocation identity shared by this run and every view of it.
    #[must_use]
    pub fn run_id(&self) -> &RunId {
        &self.run_id
    }

    /// The exact Card this view observes.
    #[must_use]
    pub fn card_ref(&self) -> &CardRef {
        &self.subject
    }

    /// An immutable sibling view scoped to `alias` in the same hydrated graph.
    ///
    /// The parent view is unchanged, so a Service run and its Model and Agent
    /// views may be used concurrently. Resolution is a local index lookup: an
    /// unknown or out-of-graph alias fails without any network IO and never
    /// falls back to the root Service.
    ///
    /// # Errors
    /// Returns `WYRD_SDK_404_UNKNOWN_ALIAS` when the alias is not registered in
    /// this bundle.
    pub fn for_card(&self, alias: &str) -> Result<Self, WyrdError> {
        Ok(Self {
            state: self.state.clone(),
            run_id: self.run_id.clone(),
            subject: self.state.card_ref(alias)?.clone(),
        })
    }

    /// The emit surface for this view.
    #[must_use]
    pub fn observe(&self) -> Observe<'_> {
        Observe { run: self }
    }

    /// This view's row correlation: subject Card plus invocation.
    fn correlation(&self) -> Correlation {
        Correlation {
            card_ref: Some(self.subject.clone()),
            run_id: Some(self.run_id.clone()),
        }
    }
}

/// The three observation emits available on one scoped run.
///
/// Drift and Eval are synchronous: their fixed tables were described at
/// `start_bifrost`, so the call only projects and enqueues. Enqueueing is not a
/// durability acknowledgement — the queue publishes in the background and
/// `WyrdState::shutdown` is the barrier.
#[derive(Debug)]
pub struct Observe<'a> {
    /// The view whose subject and invocation every emit correlates to.
    run: &'a Run,
}

impl Observe<'_> {
    /// Emit one Drift observation from a `Serialize` feature object.
    ///
    /// # Errors
    /// Returns `WYRD_SDK_400_BIFROST_NOT_STARTED` before startup,
    /// `WYRD_SDK_409_BIFROST_CLOSED` after shutdown,
    /// `WYRD_SDK_400_INVALID_OBSERVATION` when the input is not a flat object of
    /// supported scalars, and `WYRD_CLIENT_429_QUEUE_FULL` when the producer is
    /// saturated.
    pub fn drift<T: Serialize>(
        &self,
        features: &T,
        session_id: Option<SessionId>,
    ) -> Result<(), WyrdError> {
        self.drift_value(&to_value(features, "drift features")?, session_id)
    }

    /// Emit one Drift observation from a foreign runtime's JSON text.
    ///
    /// The Python and TypeScript boundaries hold a JSON document, not a Rust
    /// type, and reach the same projection through this door.
    ///
    /// # Errors
    /// As [`Observe::drift`], plus an invalid-observation error when `json` is
    /// not valid JSON.
    pub fn drift_json(&self, json: &str, session_id: Option<SessionId>) -> Result<(), WyrdError> {
        self.drift_value(&parse_json(json, "drift features")?, session_id)
    }

    /// Project and enqueue one Drift observation.
    ///
    /// Input validation runs before the lifecycle lookup so a malformed feature
    /// map reports what is wrong with it rather than reporting that Bifrost has
    /// not started.
    ///
    /// # Errors
    /// As [`Observe::drift`].
    fn drift_value(
        &self,
        features: &Value,
        session_id: Option<SessionId>,
    ) -> Result<(), WyrdError> {
        let record = drift::observation(features, session_id)?;
        let started = self.run.state.started_bifrost()?;
        let correlation = self.run.correlation();
        for row in drift::rows(&record)? {
            started
                .bifrost
                .insert_into(&started.drift, row, correlation.clone())?;
        }
        Ok(())
    }

    /// Emit one Eval observation from a `Serialize` context value.
    ///
    /// # Errors
    /// Returns `WYRD_SDK_400_BIFROST_NOT_STARTED` before startup,
    /// `WYRD_SDK_409_BIFROST_CLOSED` after shutdown, a validation error when
    /// `span_id` is supplied without `trace_id`,
    /// `WYRD_SDK_400_INVALID_OBSERVATION` when the context or media cannot be
    /// encoded, and `WYRD_CLIENT_429_QUEUE_FULL` when the producer is saturated.
    pub fn eval<T: Serialize>(
        &self,
        context: &T,
        options: EvalObservationOptions,
    ) -> Result<(), WyrdError> {
        self.eval_value(to_value(context, "eval context")?, options)
    }

    /// Emit one Eval observation from a foreign runtime's JSON text.
    ///
    /// # Errors
    /// As [`Observe::eval`], plus an invalid-observation error when `json` is
    /// not valid JSON.
    pub fn eval_json(&self, json: &str, options: EvalObservationOptions) -> Result<(), WyrdError> {
        self.eval_value(parse_json(json, "eval context")?, options)
    }

    /// Project and enqueue one Eval observation.
    ///
    /// # Errors
    /// As [`Observe::eval`].
    fn eval_value(&self, context: Value, options: EvalObservationOptions) -> Result<(), WyrdError> {
        let record = eval::observation(context, options)?;
        let started = self.run.state.started_bifrost()?;
        started
            .bifrost
            .insert_into(&started.eval, eval::row(&record)?, self.run.correlation())?;
        Ok(())
    }

    /// Emit one row into a registered caller-owned table.
    ///
    /// Async because the first call for a table describes it; later calls reuse
    /// the writer's cached schema and producer with no schema lookup. Returning
    /// is neither a durability nor a whole-row-validation acknowledgement: the
    /// queue checks row values against the described schema when it seals a
    /// batch.
    ///
    /// # Errors
    /// Returns `WYRD_SDK_400_INVALID_OBSERVATION` for a table outside
    /// `vala.datasets`, which is where reserved and system-managed tables are
    /// refused before admission; the server's not-found, authorization, or
    /// availability error for an unknown, unauthorized, or unavailable table;
    /// and the lifecycle and queue errors of [`Observe::drift`].
    pub async fn record<T: Serialize>(&self, table: &str, row: &T) -> Result<(), WyrdError> {
        self.record_value(table, &to_value(row, "record row")?)
            .await
    }

    /// Emit one row into a registered table from a foreign runtime's JSON text.
    ///
    /// # Errors
    /// As [`Observe::record`], plus an invalid-observation error when `json` is
    /// not valid JSON.
    pub async fn record_json(&self, table: &str, json: &str) -> Result<(), WyrdError> {
        self.record_value(table, &parse_json(json, "record row")?)
            .await
    }

    /// Describe on first use, then enqueue one caller row.
    ///
    /// # Errors
    /// As [`Observe::record`].
    async fn record_value(&self, table: &str, row: &Value) -> Result<(), WyrdError> {
        if !table.starts_with(DATASETS_PREFIX) || table[DATASETS_PREFIX.len()..].contains('.') {
            return Err(invalid_observation(
                "observe.record writes only registered `vala.datasets.<name>` tables",
                json!({ "table": table }),
            ));
        }
        let started = self.run.state.started_bifrost()?;
        let destination = started.bifrost.writer_table(table).await?;
        started
            .bifrost
            .insert_into(&destination, row_bytes(row)?, self.run.correlation())?;
        Ok(())
    }
}

/// Serialize the bytes of one projected row.
///
/// # Errors
/// Returns `WYRD_SDK_400_INVALID_OBSERVATION` when the projected row is not
/// serializable, which the projections' own validation already precludes.
fn row_bytes(row: &Value) -> Result<Vec<u8>, WyrdError> {
    serde_json::to_vec(row).map_err(|error| {
        invalid_observation(
            "projected row is not serializable",
            json!({ "source": error.to_string() }),
        )
    })
}

/// Convert one caller value into JSON, refusing what JSON cannot carry.
///
/// # Errors
/// Returns `WYRD_SDK_400_INVALID_OBSERVATION` when the value is not
/// serializable, which is how a non-finite float is refused before projection.
fn to_value<T: Serialize>(value: &T, field: &str) -> Result<Value, WyrdError> {
    serde_json::to_value(value).map_err(|error| {
        invalid_observation(
            "observation input is not serializable as JSON",
            json!({ "field": field, "source": error.to_string() }),
        )
    })
}

/// Parse a foreign runtime's JSON text.
///
/// # Errors
/// Returns `WYRD_SDK_400_INVALID_OBSERVATION` when the text is not valid JSON.
fn parse_json(text: &str, field: &str) -> Result<Value, WyrdError> {
    serde_json::from_str(text).map_err(|error| {
        invalid_observation(
            "observation input is not valid JSON",
            json!({ "field": field, "source": error.to_string() }),
        )
    })
}

/// One authored column a fixed projection writes: name, Arrow type, nullability.
type ProjectedColumn = (&'static str, DataType, bool);

/// Verify a described table's authored fields are exactly a projection's.
///
/// The comparison is the complete ordered sequence — count, then each field's
/// name, Arrow type, and nullability — so an extra, reordered, retyped, or
/// renullabled column fails startup instead of failing later at seal time.
/// Bifrost-managed columns are not user fields and stay outside it.
///
/// # Errors
/// Returns `WYRD_SDK_400_INVALID_OBSERVATION` carrying the expected and the
/// described sequence when they differ in any position.
fn require_projection(table: &WriterTable, expected: &[ProjectedColumn]) -> Result<(), WyrdError> {
    let schema = table.user_schema();
    let fields = schema.fields();
    let exact = fields.len() == expected.len()
        && fields
            .iter()
            .zip(expected)
            .all(|(field, (name, data_type, nullable))| {
                field.name() == name
                    && field.data_type() == data_type
                    && field.is_nullable() == *nullable
            });
    if exact {
        return Ok(());
    }
    let expected: Vec<String> = expected
        .iter()
        .map(|(name, data_type, nullable)| column_label(name, data_type, *nullable))
        .collect();
    let described: Vec<String> = fields
        .iter()
        .map(|field| column_label(field.name(), field.data_type(), field.is_nullable()))
        .collect();
    Err(invalid_observation(
        "fixed observation table declares an incompatible authored schema",
        json!({ "table": table.fqn(), "expected": expected, "described": described }),
    ))
}

/// Render one column for a schema-mismatch refusal, e.g. `series: Utf8 not null`.
fn column_label(name: &str, data_type: &DataType, nullable: bool) -> String {
    let nullability = if nullable { "null" } else { "not null" };
    format!("{name}: {data_type} {nullability}")
}

/// The refusal for any input a projection cannot accept.
fn invalid_observation(message: &str, details: Value) -> WyrdError {
    WyrdError::SdkInvalidObservation {
        message: message.to_owned(),
        details,
    }
}
