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
mod invoke;
pub mod lifecycle;
#[cfg(test)]
mod tests;
mod verify;

use arrow_schema::DataType;
use serde::Serialize;
use serde_json::{Value, json};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::eval::media::MediaRef;
use wyrd_spec::vala::ids::{RunId, SessionId};
use wyrd_spec::verification::{ExecuteVerificationRequest, Judgment};

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

tokio::task_local! {
    /// The `(card_uid, run_id)` text of the innermost [`Run::scope`] the
    /// current task runs in, which a telemetry span processor stamps on spans.
    static RUN_SCOPE: (String, String);
}

/// The `(card_uid, run_id)` text of the innermost [`Run::scope`] the current
/// task runs in, or `None` outside every scope.
///
/// A telemetry span processor reads this when a span starts so the span
/// carries the record-level `wyrd.card_uid` and `wyrd.run_id` attributes
/// Bifrost extracts.
#[must_use]
pub fn current_run_scope() -> Option<(String, String)> {
    RUN_SCOPE.try_with(Clone::clone).ok()
}

/// One application invocation, optionally scoped to a registered Card.
///
/// Cloning a run is cloning its view: the `run_id` and the state-owned Bifrost
/// writer are shared, the subject is not.
#[derive(Debug, Clone)]
#[expect(
    clippy::struct_field_names,
    reason = "`run_id` is the wire name of the invocation identity"
)]
pub struct Run {
    /// The hydrated graph and the one Bifrost lifetime this run emits through.
    state: WyrdState,
    /// The invocation identity every observation of every view correlates to.
    run_id: RunId,
    /// The alias this view was opened with: `root` for `WyrdState::run`, the
    /// alias given to `WyrdState::run_for_card` or `for_card` otherwise.
    alias: String,
    /// The exact Card `alias` resolves to in the hydrated graph.
    subject: CardRef,
}

impl Run {
    /// Run `future` inside this view's scope, so spans its task starts carry
    /// this run and Card once `wyrd_sdk::otel::start_telemetry` installed
    /// telemetry.
    ///
    /// The scope is a Tokio task-local: it follows `future` across awaits but
    /// not into tasks it spawns, and an inner scope shadows an outer one until
    /// it completes. No IO happens here.
    pub fn scope<F: Future>(&self, future: F) -> impl Future<Output = F::Output> {
        RUN_SCOPE.scope(
            (
                self.subject_uid().as_str().to_owned(),
                self.run_id.as_str().to_owned(),
            ),
            future,
        )
    }

    /// Open a run over `state` whose first view observes `subject`, opened
    /// as `alias`.
    ///
    /// Mints the invocation's UUIDv7 `run_id`; every later [`Run::for_card`]
    /// view shares it. The caller has already resolved `subject` from the
    /// state's hydrated graph, so opening never fails and performs no IO.
    pub(crate) fn new(state: WyrdState, alias: String, subject: CardRef) -> Self {
        Self {
            state,
            run_id: RunId::new(),
            alias,
            subject,
        }
    }

    /// The invocation identity shared by this run and every view of it.
    #[must_use]
    pub fn run_id(&self) -> &RunId {
        &self.run_id
    }

    /// The alias this view was opened with; `root` for the Service view.
    ///
    /// The exact Card reference for an alias is `WyrdState::card_ref`.
    #[must_use]
    pub fn alias(&self) -> &str {
        &self.alias
    }

    /// The exact Card this view observes, which every observation carries as
    /// its correlation and a foreign runtime attaches to its trace spans.
    #[must_use]
    #[cfg(feature = "internal")]
    pub fn subject(&self) -> &CardRef {
        &self.subject
    }

    /// An immutable sibling view scoped to `alias` in the same hydrated graph.
    ///
    /// The parent view is unchanged, so a Service run and its Model and Agent
    /// views may be used concurrently. Resolution is a local index lookup: an
    /// unknown or out-of-graph alias fails without any network IO and never
    /// falls back to the root Service.
    ///
    /// # Arguments
    /// * `alias` - The bundle alias of the Card the new view observes.
    ///
    /// # Errors
    /// Returns `WYRD_SDK_404_UNKNOWN_ALIAS` when the alias is not registered in
    /// this bundle.
    pub fn for_card(&self, alias: &str) -> Result<Self, WyrdError> {
        Ok(Self {
            state: self.state.clone(),
            run_id: self.run_id.clone(),
            alias: alias.to_owned(),
            subject: self.state.card_ref(alias)?.clone(),
        })
    }

    /// The emit surface for this view.
    #[must_use]
    pub fn observe(&self) -> Observe<'_> {
        Observe { run: self }
    }

    /// This view's row correlation: the subject Card's UID plus invocation.
    fn correlation(&self) -> Correlation {
        Correlation {
            card_uid: Some(self.subject_uid().clone()),
            run_id: Some(self.run_id.clone()),
        }
    }

    /// The UID of the Card this view observes: every observation's
    /// `card_uid` correlation and the `wyrd.card_uid` its spans carry.
    ///
    /// # Panics
    /// Never for a loaded [`WyrdState`]: a hydrated bundle refuses any Card
    /// without a UID at load, and every subject is resolved from that bundle.
    #[must_use]
    pub fn subject_uid(&self) -> &CardUid {
        self.subject
            .uid
            .as_ref()
            .expect("every hydrated Card carries its UID")
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
    /// # Arguments
    /// * `features` - One flat object of supported scalar feature values.
    /// * `session_id` - The session the observation belongs to, or `None`.
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
    /// An integer literal beyond 64-bit range is refused from the text before
    /// parsing would round it to a float.
    ///
    /// # Arguments
    /// * `json` - One flat JSON object of supported scalar feature values.
    /// * `session_id` - The session the observation belongs to, or `None`.
    ///
    /// # Errors
    /// As [`Observe::drift`], plus an invalid-observation error when `json` is
    /// not valid JSON or carries an integer literal beyond exact `Float64`
    /// range.
    #[cfg(feature = "internal")]
    pub fn drift_json(&self, json: &str, session_id: Option<SessionId>) -> Result<(), WyrdError> {
        drift::check_integer_literals(json)?;
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
        started.bifrost.insert_observation(
            &started.drift,
            drift::rows(&record)?,
            self.run.correlation(),
        )?;
        Ok(())
    }

    /// Emit one Eval observation from a `Serialize` context value.
    ///
    /// # Arguments
    /// * `context` - The context object the Eval observation records.
    /// * `options` - The optional trace correlation, session, and media.
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
    /// # Arguments
    /// * `json` - The context object as JSON text.
    /// * `options` - The optional trace correlation, session, and media.
    ///
    /// # Errors
    /// As [`Observe::eval`], plus an invalid-observation error when `json` is
    /// not valid JSON.
    #[cfg(feature = "internal")]
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
        started.bifrost.insert_observation(
            &started.eval,
            vec![eval::row(&record)?],
            self.run.correlation(),
        )?;
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
    /// # Arguments
    /// * `table` - A registered `vala.datasets.<name>` table.
    /// * `row` - The row, serialized to the table's columns.
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
    /// # Arguments
    /// * `table` - A registered `vala.datasets.<name>` table.
    /// * `json` - The row as one JSON object.
    ///
    /// # Errors
    /// As [`Observe::record`], plus an invalid-observation error when `json` is
    /// not valid JSON.
    #[cfg(feature = "internal")]
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
        let destination = started.bifrost.resolve_writer_table(table).await?;
        started.bifrost.insert_observation(
            &destination,
            vec![row_bytes(row)?],
            self.run.correlation(),
        )?;
        Ok(())
    }
}

impl Observe<'_> {
    /// Judge `input` with the Verifier named `verifier` and return its judgment.
    ///
    /// `verifier` is the `metadata.name` of a Verifier bound in `verified_by`
    /// to this view's subject in the hydrated graph; the subject is always
    /// this view's. An Eval Verifier takes one context object, in the forms
    /// [`Observe::eval`] accepts; a Drift Verifier takes a non-empty sequence
    /// of feature rows, in the forms [`Observe::drift`] accepts. It creates
    /// no observation, run, or Operator dispatch, and Bifrost need not be
    /// started; the server records the judgment as one result correlated to
    /// this Run. A `failed` verdict is a normal return.
    ///
    /// # Arguments
    /// * `verifier` - The `metadata.name` of a Verifier bound to this view's subject.
    /// * `input` - One Eval context object, or a non-empty sequence of Drift feature rows.
    ///
    /// # Errors
    /// Returns `WYRD_SDK_404_UNKNOWN_VERIFIER` when no Verifier of that name
    /// is bound to the subject and `WYRD_SDK_400_INVALID_OBSERVATION` when the
    /// input's shape does not match the Verifier's kind, both before any
    /// network call; the client resolution error when no server or credential
    /// is configured; and the server's refusal, including
    /// `WYRD_VERIFICATION_409_BASELINE_NOT_READY` for an unfitted baseline.
    ///
    /// # Cancellation
    /// Dropping the future abandons the request; the call is never retried.
    pub async fn verify<T: Serialize>(
        &self,
        verifier: &str,
        input: &T,
    ) -> Result<Judgment, WyrdError> {
        self.verify_value(verifier, to_value(input, "verify input")?, Vec::new())
            .await
    }

    /// Judge one Eval context carrying media a judge Prompt binds by `id`.
    ///
    /// # Arguments
    /// * `verifier` - The `metadata.name` of an Eval Verifier bound to this view's subject.
    /// * `context` - The Eval context object.
    /// * `media` - Media references a judge Prompt binds by `id`.
    ///
    /// # Errors
    /// As [`Observe::verify`], plus `WYRD_SDK_400_INVALID_OBSERVATION` when
    /// `verifier` is a Drift Verifier, which takes no media.
    ///
    /// # Cancellation
    /// As [`Observe::verify`].
    pub async fn verify_with_media<T: Serialize>(
        &self,
        verifier: &str,
        context: &T,
        media: Vec<MediaRef>,
    ) -> Result<Judgment, WyrdError> {
        self.verify_value(verifier, to_value(context, "verify input")?, media)
            .await
    }

    /// Judge input from a foreign runtime's JSON text.
    ///
    /// # Arguments
    /// * `verifier` - The `metadata.name` of a Verifier bound to this view's subject.
    /// * `json` - The input as JSON text, in the shape [`Observe::verify`] takes.
    /// * `media` - Media references a judge Prompt binds by `id`; empty for none.
    ///
    /// # Errors
    /// As [`Observe::verify_with_media`], plus an invalid-observation error
    /// when `json` is not valid JSON or a Drift row carries an integer literal
    /// beyond exact `Float64` range.
    ///
    /// # Cancellation
    /// As [`Observe::verify`].
    #[cfg(feature = "internal")]
    pub async fn verify_json(
        &self,
        verifier: &str,
        json: &str,
        media: Vec<MediaRef>,
    ) -> Result<Judgment, WyrdError> {
        verify::check_drift_integer_literals(json)?;
        self.verify_value(verifier, parse_json(json, "verify input")?, media)
            .await
    }

    /// Resolve the bound Verifier, build its request, and execute it.
    ///
    /// Resolution and input conversion are local and complete before the
    /// client is resolved, so a refusal of either sends nothing.
    ///
    /// # Errors
    /// As [`Observe::verify_with_media`].
    async fn verify_value(
        &self,
        verifier: &str,
        input: Value,
        media: Vec<MediaRef>,
    ) -> Result<Judgment, WyrdError> {
        let state = &self.run.state;
        let bound = state.bound_verifier(&self.run.subject, verifier)?;
        let request = ExecuteVerificationRequest {
            verifier_uid: bound.verifier_uid,
            subject_card_uid: bound.subject_uid,
            input: verify::direct_input(bound.implementation, input, media)?,
            run_id: Some(self.run.run_id.clone()),
        };
        verify::execute(state.client()?, &request).await
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
#[cfg(feature = "internal")]
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
