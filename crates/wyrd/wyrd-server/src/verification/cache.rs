//! The one resolution of a verification target, shared by direct verify,
//! submission acceptance, and the queued runner.
//!
//! Every Verifier execution first resolves its target: the exact Verifier and
//! its spec, the exact subject, the deployment environments the caller's owner
//! Service binds the Verifier to that subject under, and the tenant's SYSTEM
//! principal, or the
//! [`EnqueueRefusal`] that keeps it from running. [`VerifierCardCache::resolve`]
//! is the only way any path does that. It reads `wyrd.verification_target`,
//! whose `wyrd.verifier_readiness` is the one rule deciding whether a Verifier
//! can run, and caches each resolved target by tenant, Verifier, subject, and
//! owner Service.
//! Entries are never shared across tenants and are bounded by total bytes
//! with least-recently-used eviction, because every Wyrd-owned buffer is
//! bounded. Refusals are never cached, so a Card registered after one is seen
//! at once.
//!
//! A cached target is served only while this node hears every change to what
//! it was resolved from. Triggers on `wyrd.cards` (a status, version, or spec
//! hash change, or a delete), on `wyrd.drift_baselines` (a state change), and
//! on `wyrd.auth_service_accounts` (any SYSTEM row write) send a
//! [`TARGET_CHANNEL`] notification from inside the committing transaction, so
//! it is delivered only if the write commits. A Card or baseline notification
//! forgets every target naming that Card; a principal notification forgets
//! every target of the tenant. The listener's absence
//! or any lost connection forgets everything, so every resolution then reads
//! Postgres. A notification is delivered after its commit, so a resolution
//! racing a Card deletion or deprecation can still serve the old target for
//! that delivery latency.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use lru::LruCache;
use sqlx::PgConnection;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use wyrd_runtime::principal::PrincipalId;
use wyrd_spec::DataTenantId;
use wyrd_spec::card::verifier::VerifierSpec;
use wyrd_spec::envelope::Spec;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;
use wyrd_spec::reference::CardRef;
use wyrd_spec::verification::VerifierReadiness;
use wyrd_sql::queries::verification_target::{VerificationTarget, VerificationTargets};
use wyrd_sql::queries::verifier_executions::EnqueueRefusal;
use wyrd_sql::{CardRow, ParsedCardRow, SqlError, WyrdPostgres};

/// Postgres channel the target triggers notify on.
pub(crate) const TARGET_CHANNEL: &str = "wyrd_verification_target";

/// Total bytes of cached targets one process keeps.
const CACHE_BYTES: usize = 64 * 1024 * 1024;

/// Cache key: tenant, Verifier Card UID, subject Card UID, and the caller's
/// owner Service Card UID, if any.
type TargetKey = (DataTenantId, Uuid, Uuid, Option<Uuid>);

/// The exact Verifier and subject an execution resolved.
#[derive(Debug)]
pub(crate) struct Target {
    /// Exact Verifier reference.
    pub(crate) verifier: CardRef,
    /// The Verifier's building-block spec.
    pub(crate) spec: Arc<VerifierSpec>,
    /// Exact subject reference.
    pub(crate) subject: CardRef,
    /// The distinct `deployment_environment` selectors of every binding
    /// that attaches this Verifier to this subject: the owner Service's own
    /// when the subject is that Service, each of its component occurrences
    /// referencing the subject, and a subject Agent's own. Empty when none
    /// binds it, including when no owner was resolved.
    pub(crate) binding_environments: BTreeSet<Option<String>>,
    /// The tenant's active SYSTEM principal, which executes the Verifier;
    /// `None` when the tenant has none, which each caller refuses its own way.
    pub(crate) system_principal: Option<PrincipalId>,
}

impl Target {
    /// Decide what one `wyrd.verification_target` read resolves to.
    ///
    /// The database decided readiness and which Cards are not deleted, and
    /// [`VerificationTarget::refusal`] applies that answer, so this only
    /// parses what it resolved. A stored Card that does not parse
    /// is a row Wyrd never writes; it is logged and refused as unavailable
    /// rather than failing every execution that names it. Pure: no IO.
    fn resolved(read: VerificationTarget) -> Result<Self, EnqueueRefusal> {
        let unavailable = EnqueueRefusal::NotReady(VerifierReadiness::VerifierUnavailable);
        if let Some(refusal) = read.refusal() {
            return Err(refusal);
        }
        let owner = read.owner.and_then(Self::parse);
        let (Some(verifier), Some(subject)) = (read.verifier, read.subject) else {
            return Err(unavailable);
        };
        let verifier = Self::parse(verifier).ok_or(unavailable)?;
        let subject = Self::parse(subject).ok_or(EnqueueRefusal::SubjectUnavailable)?;
        let reference = exact_ref(&verifier);
        let binding_environments = binding_environments(owner.as_ref(), &subject, &verifier);
        let Spec::Verifier(spec) = verifier.spec else {
            return Err(unavailable);
        };
        Ok(Self {
            verifier: reference,
            spec: Arc::new(spec),
            subject: exact_ref(&subject),
            binding_environments,
            system_principal: read.system_principal.map(PrincipalId::new),
        })
    }

    /// Parse a stored Card row, logging and discarding one that does not.
    fn parse(row: CardRow) -> Option<ParsedCardRow> {
        let uid = row.card_uid;
        ParsedCardRow::try_from(row)
            .inspect_err(
                |error| tracing::warn!(card_uid = %uid, %error, "a stored Card does not parse"),
            )
            .ok()
    }
}

/// The effective deployment environment of an invocation that configured
/// `requested`, against the `bound` selectors of
/// [`Target::binding_environments`].
///
/// Equal invocation and binding values are that value; a sole value on either
/// side is used; neither leaves the work unscoped. Pure: no IO.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` naming `deployment_environment` when
/// `requested` is empty, when the matching bindings disagree, or when
/// `requested` conflicts with the binding's selector.
pub(crate) fn effective_environment(
    requested: Option<&str>,
    bound: &BTreeSet<Option<String>>,
) -> Result<Option<String>, WyrdError> {
    let refuse = |message: &str| WyrdError::Validation {
        message: message.to_owned(),
        details: serde_json::json!({
            "field": "deployment_environment",
            "requested": requested,
            "bound": bound,
        }),
    };
    if requested == Some("") {
        return Err(refuse(
            "deployment_environment must be non-empty or omitted",
        ));
    }
    if bound.len() > 1 {
        return Err(refuse(
            "the bindings of this Verifier on this subject select different deployment environments",
        ));
    }
    match (requested, bound.first().and_then(Option::as_ref)) {
        (Some(requested), Some(selector)) if requested != selector => Err(refuse(
            "the configured deployment environment conflicts with the binding's selector",
        )),
        (requested, selector) => Ok(requested.map(str::to_owned).or_else(|| selector.cloned())),
    }
}

/// The distinct `deployment_environment` selectors of the bindings attaching
/// `verifier` to `subject`; see [`Target::binding_environments`]. Bindings
/// are matched by pinned UID, which registration always records.
fn binding_environments(
    owner: Option<&ParsedCardRow>,
    subject: &ParsedCardRow,
    verifier: &ParsedCardRow,
) -> BTreeSet<Option<String>> {
    let mut bindings = Vec::new();
    if let Some(ParsedCardRow {
        card_uid,
        spec: Spec::Service(service),
        ..
    }) = owner
    {
        if *card_uid == subject.card_uid {
            bindings.extend(&service.verified_by);
        }
        for component in &service.components {
            if component
                .card_ref
                .as_card_ref()
                .and_then(|r| r.uid.as_ref())
                == Some(&subject.card_uid)
            {
                bindings.extend(&component.verified_by);
            }
        }
    }
    if let Spec::Agent(agent) = &subject.spec {
        bindings.extend(&agent.verified_by);
    }
    bindings
        .into_iter()
        .filter(|binding| {
            binding.verifier.as_card_ref().and_then(|r| r.uid.as_ref()) == Some(&verifier.card_uid)
        })
        .map(|binding| binding.deployment_environment.clone())
        .collect()
}

/// The exact reference of a stored Card row.
pub(crate) fn exact_ref(row: &ParsedCardRow) -> CardRef {
    CardRef {
        kind: row.kind.clone(),
        name: row.name.clone(),
        version: row.version.clone(),
        uid: Some(row.card_uid.clone()),
    }
}

/// Where a cache miss reads its target.
pub(crate) enum Db<'a> {
    /// A plain app-pool connection, acquired only on a miss; the read is one
    /// autocommit statement. Direct verify resolves this way.
    Pool(&'a WyrdPostgres),
    /// The caller's open transaction, bound to the same tenant, so the read
    /// sees what that transaction sees. Acceptance and the runner's claim
    /// resolve this way.
    Conn(&'a mut PgConnection),
}

/// One cached target and its charge against [`CACHE_BYTES`].
struct CachedTarget {
    /// The resolved target.
    target: Arc<Target>,
    /// The entry's charge: its spec's serialized size.
    bytes: usize,
}

/// The cache state guarded by one lock, so a fill and an invalidation are
/// ordered by it.
struct Entries {
    /// Cached targets, most recently used last.
    targets: LruCache<TargetKey, CachedTarget>,
    /// Sum of every held target's charge.
    bytes: usize,
    /// Bumped by every invalidation; a fill that began under an older value
    /// is not cached.
    generation: u64,
    /// Whether this node is currently listening for invalidations.
    live: bool,
}

/// Byte-bounded, tenant-keyed cache of resolved verification targets; see
/// the module docs for when it serves.
pub struct VerifierCardCache {
    /// Entries behind one short synchronous lock; no await happens under it.
    entries: Mutex<Entries>,
    /// Byte ceiling, [`CACHE_BYTES`] in production.
    capacity: usize,
}

impl Default for VerifierCardCache {
    /// An empty, not yet listening cache bounded at 64 MiB.
    fn default() -> Self {
        Self::with_capacity(CACHE_BYTES)
    }
}

impl VerifierCardCache {
    /// An empty, not yet listening cache holding at most `capacity` bytes.
    fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Mutex::new(Entries {
                targets: LruCache::unbounded(),
                bytes: 0,
                generation: 0,
                live: false,
            }),
            capacity,
        }
    }

    /// Locks the entries; a poisoned lock is recovered, because every holder
    /// leaves them consistent before anything under it could panic.
    fn entries(&self) -> MutexGuard<'_, Entries> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Resolve `verifier` on `subject` in `tenant` for a caller rooted at
    /// `owner`: the one read route of every Verifier execution. Only an
    /// invocation's admission passes `owner`; a worker re-resolving accepted
    /// work passes `None`, since its environment is already frozen.
    ///
    /// A target cached while listening is returned with no IO. Otherwise
    /// `wyrd.verification_target` reads it from `db` in one statement and
    /// [`Target::resolved`] applies the database's answer; a resolved target
    /// is cached unless an invalidation or lost listener raced the read.
    ///
    /// # Errors
    /// Returns [`SqlError::Connect`] when a [`Db::Pool`] miss cannot get a
    /// connection and [`SqlError::Query`] when the read fails or its
    /// readiness does not decode. A refusal is `Ok(Err(_))`.
    pub(crate) async fn resolve(
        &self,
        db: Db<'_>,
        tenant: DataTenantId,
        verifier: &CardUid,
        subject: &CardUid,
        owner: Option<&CardUid>,
    ) -> Result<Result<Arc<Target>, EnqueueRefusal>, SqlError> {
        let key = (
            tenant,
            verifier.as_uuid(),
            subject.as_uuid(),
            owner.map(CardUid::as_uuid),
        );
        if let Some(target) = self.cached(&key) {
            return Ok(Ok(target));
        }
        tracing::debug!("verification target cache miss");
        let generation = self.fill_generation();
        let read = match db {
            Db::Pool(postgres) => {
                let mut conn = postgres.conn().await?;
                VerificationTargets::new(&mut conn, tenant)
                    .read(verifier, subject, owner)
                    .await
            }
            Db::Conn(conn) => {
                VerificationTargets::new(conn, tenant)
                    .read(verifier, subject, owner)
                    .await
            }
        }
        .map_err(SqlError::Query)?;
        Ok(Target::resolved(read).map(|target| self.insert(key, generation, target)))
    }

    /// The target cached for `key`, marked most recently used, served only
    /// while listening.
    fn cached(&self, key: &TargetKey) -> Option<Arc<Target>> {
        let mut entries = self.entries();
        if !entries.live {
            return None;
        }
        entries
            .targets
            .get(key)
            .map(|cached| Arc::clone(&cached.target))
    }

    /// The generation a read must start under for its target to be cached,
    /// or `None` when the node is not listening.
    fn fill_generation(&self) -> Option<u64> {
        let entries = self.entries();
        entries.live.then_some(entries.generation)
    }

    /// Caches `target` for `key` when `generation` is the current one while
    /// listening, and returns it either way.
    ///
    /// A read that raced an invalidation is returned but not cached. Evicts
    /// least-recently-used targets until the total fits the byte ceiling; a
    /// target larger than the whole ceiling is not kept.
    fn insert(&self, key: TargetKey, generation: Option<u64>, target: Target) -> Arc<Target> {
        let target = Arc::new(target);
        let bytes = serde_json::to_vec(&*target.spec).map_or(0, |json| json.len());
        let mut entries = self.entries();
        if !entries.live || generation != Some(entries.generation) {
            return target;
        }
        entries.bytes += bytes;
        let cached = CachedTarget {
            target: Arc::clone(&target),
            bytes,
        };
        if let Some((_, replaced)) = entries.targets.push(key, cached) {
            entries.bytes -= replaced.bytes;
        }
        while entries.bytes > self.capacity {
            let Some((_, evicted)) = entries.targets.pop_lru() else {
                break;
            };
            entries.bytes -= evicted.bytes;
        }
        target
    }

    /// Forgets what a notification names: every target naming the Card for
    /// `tenant|card_uid`, every target of the tenant for `tenant`, or every
    /// target for any other payload.
    fn invalidate(&self, payload: &str) {
        tracing::debug!(payload, "verification target invalidated");
        let mut entries = self.entries();
        entries.generation = entries.generation.wrapping_add(1);
        let mut parts = payload.splitn(2, '|');
        let tenant = parts
            .next()
            .and_then(|tenant| Uuid::parse_str(tenant).ok())
            .and_then(|tenant| DataTenantId::new(tenant).ok());
        let card = match (tenant, parts.next().map(Uuid::parse_str)) {
            (Some(_), None) => None,
            (Some(_), Some(Ok(card))) => Some(card),
            _ => {
                entries.targets.clear();
                entries.bytes = 0;
                return;
            }
        };
        // ponytail: scans every cached target per notification; Card writes are
        // rare next to resolutions. Index targets by Card if invalidation shows up in profiles.
        let stale: Vec<TargetKey> = entries
            .targets
            .iter()
            .map(|(key, _)| *key)
            .filter(|(key_tenant, verifier, subject, owner)| {
                Some(*key_tenant) == tenant
                    && card.is_none_or(|card| {
                        *verifier == card || *subject == card || *owner == Some(card)
                    })
            })
            .collect();
        for key in stale {
            if let Some(evicted) = entries.targets.pop(&key) {
                entries.bytes -= evicted.bytes;
            }
        }
    }

    /// How many invalidations and listener resets this cache has applied, so
    /// a test can wait until a committed write's notification has arrived
    /// instead of sleeping.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn invalidations(&self) -> u64 {
        self.entries().generation
    }

    /// Forgets every target and records whether the node is now listening.
    fn reset(&self, live: bool) {
        let mut entries = self.entries();
        entries.generation = entries.generation.wrapping_add(1);
        entries.targets.clear();
        entries.bytes = 0;
        entries.live = live;
    }

    /// Listens for target invalidations from `postgres` until `shutdown`.
    ///
    /// Targets serve only while the listener is subscribed and its heartbeat
    /// answers, so a lost or silently stalled listener stops serving within
    /// the heartbeat bound; see [`wyrd_sql::NotificationListener::watch`].
    pub async fn watch(&self, postgres: &wyrd_sql::WyrdPostgres, shutdown: CancellationToken) {
        postgres
            .listener(TARGET_CHANNEL)
            .watch(
                shutdown,
                |live| self.reset(live),
                |payload| self.invalidate(payload),
            )
            .await;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use uuid::Uuid;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::card::verifier::VerifierSpec;
    use wyrd_spec::reference::CardRef;

    use std::collections::BTreeSet;

    use super::{Target, TargetKey, VerifierCardCache, effective_environment};

    /// A static Card reference of `kind`.
    ///
    /// # Panics
    /// Panics when the static reference is invalid.
    fn reference(kind: &str) -> CardRef {
        serde_json::from_value(serde_json::json!({
            "kind": kind, "name": "drift", "version": "1.0.0",
        }))
        .expect("static Card reference")
    }

    /// A one-assertion target.
    ///
    /// # Panics
    /// Panics when the static spec or reference is invalid.
    fn target() -> Target {
        Target {
            verifier: reference("Verifier"),
            spec: Arc::new(
                serde_json::from_value::<VerifierSpec>(serde_json::json!({
                    "requirements": [{
                        "id": "valid",
                        "statement": "The input is true.",
                        "check": {"kind": "assert", "path": "inputs.x", "operator": "equals", "expected": true}
                    }]
                }))
                .expect("static Verifier spec"),
            ),
            subject: reference("Service"),
            binding_environments: BTreeSet::new(),
            system_principal: None,
        }
    }

    /// A fresh key in `tenant`.
    fn key(tenant: DataTenantId) -> TargetKey {
        (tenant, Uuid::now_v7(), Uuid::now_v7(), Some(Uuid::now_v7()))
    }

    /// Caches a target for `key` as one read under the current generation
    /// would.
    fn fill(cache: &VerifierCardCache, key: TargetKey) {
        cache.insert(key, cache.fill_generation(), target());
    }

    /// Targets serve only while listening and within their key; a raced fill
    /// is not cached; a Card notification forgets targets naming that Card
    /// as Verifier or subject, a tenant notification every target of the
    /// tenant, a malformed one and a lost listener everything.
    #[test]
    fn targets_serve_only_while_listening_and_until_invalidated() {
        let cache = VerifierCardCache::default();
        let (tenant, other) = (DataTenantId::new_v7(), DataTenantId::new_v7());
        let (a, b) = (key(tenant), key(tenant));

        fill(&cache, a);
        assert!(cache.cached(&a).is_none(), "not listening");

        cache.reset(true);
        fill(&cache, a);
        fill(&cache, b);
        assert!(cache.cached(&a).is_some());
        assert!(
            cache.cached(&(other, a.1, a.2, a.3)).is_none(),
            "never across tenants"
        );

        cache.invalidate(&format!("{}|{}", tenant.as_uuid(), a.2));
        assert!(cache.cached(&a).is_none(), "a's subject changed");
        assert!(cache.cached(&b).is_some(), "b names neither Card");
        cache.invalidate(&format!("{}|{}", other.as_uuid(), b.1));
        assert!(cache.cached(&b).is_some(), "another tenant's Card");
        cache.invalidate(&format!("{}|{}", tenant.as_uuid(), b.1));
        assert!(cache.cached(&b).is_none(), "b's Verifier changed");
        fill(&cache, b);
        cache.invalidate(&format!("{}|{}", tenant.as_uuid(), b.3.expect("owned key")));
        assert!(cache.cached(&b).is_none(), "b's owner changed");

        fill(&cache, a);
        cache.invalidate(&tenant.as_uuid().to_string());
        assert!(cache.cached(&a).is_none(), "the tenant's principal changed");

        let stale = cache.fill_generation();
        cache.invalidate("malformed");
        cache.insert(a, stale, target());
        assert!(cache.cached(&a).is_none(), "a raced fill is not cached");

        fill(&cache, a);
        cache.reset(false);
        cache.reset(true);
        assert!(cache.cached(&a).is_none(), "a lost listener forgets");
    }

    /// The byte ceiling evicts the least recently used target first, and a
    /// target larger than the whole ceiling is returned but not kept.
    #[test]
    fn targets_are_evicted_least_recently_used_by_bytes() {
        let tenant = DataTenantId::new_v7();
        let size = serde_json::to_vec(&*target().spec)
            .expect("the spec serializes")
            .len();
        let cache = VerifierCardCache::with_capacity(2 * size);
        cache.reset(true);
        let (a, b, c) = (key(tenant), key(tenant), key(tenant));
        fill(&cache, a);
        fill(&cache, b);
        assert!(cache.cached(&a).is_some(), "a is now most recent");
        fill(&cache, c);
        assert!(cache.cached(&b).is_none(), "b was least recently used");
        assert!(cache.cached(&a).is_some() && cache.cached(&c).is_some());

        let tiny = VerifierCardCache::with_capacity(size - 1);
        tiny.reset(true);
        tiny.insert(a, tiny.fill_generation(), target());
        assert!(tiny.cached(&a).is_none(), "an oversized target is not kept");
    }

    /// Equal invocation and binding environments are accepted, a sole value
    /// is used, neither stays unscoped, and an empty value, a conflict, or
    /// disagreeing bindings are refused.
    #[test]
    fn effective_environment_follows_the_selector_rules() {
        let bound = |values: &[Option<&str>]| -> BTreeSet<Option<String>> {
            values
                .iter()
                .map(|value| value.map(str::to_owned))
                .collect()
        };
        let prod = Some("prod".to_owned());
        assert_eq!(
            effective_environment(Some("prod"), &bound(&[Some("prod")])).ok(),
            Some(prod.clone())
        );
        assert_eq!(
            effective_environment(None, &bound(&[Some("prod")])).ok(),
            Some(prod.clone())
        );
        assert_eq!(
            effective_environment(Some("prod"), &bound(&[None])).ok(),
            Some(prod.clone())
        );
        assert_eq!(
            effective_environment(Some("prod"), &bound(&[])).ok(),
            Some(prod)
        );
        assert_eq!(effective_environment(None, &bound(&[])).ok(), Some(None));
        for (requested, values) in [
            (Some(""), bound(&[])),
            (Some("staging"), bound(&[Some("prod")])),
            (None, bound(&[Some("prod"), Some("staging")])),
            (None, bound(&[None, Some("prod")])),
        ] {
            let error = effective_environment(requested, &values).expect_err("refused");
            assert_eq!(error.problem().code, "WYRD_SPEC_400_VALIDATION");
            assert_eq!(error.problem().details["field"], "deployment_environment");
        }
    }
}
