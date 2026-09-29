//! Verification binding projection, owner activity, and schedule arming.
//!
//! `wyrd.verification_bindings` holds one row per effective inline
//! `verified_by` binding of an exact Service or standalone Agent Card version.
//! Composite registration projects the rows through [`project_bindings`] on
//! the registry's own [`TenantConn`], so the owner Card, its Card-bound
//! principal, and its bindings commit or roll back together. Runtime activity
//! is the owner principal's `last_authenticated_at`, which the tenant token
//! issuer records through [`record_machine_authentication`]; admission reads
//! it through [`binding_activity`]. Every function here runs on the caller's
//! transaction under forced RLS and never commits.
// raw-query grep allowlist: verification tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use std::str::FromStr;

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use croner::Cron;
use croner::parser::{CronParser, Seconds, Year};
use sqlx::types::{Json, Uuid};
use wyrd_runtime::principal::PrincipalId;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{BindingId, CardUid};
use wyrd_spec::verification::DriftWindow;
/// The frozen Trigger/Operator identity is the wire contract's own type, so
/// stored JSON, dispatch rows, and Run GET share one shape.
pub use wyrd_spec::verification::FrozenTarget;

use crate::TenantConn;

/// Default `WYRD_VERIFICATION_INACTIVITY_TIMEOUT_SECONDS`: one day.
pub const DEFAULT_INACTIVITY_TIMEOUT_SECONDS: i64 = 86_400;

/// Insert one binding under its natural key, keeping the stored row on conflict.
///
/// The no-op `DO UPDATE` exists only so `RETURNING` yields the stored
/// `binding_id` for an existing key; the owner Card is immutable, so a
/// conflicting row always carries the same frozen targets.
const PROJECT_BINDING_SQL: &str = r#"
    INSERT INTO wyrd.verification_bindings (
        binding_id, data_tenant_id, owner_card_uid, owner_card_kind,
        subject_occurrence_key, subject_card_uid, verifier_uid,
        trigger_uid, trigger_digest, operators, activation,
        schedule_cron, schedule_tz
    ) VALUES ($1, wyrd.current_tenant(), $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
    ON CONFLICT ON CONSTRAINT verification_bindings_natural_key
    DO UPDATE SET binding_id = wyrd.verification_bindings.binding_id
    RETURNING binding_id
"#;

/// List one owner Card version's binding identities in identity order.
const OWNER_BINDING_IDS_SQL: &str = r#"
    SELECT binding_id
      FROM wyrd.verification_bindings
     WHERE owner_card_uid = $1
     ORDER BY binding_id
"#;

/// Stamp a qualifying exchange on an active Card-bound Service or Agent.
///
/// The stamp is PostgreSQL's own `statement_timestamp()`, so activity is
/// written and later evaluated on one clock. `GREATEST` keeps the later of the
/// stored and database instants, so a database clock that steps backward never
/// moves activity backward (`GREATEST` ignores a stored NULL). Returns the
/// owner Card UID beside the database time that was stamped, or no row for a
/// Card-free, inactive, or non-machine principal.
const RECORD_AUTHENTICATION_SQL: &str = r#"
    UPDATE wyrd.auth_service_accounts
       SET last_authenticated_at = GREATEST(last_authenticated_at, statement_timestamp())
     WHERE id = $1
       AND principal_kind IN ('service', 'agent')
       AND card_uid IS NOT NULL
       AND status = 'active'
    RETURNING card_uid, statement_timestamp()
"#;

/// Lock and list one owner's `schedule` bindings whose cursor is still null.
const UNARMED_SCHEDULES_SQL: &str = r#"
    SELECT binding_id, schedule_cron, schedule_tz
      FROM wyrd.verification_bindings
     WHERE owner_card_uid = $1
       AND activation = 'schedule'
       AND next_run_at IS NULL
     ORDER BY binding_id
       FOR UPDATE
"#;

/// Arm one cursor, only while it is still null so an armed cursor never moves.
const ARM_SCHEDULE_SQL: &str = r#"
    UPDATE wyrd.verification_bindings
       SET next_run_at = $2
     WHERE binding_id = $1
       AND next_run_at IS NULL
"#;

/// Read one binding's owner principal state and derived admission gate.
///
/// `$2` is the inactivity window in whole seconds; the cutoff is derived from
/// PostgreSQL's own `statement_timestamp()`, the same clock the stamp was
/// written on. The principal join is left so a binding whose owner has no
/// Card-bound principal still reads as inactive.
const BINDING_ACTIVITY_SQL: &str = r#"
    SELECT b.binding_id, b.owner_card_uid, p.id AS principal_id,
           p.last_authenticated_at, b.next_run_at,
           (p.status = 'active'
            AND c.status = 'active'
            AND p.last_authenticated_at IS NOT NULL
            AND p.last_authenticated_at
                > statement_timestamp() - ($2::bigint * INTERVAL '1 second')) AS active
      FROM wyrd.verification_bindings b
      JOIN wyrd.cards c
        ON c.card_uid = b.owner_card_uid
      LEFT JOIN wyrd.auth_service_accounts p
        ON p.card_uid = b.owner_card_uid
       AND p.card_kind = b.owner_card_kind
     WHERE b.binding_id = $1
"#;

/// Why a Trigger schedule cannot be armed.
#[derive(Debug, thiserror::Error)]
pub enum ScheduleError {
    /// The cron expression is not a five-field minute-granularity pattern.
    #[error("invalid cron expression {cron:?}: {reason}")]
    InvalidCron {
        /// The authored expression.
        cron: String,
        /// Parser diagnostic.
        reason: String,
    },
    /// The timezone is not an IANA zone name.
    #[error("unknown IANA timezone {tz:?}")]
    UnknownTimezone {
        /// The authored zone name.
        tz: String,
    },
    /// The pattern matches no future instant.
    #[error("cron expression {cron:?} has no occurrence after {after}")]
    NoOccurrence {
        /// The authored expression.
        cron: String,
        /// The instant searched from.
        after: DateTime<Utc>,
    },
    /// The pattern matches no earlier instant.
    #[error("cron expression {cron:?} has no occurrence before {before}")]
    NoPreviousOccurrence {
        /// The authored expression.
        cron: String,
        /// The instant searched back from.
        before: DateTime<Utc>,
    },
}

/// A parsed `schedule` Trigger: a five-field cron pattern in one timezone.
///
/// The one owner of what a valid Trigger schedule is and where its next
/// boundary falls. Registration parses it to refuse an unarmable binding, and
/// arming a cursor computes the first boundary strictly after the qualifying
/// exchange. Seconds and year fields are refused so a schedule has minute
/// granularity; an absent timezone means UTC.
#[derive(Debug, Clone)]
pub struct BindingSchedule {
    /// The authored expression, kept for diagnostics.
    source: String,
    /// Parsed pattern.
    cron: Cron,
    /// Zone the pattern's wall-clock fields are evaluated in.
    tz: Tz,
}

impl BindingSchedule {
    /// Parse an authored `cron` and optional IANA `tz`.
    ///
    /// # Errors
    /// Returns [`ScheduleError::InvalidCron`] for a pattern that is not five
    /// fields or does not parse, and [`ScheduleError::UnknownTimezone`] for a
    /// zone name absent from the IANA database.
    pub fn parse(cron: &str, tz: Option<&str>) -> Result<Self, ScheduleError> {
        let parsed = CronParser::builder()
            .seconds(Seconds::Disallowed)
            .year(Year::Disallowed)
            .build()
            .parse(cron)
            .map_err(|error| ScheduleError::InvalidCron {
                cron: cron.to_owned(),
                reason: error.to_string(),
            })?;
        let tz = match tz {
            Some(name) => Tz::from_str(name).map_err(|_| ScheduleError::UnknownTimezone {
                tz: name.to_owned(),
            })?,
            None => Tz::UTC,
        };
        Ok(Self {
            source: cron.to_owned(),
            cron: parsed,
            tz,
        })
    }

    /// Return the first boundary strictly after `instant`.
    ///
    /// The search runs in the schedule's zone, so daylight-saving transitions
    /// follow the zone's wall clock, and the result is converted back to UTC.
    ///
    /// # Errors
    /// Returns [`ScheduleError::NoOccurrence`] when the pattern names no
    /// reachable future instant, such as February 30th.
    pub fn next_after(&self, instant: DateTime<Utc>) -> Result<DateTime<Utc>, ScheduleError> {
        self.cron
            .find_next_occurrence(&instant.with_timezone(&self.tz), false)
            .map(|next| next.with_timezone(&Utc))
            .map_err(|_| ScheduleError::NoOccurrence {
                cron: self.source.clone(),
                after: instant,
            })
    }

    /// Return the last boundary strictly before `instant`.
    ///
    /// Like [`Self::next_after`], the search runs in the schedule's zone.
    ///
    /// # Errors
    /// Returns [`ScheduleError::NoPreviousOccurrence`] when the pattern names
    /// no reachable earlier instant.
    pub fn previous_before(&self, instant: DateTime<Utc>) -> Result<DateTime<Utc>, ScheduleError> {
        self.cron
            .find_previous_occurrence(&instant.with_timezone(&self.tz), false)
            .map(|previous| previous.with_timezone(&Utc))
            .map_err(|_| ScheduleError::NoPreviousOccurrence {
                cron: self.source.clone(),
                before: instant,
            })
    }

    /// Decide what claiming the occurrence due at `due` produces at `now`.
    ///
    /// The due occurrence fixes its window `[previous boundary, due)` no matter
    /// how late within its period it is claimed: an hourly occurrence due at
    /// 01:00 claimed at 01:03 analyzes `[00:00, 01:00)`. When a later boundary
    /// has also passed by `now` — an outage or stalled scheduler — every passed
    /// occurrence, including the stored one, is missed: no window is produced
    /// and none is backfilled. Either way the next cursor is the first
    /// boundary strictly after `now`, so it is always in the future.
    ///
    /// # Errors
    /// Returns [`ScheduleError::NoOccurrence`] or
    /// [`ScheduleError::NoPreviousOccurrence`] when a needed boundary does not
    /// exist.
    pub fn occurrence(
        &self,
        due: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<ScheduledOccurrence, ScheduleError> {
        let next_run_at = self.next_after(now)?;
        let window = if self.next_after(due)? <= now {
            None
        } else {
            Some(DriftWindow {
                start: self.previous_before(due)?,
                end: due,
            })
        };
        Ok(ScheduledOccurrence {
            window,
            next_run_at,
        })
    }
}

/// What claiming one due schedule occurrence produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduledOccurrence {
    /// The fixed comparison window, or `None` when the occurrence was missed.
    pub window: Option<DriftWindow>,
    /// The next cursor: the first boundary strictly after the claim time.
    pub next_run_at: DateTime<Utc>,
}

/// The effective activation frozen on a binding row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingActivation {
    /// A cron schedule; its cursor stays null until the owner activates.
    Schedule {
        /// Authored cron expression, already validated by [`BindingSchedule`].
        cron: String,
        /// Authored IANA timezone; absent means UTC.
        tz: Option<String>,
    },
    /// Each committed observation for the subject; no cursor.
    ObservationsReady,
}

/// One effective binding of an owner Card version, ready to project.
#[derive(Debug, Clone)]
pub struct NewBinding {
    /// Component alias for a Service component binding, or
    /// [`wyrd_spec::card::verifier::OWNER_OCCURRENCE_KEY`] for a
    /// Service-level or standalone-Agent binding; never null, so the natural
    /// key is total.
    pub subject_occurrence_key: String,
    /// Exact verified subject: the owner itself or the component's Card.
    pub subject_card_uid: CardUid,
    /// Exact bound Verifier version.
    pub verifier_uid: CardUid,
    /// Effective Trigger.
    pub trigger: FrozenTarget,
    /// Effective `on_failure` Operators in authored order.
    pub operators: Vec<FrozenTarget>,
    /// Effective activation.
    pub activation: BindingActivation,
}

/// Project an owner Card version's effective bindings.
///
/// Inserts one row per binding under the natural key (tenant, owner Card UID,
/// subject occurrence, Verifier UID) with a freshly minted `UUIDv7`. A key
/// that already exists keeps its stored identity and frozen targets — the
/// owner Card is immutable, so a re-projection is always of the same content
/// — which is what makes re-apply and reordering identity-preserving. Returns
/// the identities in input order.
///
/// # Errors
/// Returns the database error when an insert fails, including a foreign-key
/// violation when `owner_card_uid` names no Card in the caller's tenant. The
/// caller's transaction then rolls back with the Card it was projecting.
#[tracing::instrument(skip(conn, bindings), fields(operation = "verification.bindings.project", owner = %owner_card_uid))]
pub async fn project_bindings(
    conn: &mut TenantConn<'_>,
    owner_card_uid: &CardUid,
    owner_kind: &CardKind,
    bindings: &[NewBinding],
) -> Result<Vec<BindingId>, sqlx::Error> {
    let mut ids = Vec::with_capacity(bindings.len());
    for binding in bindings {
        let (trigger_uid, trigger_digest) = match &binding.trigger {
            FrozenTarget::Uid(uid) => (Some(uid.as_uuid()), None),
            FrozenTarget::Digest(digest) => (None, Some(digest.as_str())),
        };
        let (activation, cron, tz) = match &binding.activation {
            BindingActivation::Schedule { cron, tz } => {
                ("schedule", Some(cron.as_str()), tz.as_deref())
            }
            BindingActivation::ObservationsReady => ("observations_ready", None, None),
        };
        let id: Uuid = sqlx::query_scalar(PROJECT_BINDING_SQL)
            .bind(BindingId::new_v7().as_uuid())
            .bind(owner_card_uid.as_uuid())
            .bind(owner_kind.wire_name())
            .bind(binding.subject_occurrence_key.as_str())
            .bind(binding.subject_card_uid.as_uuid())
            .bind(binding.verifier_uid.as_uuid())
            .bind(trigger_uid)
            .bind(trigger_digest)
            .bind(Json(&binding.operators))
            .bind(activation)
            .bind(cron)
            .bind(tz)
            .fetch_one(&mut **conn.transaction())
            .await?;
        ids.push(stored_binding_id(id)?);
    }
    Ok(ids)
}

/// List an owner Card version's binding identities, ordered by identity.
///
/// # Errors
/// Returns the database error when the read fails, or a decode error when a
/// stored identity is not a `UUIDv7`.
pub async fn owner_binding_ids(
    conn: &mut TenantConn<'_>,
    owner_card_uid: &CardUid,
) -> Result<Vec<BindingId>, sqlx::Error> {
    let rows: Vec<Uuid> = sqlx::query_scalar(OWNER_BINDING_IDS_SQL)
        .bind(owner_card_uid.as_uuid())
        .fetch_all(&mut **conn.transaction())
        .await?;
    rows.into_iter().map(stored_binding_id).collect()
}

/// Record a qualifying machine exchange and arm the owner's unarmed schedules.
///
/// The tenant token issuer calls this in the transaction that issues the
/// token, only for an API-key or workload `jwt-bearer` grant. It advances the
/// principal's `last_authenticated_at` to PostgreSQL's statement time — never
/// backward — when the principal is an active Card-bound Service or Agent,
/// then sets every still-null `schedule` cursor of that exact owner Card
/// version to its first boundary after that same database instant. Arming and
/// the later due predicate therefore share one clock, and the caller's wall
/// clock decides nothing. An already armed cursor is never moved, so a later
/// exchange renews activity without resetting or postponing the schedule. A
/// Card-free or inactive principal is left untouched.
///
/// A stored schedule that no longer parses leaves its cursor null — the
/// binding creates no work — and is reported through tracing rather than
/// refusing the caller's token.
///
/// # Errors
/// Returns the database error when a read or write fails; nothing is
/// committed here.
#[tracing::instrument(skip(conn), fields(operation = "verification.owner.activate"))]
pub async fn record_machine_authentication(
    conn: &mut TenantConn<'_>,
    principal_id: PrincipalId,
) -> Result<(), sqlx::Error> {
    let stamped: Option<(Uuid, DateTime<Utc>)> = sqlx::query_as(RECORD_AUTHENTICATION_SQL)
        .bind(principal_id.as_uuid())
        .fetch_optional(&mut **conn.transaction())
        .await?;
    let Some((owner, at)) = stamped else {
        return Ok(());
    };
    let unarmed: Vec<(Uuid, String, Option<String>)> = sqlx::query_as(UNARMED_SCHEDULES_SQL)
        .bind(owner)
        .fetch_all(&mut **conn.transaction())
        .await?;
    for (binding_id, cron, tz) in unarmed {
        let next = match BindingSchedule::parse(&cron, tz.as_deref())
            .and_then(|schedule| schedule.next_after(at))
        {
            Ok(next) => next,
            Err(error) => {
                tracing::error!(%binding_id, %error, "stored binding schedule cannot be armed");
                continue;
            }
        };
        sqlx::query(ARM_SCHEDULE_SQL)
            .bind(binding_id)
            .bind(next)
            .execute(&mut **conn.transaction())
            .await?;
    }
    Ok(())
}

/// How long a qualifying exchange keeps a binding owner runtime-active.
///
/// A value object, not runtime state: admission derives activity from the
/// principal row and this window on every query. The window is carried as a
/// duration only; the instant it is measured back from is PostgreSQL's, so no
/// cutoff is ever computed in Rust.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InactivityTimeout(i64);

impl InactivityTimeout {
    /// Build a window of `seconds`.
    #[must_use]
    pub const fn from_seconds(seconds: i64) -> Self {
        Self(seconds)
    }

    /// The window's length in whole seconds, as the activity predicate binds it.
    #[must_use]
    pub const fn seconds(self) -> i64 {
        self.0
    }
}

impl Default for InactivityTimeout {
    /// The spec default of [`DEFAULT_INACTIVITY_TIMEOUT_SECONDS`].
    fn default() -> Self {
        Self::from_seconds(DEFAULT_INACTIVITY_TIMEOUT_SECONDS)
    }
}

/// One binding's current admission gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingActivity {
    /// The binding.
    pub binding_id: BindingId,
    /// Exact owner Card version whose principal gates the binding.
    pub owner_card_uid: CardUid,
    /// The owner's Card-bound principal, absent if none exists.
    pub principal_id: Option<PrincipalId>,
    /// Last qualifying machine exchange, absent before the first.
    pub last_authenticated_at: Option<DateTime<Utc>>,
    /// Schedule cursor; absent before arming and for `observations_ready`.
    pub next_run_at: Option<DateTime<Utc>>,
    /// Whether new binding-created work may be admitted at database time.
    pub active: bool,
}

/// Raw decoded [`BINDING_ACTIVITY_SQL`] row, converted to [`BindingActivity`]
/// through the identity constructors before it leaves this module.
#[derive(sqlx::FromRow)]
struct BindingActivityRow {
    /// Stored binding UUID; must be version 7.
    binding_id: Uuid,
    /// Stored owner Card UID; must be version 7.
    owner_card_uid: Uuid,
    /// Owner principal id, absent without a Card-bound principal.
    principal_id: Option<Uuid>,
    /// Last qualifying machine exchange.
    last_authenticated_at: Option<DateTime<Utc>>,
    /// Schedule cursor.
    next_run_at: Option<DateTime<Utc>>,
    /// Derived admission gate.
    active: bool,
}

impl BindingActivityRow {
    /// Convert raw identities into their domain types.
    ///
    /// # Errors
    /// Returns [`sqlx::Error::Decode`] when the stored binding or owner UUID is
    /// not version 7.
    fn into_activity(self) -> Result<BindingActivity, sqlx::Error> {
        Ok(BindingActivity {
            binding_id: stored_binding_id(self.binding_id)?,
            owner_card_uid: CardUid::from_uuid(self.owner_card_uid)
                .map_err(|error| sqlx::Error::Decode(Box::new(error)))?,
            principal_id: self.principal_id.map(PrincipalId::new),
            last_authenticated_at: self.last_authenticated_at,
            next_run_at: self.next_run_at,
            active: self.active,
        })
    }
}

/// Read one binding's admission gate at PostgreSQL's statement time.
///
/// A binding is active only while its exact owner Card version is active,
/// that version's Card-bound principal is active, and the principal's last
/// qualifying exchange is newer than `timeout` before the database's own
/// statement time — the same clock the stamp was written on. Component
/// bindings resolve to their containing Service's principal, so they inherit
/// its activity; replicas sharing one principal share this one row; two A/B
/// Card versions have distinct principals and are gated independently.
/// Suspension or deletion is visible on the very next read, regardless of any
/// still-valid access token.
///
/// # Errors
/// Returns the database error when the read fails, or a decode error when the
/// stored binding or owner identity is not a `UUIDv7`. An unknown or
/// other-tenant binding is `Ok(None)`.
pub async fn binding_activity(
    conn: &mut TenantConn<'_>,
    binding_id: BindingId,
    timeout: InactivityTimeout,
) -> Result<Option<BindingActivity>, sqlx::Error> {
    let row: Option<BindingActivityRow> = sqlx::query_as(BINDING_ACTIVITY_SQL)
        .bind(binding_id.as_uuid())
        .bind(timeout.seconds())
        .fetch_optional(&mut **conn.transaction())
        .await?;
    row.map(BindingActivityRow::into_activity).transpose()
}

/// Decode a stored binding identity.
///
/// # Errors
/// Returns [`sqlx::Error::Decode`] when the stored UUID is not version 7.
fn stored_binding_id(value: Uuid) -> Result<BindingId, sqlx::Error> {
    BindingId::new(value).map_err(|error| sqlx::Error::Decode(Box::new(error)))
}

#[cfg(test)]
mod tests {
    //! Pure schedule parsing and boundary arithmetic.

    use chrono::TimeZone;

    use super::*;

    /// Build a UTC instant.
    ///
    /// # Panics
    /// Panics when the components do not name exactly one UTC instant.
    fn utc(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, h, mi, s)
            .single()
            .expect("test_setup: unambiguous UTC instant")
    }

    /// Prove the next boundary is strictly after the arming instant.
    ///
    /// # Panics
    /// Panics when an hourly schedule arms to anything but the next hour,
    /// including when armed exactly on a boundary.
    #[test]
    fn next_boundary_is_strictly_future() {
        let hourly = BindingSchedule::parse("0 * * * *", None).expect("hourly parses");
        assert_eq!(
            hourly.next_after(utc(2026, 9, 21, 0, 59, 59)).unwrap(),
            utc(2026, 9, 21, 1, 0, 0)
        );
        assert_eq!(
            hourly.next_after(utc(2026, 9, 21, 1, 0, 0)).unwrap(),
            utc(2026, 9, 21, 2, 0, 0)
        );
    }

    /// Prove the pattern's wall-clock fields are evaluated in its timezone.
    ///
    /// # Panics
    /// Panics when a daily 09:00 New York schedule does not arm to 13:00 UTC
    /// during daylight-saving time.
    #[test]
    fn schedule_honors_timezone() {
        let daily = BindingSchedule::parse("0 9 * * *", Some("America/New_York")).unwrap();
        assert_eq!(
            daily.next_after(utc(2026, 9, 21, 12, 0, 0)).unwrap(),
            utc(2026, 9, 21, 13, 0, 0)
        );
    }

    /// Refuse patterns and zones that could never be armed.
    ///
    /// # Panics
    /// Panics when a seconds field, garbage, or an unknown zone is accepted, or
    /// when an unreachable date yields an occurrence.
    #[test]
    fn unarmable_schedules_are_refused() {
        assert!(matches!(
            BindingSchedule::parse("0 0 * * * *", None),
            Err(ScheduleError::InvalidCron { .. })
        ));
        assert!(matches!(
            BindingSchedule::parse("every hour", None),
            Err(ScheduleError::InvalidCron { .. })
        ));
        assert!(matches!(
            BindingSchedule::parse("0 * * * *", Some("Mars/Olympus")),
            Err(ScheduleError::UnknownTimezone { .. })
        ));
        let never = BindingSchedule::parse("0 0 30 2 *", None).unwrap();
        assert!(matches!(
            never.next_after(utc(2026, 1, 1, 0, 0, 0)),
            Err(ScheduleError::NoOccurrence { .. })
        ));
    }

    /// A late claim within the occurrence's period keeps its fixed window and
    /// moves the cursor to the next future boundary.
    ///
    /// # Panics
    /// Panics when an hourly occurrence due at 01:00 claimed at 01:03 does not
    /// analyze `[00:00, 01:00)` or does not advance the cursor to 02:00.
    #[test]
    fn late_claim_keeps_the_due_window() {
        let hourly = BindingSchedule::parse("0 * * * *", None).unwrap();
        let occurrence = hourly
            .occurrence(utc(2026, 9, 21, 1, 0, 0), utc(2026, 9, 21, 1, 3, 0))
            .unwrap();
        assert_eq!(
            occurrence,
            ScheduledOccurrence {
                window: Some(DriftWindow {
                    start: utc(2026, 9, 21, 0, 0, 0),
                    end: utc(2026, 9, 21, 1, 0, 0),
                }),
                next_run_at: utc(2026, 9, 21, 2, 0, 0),
            }
        );
    }

    /// Once a later boundary has passed, the stored occurrence is missed: no
    /// window, no catch-up, and the cursor jumps to the next future boundary,
    /// including when the claim lands exactly on a boundary.
    ///
    /// # Panics
    /// Panics when a missed occurrence yields a window or the cursor is not
    /// strictly in the future.
    #[test]
    fn missed_occurrences_are_skipped_without_backfill() {
        let hourly = BindingSchedule::parse("0 * * * *", None).unwrap();
        let missed = hourly
            .occurrence(utc(2026, 9, 21, 1, 0, 0), utc(2026, 9, 21, 5, 30, 0))
            .unwrap();
        assert_eq!(missed.window, None);
        assert_eq!(missed.next_run_at, utc(2026, 9, 21, 6, 0, 0));

        let on_boundary = hourly
            .occurrence(utc(2026, 9, 21, 1, 0, 0), utc(2026, 9, 21, 2, 0, 0))
            .unwrap();
        assert_eq!(on_boundary.window, None);
        assert_eq!(on_boundary.next_run_at, utc(2026, 9, 21, 3, 0, 0));
    }

    /// A daily schedule's window spans the whole previous day.
    ///
    /// # Panics
    /// Panics when a daily 02:00 occurrence does not analyze the prior 24 hours.
    #[test]
    fn daily_window_spans_the_previous_period() {
        let daily = BindingSchedule::parse("0 2 * * *", None).unwrap();
        let occurrence = daily
            .occurrence(utc(2026, 9, 22, 2, 0, 0), utc(2026, 9, 22, 2, 0, 0))
            .unwrap();
        assert_eq!(
            occurrence.window,
            Some(DriftWindow {
                start: utc(2026, 9, 21, 2, 0, 0),
                end: utc(2026, 9, 22, 2, 0, 0),
            })
        );
        assert_eq!(occurrence.next_run_at, utc(2026, 9, 23, 2, 0, 0));
    }

    /// The default inactivity window is one day, in the seconds the activity
    /// predicate binds.
    ///
    /// # Panics
    /// Panics when the default window is not one day.
    #[test]
    fn inactivity_window_defaults_to_one_day() {
        assert_eq!(InactivityTimeout::default().seconds(), 86_400);
    }
}
