//! The one Bifrost lifetime a [`crate::state::WyrdState`] owns.
//!
//! A state starts Bifrost once, describes both fixed observation tables before
//! reporting success, and then serves every scoped run from that one facade.
//! The phase machine exists because `WyrdState` is `Clone` over a shared
//! `Arc`: two clones must not race into two writers, and a caller must not be
//! able to replace a writer that may still hold pending batches.

use std::fmt::{self, Debug, Formatter};
use std::sync::{Arc, Mutex};

use wyrd_spec::error::WyrdError;

use crate::bifrost::{Bifrost, WriterTable};

/// The fixed Drift observation table every state describes at startup.
pub const DRIFT_OBSERVATIONS_TABLE: &str = "vala.drift.observations";
/// The fixed Eval observation table every state describes at startup.
pub const EVAL_OBSERVATIONS_TABLE: &str = "vala.eval.observations";

/// The connected writer plus the two fixed destinations resolved with it.
///
/// Holding a [`WriterTable`] is the proof that its table was described for this
/// writer's lifetime, which is what makes `observe.drift` and `observe.eval`
/// synchronous projection-and-enqueue calls.
pub struct StartedBifrost {
    /// The one state-owned facade: transport, pooled producers, and queries.
    pub bifrost: Bifrost,
    /// `vala.drift.observations`, described once at startup.
    pub drift: WriterTable,
    /// `vala.eval.observations`, described once at startup.
    pub eval: WriterTable,
}

/// Where one state's Bifrost lifetime currently sits.
enum Phase {
    /// No writer has been built; `start_bifrost` may claim the transition.
    NotStarted,
    /// A start is in flight and holds the claim; a second start is refused.
    Starting,
    /// One connected writer with both fixed tables described.
    Started(Arc<StartedBifrost>),
    /// A shutdown succeeded — after a drain, or before any writer existed;
    /// this state never starts or writes again.
    Closed,
}

/// The Bifrost lifetime shared by every clone of one `WyrdState`.
pub struct BifrostLifecycle {
    /// The current phase. Never held across an `await`: a start takes the
    /// [`Phase::Starting`] claim, releases the lock for its IO, and re-takes it
    /// to publish or release.
    phase: Mutex<Phase>,
}

impl Default for BifrostLifecycle {
    /// A state that has not started Bifrost.
    fn default() -> Self {
        Self {
            phase: Mutex::new(Phase::NotStarted),
        }
    }
}

impl Debug for BifrostLifecycle {
    /// Names the phase without reaching into the writer, which is not `Debug`.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let phase = match &*self.phase.lock().expect("bifrost phase lock poisoned") {
            Phase::NotStarted => "not_started",
            Phase::Starting => "starting",
            Phase::Started(_) => "started",
            Phase::Closed => "closed",
        };
        f.debug_struct("BifrostLifecycle")
            .field("phase", &phase)
            .finish()
    }
}

impl BifrostLifecycle {
    /// Claim the single start transition before performing any connect IO.
    ///
    /// Claiming first is what makes a repeated start a refusal rather than a
    /// second transport dial whose result is then thrown away.
    ///
    /// # Errors
    /// Returns `WYRD_SDK_409_BIFROST_ALREADY_STARTED` when a writer is started
    /// or a concurrent start holds the claim, and
    /// `WYRD_SDK_409_BIFROST_CLOSED` after a successful shutdown.
    ///
    /// # Panics
    /// Panics if the phase lock is poisoned.
    pub fn claim(&self) -> Result<StartClaim<'_>, WyrdError> {
        let mut phase = self.phase.lock().expect("bifrost phase lock poisoned");
        match &*phase {
            Phase::NotStarted => {
                *phase = Phase::Starting;
                Ok(StartClaim { lifecycle: self })
            }
            Phase::Starting | Phase::Started(_) => Err(already_started()),
            Phase::Closed => Err(closed()),
        }
    }

    /// The started writer, for a synchronous observation.
    ///
    /// # Errors
    /// Returns `WYRD_SDK_400_BIFROST_NOT_STARTED` before `start_bifrost`
    /// completes and `WYRD_SDK_409_BIFROST_CLOSED` after shutdown.
    ///
    /// # Panics
    /// Panics if the phase lock is poisoned.
    pub fn started(&self) -> Result<Arc<StartedBifrost>, WyrdError> {
        match &*self.phase.lock().expect("bifrost phase lock poisoned") {
            Phase::Started(started) => Ok(Arc::clone(started)),
            Phase::NotStarted | Phase::Starting => Err(not_started()),
            Phase::Closed => Err(closed()),
        }
    }

    /// Drain every producer of the started writer and close this state.
    ///
    /// Every success is terminal. A never-started state, or one whose start is
    /// still in flight, closes at once: the in-flight start then finds `Closed`
    /// and cannot install its writer, so nothing restarts after a successful
    /// shutdown. A started writer is drained first; a drain failure is
    /// ambiguous — batches may or may not have been acknowledged — so the phase
    /// stays `Started` and the caller retries `shutdown` on the same state
    /// rather than replacing the writer.
    ///
    /// # Errors
    /// Returns the first producer or sink failure from the drain.
    ///
    /// # Panics
    /// Panics if the phase lock is poisoned.
    pub async fn shutdown(&self) -> Result<(), WyrdError> {
        let started = {
            let mut phase = self.phase.lock().expect("bifrost phase lock poisoned");
            match &*phase {
                Phase::Started(started) => Arc::clone(started),
                Phase::NotStarted | Phase::Starting => {
                    *phase = Phase::Closed;
                    return Ok(());
                }
                Phase::Closed => return Ok(()),
            }
        };
        started.bifrost.shutdown().await?;
        *self.phase.lock().expect("bifrost phase lock poisoned") = Phase::Closed;
        Ok(())
    }
}

/// An exclusive, in-flight claim on one state's single start transition.
///
/// While the claim lives, `Starting` is its phase alone: only `NotStarted`
/// grants a claim, and a shutdown moves it to `Closed`. Dropping the claim
/// without publishing — a describe failure or a cancelled start future —
/// returns `Starting` to `NotStarted` so the caller can retry startup, and
/// leaves every other phase untouched.
pub struct StartClaim<'a> {
    /// The lifetime this claim will publish into or release.
    lifecycle: &'a BifrostLifecycle,
}

impl StartClaim<'_> {
    /// Describe both fixed observation tables, then publish the started writer.
    ///
    /// The describes happen inside the claim so a state whose fixed tables are
    /// missing, unauthorized, or unreachable never reaches `Started`, and no
    /// run can enqueue an observation the server would refuse per row.
    ///
    /// # Errors
    /// Returns the server's not-found, authentication, authorization, or
    /// availability error for either fixed table, a validation error when a
    /// described table does not carry the projected observation columns, and
    /// `WYRD_SDK_409_BIFROST_CLOSED` when a shutdown won the race while the
    /// describes were in flight; that writer is dropped, never installed.
    ///
    /// # Panics
    /// Panics if the phase lock is poisoned.
    pub async fn complete(self, bifrost: Bifrost) -> Result<(), WyrdError> {
        let drift = bifrost.writer_table(DRIFT_OBSERVATIONS_TABLE).await?;
        crate::observe::drift::require_projection_columns(&drift)?;
        let eval = bifrost.writer_table(EVAL_OBSERVATIONS_TABLE).await?;
        crate::observe::eval::require_projection_columns(&eval)?;
        let started = Arc::new(StartedBifrost {
            bifrost,
            drift,
            eval,
        });
        let mut phase = self
            .lifecycle
            .phase
            .lock()
            .expect("bifrost phase lock poisoned");
        match &*phase {
            Phase::Starting => {
                *phase = Phase::Started(started);
                Ok(())
            }
            _ => Err(closed()),
        }
    }
}

impl Drop for StartClaim<'_> {
    /// Release an unpublished claim so a failed start can be retried.
    ///
    /// Only `Starting` is rolled back: a published `Started` or a shutdown's
    /// `Closed` is final with respect to this claim.
    fn drop(&mut self) {
        if let Ok(mut phase) = self.lifecycle.phase.lock()
            && matches!(*phase, Phase::Starting)
        {
            *phase = Phase::NotStarted;
        }
    }
}

/// The refusal for a second `start_bifrost` on one state.
fn already_started() -> WyrdError {
    WyrdError::SdkBifrostAlreadyStarted {
        message: "this WyrdState already started Bifrost".to_owned(),
        details: serde_json::Value::Null,
    }
}

/// The refusal for an observation before `start_bifrost` completed.
fn not_started() -> WyrdError {
    WyrdError::SdkBifrostNotStarted {
        message: "call start_bifrost before observing".to_owned(),
        details: serde_json::Value::Null,
    }
}

/// The refusal for any Bifrost use after a successful shutdown.
fn closed() -> WyrdError {
    WyrdError::SdkBifrostClosed {
        message: "this WyrdState was shut down; create a new state to start again".to_owned(),
        details: serde_json::Value::Null,
    }
}
