//! The scoped observation surface: one invocation, Card-scoped views, and
//! Verifier executions.
//!
//! [`Run`] is one application invocation. It is opened locally — no network IO,
//! no server-side Run resource — and carries the `run_id` and the observed
//! subject `CardRef` that every observation sends as Bifrost row correlation.
//! [`Run::for_card`] returns an immutable sibling view over the same invocation,
//! so concurrent views never disturb each other's subject.
//!
//! [`Observe::verify`] executes a bound Verifier now and [`Observe::submit`]
//! queues one execution or records scheduled-window samples. Slot projection,
//! evidence upload, and execution polling live here rather than in
//! [`crate::bifrost`]: the facade, the producer pool, and `wyrd-queue` stay
//! generic plumbing that buffers JSON rows against a described schema. Every
//! SDK projects this one Rust surface, so the three languages cannot drift
//! into three projections.

pub mod drift;
pub mod lifecycle;
#[cfg(test)]
mod tests;

use arrow_schema::DataType;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardUid, GroupId};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::ids::RunId;
use wyrd_spec::vala::ids::SessionId;
use wyrd_spec::verification::execution::VerificationResult;

use crate::bifrost::{Correlation, WriterTable};
use crate::state::{BoundVerifier, WyrdState};
use crate::verify::{Execution, Inputs, Projected, RunEvidence, Submission, Target};

pub use lifecycle::DRIFT_OBSERVATIONS_TABLE;

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
    /// this run and Card once `wyrd_ai::otel::start_telemetry` installed
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
        Observe {
            run: self,
            group_id: None,
        }
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
    /// Never for a loaded [`WyrdState`]: a Card bundle refuses any Card
    /// without a UID at load, and every subject is resolved from that bundle.
    #[must_use]
    pub fn subject_uid(&self) -> &CardUid {
        self.subject
            .uid
            .as_ref()
            .expect("every hydrated Card carries its UID")
    }
}

/// The observation and verification surface of one scoped run.
///
/// Recording is not a durability acknowledgement — the queue publishes in the
/// background and `WyrdState::shutdown` is the barrier.
#[derive(Debug)]
pub struct Observe<'a> {
    /// The view whose subject and invocation every emit correlates to.
    run: &'a Run,
    /// The caller group every `verify`/`submit` execution from this surface
    /// carries; `None` keeps executions ungrouped.
    group_id: Option<GroupId>,
}

impl Observe<'_> {
    /// Project and enqueue one Drift observation.
    ///
    /// Input validation runs before the lifecycle lookup so a malformed feature
    /// map reports what is wrong with it rather than reporting that Bifrost has
    /// not started.
    ///
    /// # Errors
    /// Returns `WYRD_SDK_400_INVALID_OBSERVATION` for a malformed feature
    /// map, `WYRD_SDK_400_BIFROST_NOT_STARTED` before startup,
    /// `WYRD_SDK_409_BIFROST_CLOSED` after shutdown, and
    /// `WYRD_CLIENT_429_QUEUE_FULL` when the producer is saturated.
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
    /// Carry `group_id` on every execution `verify` and `submit` start from
    /// this surface.
    ///
    /// The value is the caller's own opaque group, such as a conversation or
    /// order ID, scoped by the server to the tenant and authenticated owner
    /// Service. It performs no IO: grouped Verifier bindings discover
    /// executions sharing it later. Window samples carry no group.
    ///
    /// # Arguments
    /// * `group_id` - The caller group the executions belong to.
    #[must_use]
    pub fn group_id(mut self, group_id: GroupId) -> Self {
        self.group_id = Some(group_id);
        self
    }

    /// Carry the foreign-runtime caller's optional `group_id` text, validated
    /// as a [`GroupId`]; `None` leaves the surface ungrouped.
    ///
    /// # Errors
    /// Returns `WYRD_SPEC_400_VALIDATION` naming `group_id` when the text is
    /// empty, longer than 256 bytes, or holds whitespace or a control
    /// character.
    #[cfg(feature = "internal")]
    pub fn grouped(self, group_id: Option<&str>) -> Result<Self, WyrdError> {
        let Some(group_id) = group_id else {
            return Ok(self);
        };
        GroupId::new(group_id)
            .map(|group_id| self.group_id(group_id))
            .map_err(|error| WyrdError::Validation {
                message: format!("group_id is invalid: {error}"),
                details: json!({ "field": "group_id", "reason": error.to_string() }),
            })
    }

    /// Execute the Verifier named `verifier` on this view's subject now and
    /// return its attributable result.
    ///
    /// `verifier` is the `metadata.name` of a Verifier bound in
    /// `verified_by` to this view's subject in the hydrated graph. Values
    /// project onto its named slots as [`Inputs`] describes; files are hashed
    /// and uploaded during this call. The execution carries this view's
    /// `run_id`, creates no queued work, and dispatches no Operator. A failed
    /// verdict is a normal result, not an error.
    ///
    /// # Arguments
    /// * `verifier` - The `metadata.name` of a Verifier bound to this view's subject.
    /// * `inputs` - Values named by slot, or one value for a single-slot Verifier.
    ///
    /// # Errors
    /// Returns `WYRD_SDK_404_UNKNOWN_VERIFIER` when no Verifier of that name
    /// is bound to the subject, `WYRD_VERIFICATION_422_INPUT_UNSUPPORTED`
    /// for a scheduled-window Verifier, which only `submit` records samples
    /// for, and the slot refusals of `submit`, all before any execution
    /// request; then the client resolution error and the server's refusal,
    /// including `WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT` after the
    /// 60-second deadline.
    ///
    /// # Cancellation
    /// Dropping the future abandons the request; it is never retried.
    pub async fn verify(
        &self,
        verifier: &str,
        inputs: impl Into<Inputs>,
    ) -> Result<VerificationResult, WyrdError> {
        let state = &self.run.state;
        let bound = state.bound_verifier(&self.run.subject, verifier)?;
        let target = self.target(&bound);
        match target.project(inputs.into())? {
            Projected::Run(run) => target.execute(state.client()?, run).await,
            Projected::Window(_) => Err(WyrdError::VerificationInputUnsupported {
                message:
                    "a scheduled-window Verifier judges recorded samples; record them with submit"
                        .to_owned(),
                details: json!({ "verifier": verifier }),
            }),
        }
    }

    /// [`Observe::verify`], with every requirement's custom `data` decoded
    /// as `T`.
    ///
    /// The execution is the ordinary one; decoding happens after its result
    /// arrives, so a mismatch neither changes the stored result nor reruns
    /// the Verifier.
    ///
    /// # Arguments
    /// * `verifier` - The `metadata.name` of a Verifier bound to this view's subject.
    /// * `inputs` - Values named by slot, or one value for a single-slot Verifier.
    ///
    /// # Errors
    /// Returns the errors of [`Observe::verify`], then
    /// `WYRD_SDK_422_VERIFICATION_DATA_INVALID` when present data does not
    /// decode as `T`.
    ///
    /// # Cancellation
    /// Dropping the future abandons the request; it is never retried.
    pub async fn verify_typed<T: DeserializeOwned>(
        &self,
        verifier: &str,
        inputs: impl Into<Inputs>,
    ) -> Result<VerificationResult<T>, WyrdError> {
        self.verify(verifier, inputs).await?.decode_data()
    }

    /// Submit evidence for the Verifier named `verifier` on this view's
    /// subject without waiting for a judgment.
    ///
    /// Run evidence is staged for one queued execution and returns its
    /// [`Execution`]; a `202` confirms staging, not queue
    /// acceptance, so a replay conflict or a loss before acceptance surfaces
    /// through the handle. Window samples are recorded through this state's
    /// Bifrost Drift observation path and return [`Submission::Recorded`]:
    /// the Verifier's schedule judges the window later.
    ///
    /// # Arguments
    /// * `verifier` - The `metadata.name` of a Verifier bound to this view's subject.
    /// * `inputs` - Values named by slot, or one value for a single-slot Verifier.
    ///
    /// # Errors
    /// Returns `WYRD_SDK_404_UNKNOWN_VERIFIER` for an unbound name and
    /// `WYRD_VERIFICATION_400_INPUT_INVALID` for an unknown slot, a value of
    /// the wrong kind, an unreadable file, or a file format its slot does not
    /// accept, and `WYRD_VERIFICATION_413_INPUT_TOO_LARGE` for a file above
    /// its slot's `max_bytes`, all before any execution request; for Window
    /// samples, the Bifrost lifecycle and queue errors of an observation;
    /// otherwise the client resolution error and the server's refusal.
    ///
    /// # Cancellation
    /// Dropping the future after the request was sent may leave a staged
    /// submission whose execution ID the caller never learns.
    pub async fn submit(
        &self,
        verifier: &str,
        inputs: impl Into<Inputs>,
    ) -> Result<Submission, WyrdError> {
        match self.record_or_stage(verifier, inputs)? {
            None => Ok(Submission::Recorded),
            Some(staged) => Ok(Submission::Queued(self.enqueue_run(staged).await?)),
        }
    }

    /// The synchronous half of [`Observe::submit`]: project `inputs` for the
    /// Verifier named `verifier`, record Window samples now, and return
    /// `None`, or return the Run evidence still to enqueue.
    ///
    /// A foreign runtime calls this on its own thread, inside its producer
    /// span scope, so each recorded sample's queued write records its
    /// `bifrost.enqueue` PRODUCER span there.
    ///
    /// # Errors
    /// As [`Observe::submit`], before any execution request.
    #[cfg(feature = "internal")]
    pub fn stage(
        &self,
        verifier: &str,
        inputs: impl Into<Inputs>,
    ) -> Result<Option<StagedRun>, WyrdError> {
        self.record_or_stage(verifier, inputs)
    }

    /// Project `inputs` for the Verifier named `verifier`: record Window
    /// samples and return `None`, or return the Run evidence to enqueue.
    ///
    /// # Errors
    /// As [`Observe::submit`], before any execution request.
    fn record_or_stage(
        &self,
        verifier: &str,
        inputs: impl Into<Inputs>,
    ) -> Result<Option<StagedRun>, WyrdError> {
        let bound = self.run.state.bound_verifier(&self.run.subject, verifier)?;
        match self.target(&bound).project(inputs.into())? {
            Projected::Run(evidence) => Ok(Some(StagedRun {
                verifier: verifier.to_owned(),
                group_id: self.group_id.clone(),
                evidence,
            })),
            Projected::Window(features) => {
                self.drift_value(&features, None)?;
                Ok(None)
            }
        }
    }

    /// The asynchronous half of [`Observe::submit`]: pin `staged`'s files
    /// and stage it for one queued execution.
    ///
    /// # Errors
    /// As [`Observe::submit`] for Run evidence.
    ///
    /// # Cancellation
    /// As [`Observe::submit`].
    #[cfg(feature = "internal")]
    pub async fn enqueue_staged(&self, staged: StagedRun) -> Result<Execution, WyrdError> {
        self.enqueue_run(staged).await
    }

    /// Pin `staged`'s files and stage it for one queued execution.
    ///
    /// # Errors
    /// As [`Observe::submit`] for Run evidence.
    ///
    /// # Cancellation
    /// As [`Observe::submit`].
    async fn enqueue_run(&self, staged: StagedRun) -> Result<Execution, WyrdError> {
        let state = &self.run.state;
        let bound = state.bound_verifier(&self.run.subject, &staged.verifier)?;
        let mut target = self.target(&bound);
        target.group_id = staged.group_id;
        target.enqueue(state.client()?, staged.evidence).await
    }

    /// The execution target of `bound` for this view's Run.
    fn target<'a>(&self, bound: &'a BoundVerifier) -> Target<'a> {
        Target {
            spec: &bound.spec,
            verifier_card_uid: bound.verifier_card_uid.clone(),
            card_uid: bound.card_uid.clone(),
            run_id: self.run.run_id.clone(),
            group_id: self.group_id.clone(),
            deployment_environment: self.run.state.deployment_environment().map(str::to_owned),
        }
    }
}

/// Run evidence [`Observe::stage`] projected and still has to enqueue.
#[derive(Debug)]
pub struct StagedRun {
    /// The bound Verifier's `metadata.name`, resolved again on enqueue.
    verifier: String,
    /// The caller group captured when the evidence was staged.
    group_id: Option<GroupId>,
    /// The projected evidence, before its files are pinned.
    evidence: RunEvidence,
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
