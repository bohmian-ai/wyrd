//! Current tenant authority: compiled grants shared by issuance and every
//! request.
//!
//! [`TenantAuthority`] compiles one tenant's bindings, groups, live accounts
//! and credentials at one authorization revision into per-account grant sets.
//! [`AuthorityCache`] keeps one per tenant in process memory, so a warm
//! request authorizes with a map lookup and no statement. A miss, a known
//! newer revision, or expired freshness triggers the sanctioned one-statement
//! reload; when that fails the request is refused as unavailable, never
//! allowed from token claims.
//!
//! Freshness is renewed only by background confirmation that reads the
//! committed revision. An unchanged revision renews the snapshot without
//! recompiling it; a changed one recompiles. Without confirmation a snapshot
//! serves for at most [`FRESHNESS_BOUND`] after its last successful
//! confirmation. Requests never renew it.

use std::collections::hash_map::Entry as Slot;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use uuid::Uuid;
use wyrd_auth_verify::VerifiedToken;
use wyrd_runtime::{
    DelegationStep, EffectivePermissions, Permission, PermissionSet, Principal, PrincipalId,
    RoleRef,
};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{
    Operation, PermissionScope, PrincipalKindTag, WorkloadFamily, compile_binding,
};
use wyrd_spec::error::WyrdError;
use wyrd_sql::queries::auth::{
    AuthorityState, authority_state, current_authorization_revision, workload_family,
};
use wyrd_sql::{TenantConn, WyrdPostgres};

/// How long a snapshot may keep serving after its last successful database
/// confirmation when no newer revision is known.
pub const FRESHNESS_BOUND: Duration = Duration::from_secs(300);

/// How often background confirmation reads each cached tenant's committed
/// revision; bounds propagation when a notification is missed.
pub const CONFIRM_INTERVAL: Duration = Duration::from_secs(2);

/// One live account's current authority.
#[derive(Clone, Debug)]
pub struct AccountAuthority {
    /// Account kind.
    pub kind: PrincipalKindTag,
    /// Union of every applicable direct, provider and group binding.
    pub grants: Arc<PermissionSet>,
    /// Names of every role bound to the account directly or through a group.
    pub roles: Arc<[RoleRef]>,
    /// Durable runtime-family restriction of a Service account.
    pub restriction: Option<WorkloadFamily>,
}

/// What one stored credential, revoked or not, bounds.
///
/// Revocation ends renewal; it does not withdraw a bearer the key already
/// minted, which stays bounded by the key's ceiling.
#[derive(Clone, Debug, PartialEq, Eq)]
enum KeyState {
    /// Its restrictive ceiling, when it carries one.
    Bounded(Option<Arc<PermissionSet>>),
    /// Its stored ceiling does not decode, so nothing it minted is usable.
    Corrupt,
}

/// A tenant's compiled authority at one authorization revision.
#[derive(Debug, Default)]
pub struct TenantAuthority {
    revision: i64,
    /// When this snapshot was compiled. A token issued in or after this
    /// second may name an account or credential committed after the load.
    loaded_at: chrono::DateTime<chrono::Utc>,
    accounts: HashMap<Uuid, AccountAuthority>,
    keys: HashMap<Uuid, KeyState>,
    /// Accounts whose stored authority does not decode, with the row naming
    /// it. They hold no authority and are absent from `accounts`, so one
    /// corrupt row fails only its own subjects, never the tenant.
    corrupt: HashMap<Uuid, String>,
    /// Account and credential ids a token named and this snapshot, loaded
    /// after that token reached this process, does not hold. Such an id is
    /// not committed state the load missed, so it never forces another
    /// reload; a consumed refresh row is the usual one.
    settled: Mutex<HashSet<Uuid>>,
}

/// Why a stored authorization row cannot be compiled.
#[derive(Debug, thiserror::Error)]
pub enum AuthorityError {
    /// The load statement failed.
    #[error("authority load failed: {0}")]
    Database(#[from] sqlx::Error),
    /// A stored role, scope, or ceiling does not decode.
    #[error("stored authorization for role {role:?} is corrupt")]
    Corrupt {
        /// Role or credential naming the corrupt row.
        role: String,
    },
}

impl From<AuthorityError> for WyrdError {
    fn from(error: AuthorityError) -> Self {
        match error {
            AuthorityError::Database(error) => crate::error::store_error(error),
            AuthorityError::Corrupt { role } => Self::RoleCorrupt {
                message: format!("stored authorization for {role} is corrupt"),
                details: serde_json::json!({ "role": role }),
            },
        }
    }
}

impl TenantAuthority {
    /// Loads and compiles the tenant's current authority in one statement on
    /// the caller's transaction.
    ///
    /// # Errors
    ///
    /// [`AuthorityError`] when the load fails or a stored row is corrupt.
    pub async fn load(conn: &mut TenantConn<'_>) -> Result<Self, AuthorityError> {
        Self::compile(authority_state(conn).await?)
    }

    /// Compiles one loaded state. Bindings of inactive or unknown subjects
    /// contribute nothing.
    ///
    /// A stored role, scope, or account kind that does not decode marks only
    /// the accounts it binds corrupt: they hold no authority and
    /// [`Self::check`] names the row, while every other account compiles. A
    /// binding naming no single subject and a credential ceiling that does
    /// not decode grant nothing.
    ///
    /// # Errors
    ///
    /// Infallible today; the `Result` keeps [`Self::load`]'s error type.
    pub fn compile(state: AuthorityState) -> Result<Self, AuthorityError> {
        let mut corrupt: HashMap<Uuid, String> = HashMap::new();
        let mut corrupt_groups: HashMap<Uuid, String> = HashMap::new();
        let mut direct: HashMap<Uuid, Vec<Permission>> = HashMap::new();
        let mut groups: HashMap<Uuid, Vec<Permission>> = HashMap::new();
        let mut direct_roles: HashMap<Uuid, Vec<RoleRef>> = HashMap::new();
        let mut group_roles: HashMap<Uuid, Vec<RoleRef>> = HashMap::new();
        for binding in state.bindings.0 {
            let subject = match (
                binding.user_id,
                binding.service_account_id,
                binding.group_id,
            ) {
                (Some(id), None, None) | (None, Some(id), None) => Some((id, false)),
                (None, None, Some(group)) => Some((group, true)),
                _ => None,
            };
            let decoded = serde_json::from_value::<Vec<Operation>>(binding.operations.clone())
                .ok()
                .zip(serde_json::from_value::<PermissionScope>(binding.scope.clone()).ok())
                .zip(RoleRef::new(&binding.role).ok());
            let (Some((id, group)), Some(((operations, scope), role))) = (subject, decoded) else {
                tracing::error!(role = %binding.role, "a stored role binding is corrupt; its subjects hold no authority");
                if let Some((id, group)) = subject {
                    let marked = if group {
                        &mut corrupt_groups
                    } else {
                        &mut corrupt
                    };
                    marked.insert(id, binding.role.clone());
                }
                continue;
            };
            let Ok(permissions) = compile_binding(&operations, &scope) else {
                continue;
            };
            let (target, names) = if group {
                (groups.entry(id), group_roles.entry(id))
            } else {
                (direct.entry(id), direct_roles.entry(id))
            };
            target.or_default().extend(permissions);
            names.or_default().push(role);
        }
        for membership in state.memberships.0 {
            let Some(member) = membership.user_id.or(membership.service_account_id) else {
                continue;
            };
            if let Some(role) = corrupt_groups.get(&membership.group_id) {
                corrupt.insert(member, role.clone());
            }
            if let Some(granted) = groups.get(&membership.group_id) {
                direct
                    .entry(member)
                    .or_default()
                    .extend(granted.iter().cloned());
            }
            if let Some(names) = group_roles.get(&membership.group_id) {
                direct_roles
                    .entry(member)
                    .or_default()
                    .extend(names.iter().cloned());
            }
        }
        let mut accounts = HashMap::new();
        for user in state.users.0 {
            if corrupt.contains_key(&user) {
                continue;
            }
            let mut grants: PermissionSet = direct
                .remove(&user)
                .unwrap_or_default()
                .into_iter()
                .collect();
            // Every real User may read and manage its own credentials;
            // ownership is enforced where the capability is checked.
            grants.insert(Permission::personal_credentials_read());
            grants.insert(Permission::personal_credentials_write());
            accounts.insert(
                user,
                AccountAuthority {
                    kind: PrincipalKindTag::User,
                    grants: Arc::new(grants),
                    roles: role_names(direct_roles.remove(&user)),
                    restriction: None,
                },
            );
        }
        for account in state.service_accounts.0 {
            if corrupt.contains_key(&account.id) {
                continue;
            }
            let kind = match account.principal_kind.as_str() {
                "service" => Some(PrincipalKindTag::Service),
                "tenant_admin" => Some(PrincipalKindTag::TenantAdmin),
                _ => None,
            };
            let restriction = workload_family(
                account.restriction_kind.as_deref(),
                account.restriction_name.as_deref(),
            );
            let (Some(kind), Ok(restriction)) = (kind, restriction) else {
                tracing::error!(account = %account.id, "a stored service account is corrupt; it holds no authority");
                corrupt.insert(account.id, format!("account {}", account.id));
                continue;
            };
            let grants = direct
                .remove(&account.id)
                .unwrap_or_default()
                .into_iter()
                .collect();
            accounts.insert(
                account.id,
                AccountAuthority {
                    kind,
                    grants: Arc::new(grants),
                    roles: role_names(direct_roles.remove(&account.id)),
                    restriction,
                },
            );
        }
        let mut keys = HashMap::new();
        for key in state.api_keys.0 {
            let ceiling = key
                .permission_ceiling
                .map(|expressions| {
                    expressions
                        .iter()
                        .map(|expression| expression.parse::<Permission>())
                        .collect::<Result<PermissionSet, _>>()
                        .map(Arc::new)
                })
                .transpose();
            let stateful = ceiling.map_or_else(
                |_| {
                    tracing::error!(credential = %key.id, "a stored credential ceiling is corrupt; the credential is unusable");
                    KeyState::Corrupt
                },
                KeyState::Bounded,
            );
            keys.insert(key.id, stateful);
        }
        Ok(Self {
            revision: state.revision,
            loaded_at: chrono::Utc::now(),
            accounts,
            keys,
            corrupt,
            settled: Mutex::default(),
        })
    }

    /// Refuses account `id` when its stored authority is corrupt.
    ///
    /// # Errors
    ///
    /// [`AuthorityError::Corrupt`] naming the row that does not decode.
    pub fn check(&self, id: PrincipalId) -> Result<(), AuthorityError> {
        match self.corrupt.get(&id.as_uuid()) {
            Some(role) => Err(AuthorityError::Corrupt { role: role.clone() }),
            None => Ok(()),
        }
    }

    /// The current grants of live account `id` of `kind`, loaded on the
    /// caller's transaction; `None` when it is missing, inactive or of another
    /// kind.
    ///
    /// # Errors
    ///
    /// [`WyrdError`] when the load fails or a stored row is corrupt.
    pub async fn current_grants(
        conn: &mut TenantConn<'_>,
        id: PrincipalId,
        kind: PrincipalKindTag,
    ) -> Result<Option<Arc<PermissionSet>>, WyrdError> {
        let authority = Self::load(conn).await?;
        authority.check(id)?;
        Ok(authority
            .account(id)
            .filter(|account| account.kind == kind)
            .map(|account| Arc::clone(&account.grants)))
    }

    /// The authorization revision this snapshot was compiled at.
    #[must_use]
    pub const fn revision(&self) -> i64 {
        self.revision
    }

    /// The live account `id`, or `None` when it is missing or inactive.
    #[must_use]
    pub fn account(&self, id: PrincipalId) -> Option<&AccountAuthority> {
        self.accounts.get(&id.as_uuid())
    }

    /// The current authority of the verified token's caller, or `None` when
    /// its subject, any delegation actor, or its API-key credential is no
    /// longer live.
    ///
    /// The subject's current grants are bounded by the signed ceiling, the
    /// presented key's current ceiling, and every delegation actor's current
    /// grants; actor identity alone is attribution, never authority.
    #[must_use]
    pub fn authorize(&self, verified: &VerifiedToken) -> Option<Principal> {
        let subject = &verified.principal;
        let account = self
            .account(subject.id)
            .filter(|a| a.kind == subject.kind.tag())?;
        let mut authority = EffectivePermissions::new(Arc::clone(&account.grants));
        if let Some(ceiling) = &verified.permission_ceiling {
            authority = authority.restricted_by(Arc::new(ceiling.clone()));
        }
        if let Some(credential) = subject.credential_id {
            // A credential that is not a stored key, such as a consumed
            // refresh row, adds no key ceiling.
            match self.keys.get(&credential) {
                Some(KeyState::Corrupt) => return None,
                Some(KeyState::Bounded(Some(ceiling))) => {
                    authority = authority.restricted_by(Arc::clone(ceiling));
                }
                Some(KeyState::Bounded(None)) | None => {}
            }
        }
        for step in &verified.delegation_chain {
            let actor = self
                .account(step.principal.id)
                .filter(|a| a.kind == step.principal.kind.tag())?;
            authority = authority.restricted_by(Arc::clone(&actor.grants));
        }
        let mut principal = subject.clone();
        principal.effective_permissions = authority;
        principal.roles = account.roles.to_vec();
        Some(principal)
    }

    /// Whether `principal`, acting through `chain`, holds here exactly the
    /// authority it held in `admitted`: the same live subject and actor
    /// grants and the same state of its credential. Any difference, including
    /// a widening, is a change, so an open stream never gains authority.
    #[must_use]
    pub fn unchanged_for(
        &self,
        admitted: &Self,
        principal: &Principal,
        chain: &[DelegationStep],
    ) -> bool {
        let same_account = |id: PrincipalId| {
            !self.corrupt.contains_key(&id.as_uuid())
                && match (self.account(id), admitted.account(id)) {
                    (Some(now), Some(then)) => now.kind == then.kind && now.grants == then.grants,
                    _ => false,
                }
        };
        same_account(principal.id)
            && chain.iter().all(|step| same_account(step.principal.id))
            && principal.credential_id.is_none_or(|credential| {
                self.keys.get(&credential) == admitted.keys.get(&credential)
            })
    }

    /// Whether `verified` may name state committed after this snapshot
    /// loaded: it was issued no earlier than the load second, and its
    /// account, an actor in its chain, or its credential is neither held
    /// here nor [settled](Self::settle).
    ///
    /// The snapshot holds every stored key, revoked ones included, so an
    /// unknown credential of a token older than the load is not a key.
    #[must_use]
    pub fn predates(&self, verified: &VerifiedToken) -> bool {
        if verified.iat.timestamp() < self.loaded_at.timestamp() {
            return false;
        }
        let settled = self
            .settled
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let unknown_account =
            |id: PrincipalId| self.account(id).is_none() && !settled.contains(&id.as_uuid());
        unknown_account(verified.principal.id)
            || verified
                .delegation_chain
                .iter()
                .any(|step| unknown_account(step.principal.id))
            || verified.principal.credential_id.is_some_and(|credential| {
                !self.keys.contains_key(&credential) && !settled.contains(&credential)
            })
    }

    /// Records that this snapshot, loaded after `verified` reached the
    /// process, already answers for every id it names, so later requests
    /// carrying the same token authorize from memory instead of reloading
    /// in its issuance second.
    ///
    /// Only call this with a snapshot loaded after the request carrying
    /// `verified` arrived: any key or account that token names was committed
    /// before it was issued, so one absent here does not exist.
    fn settle(&self, verified: &VerifiedToken) {
        let mut settled = self
            .settled
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let accounts = std::iter::once(verified.principal.id)
            .chain(
                verified
                    .delegation_chain
                    .iter()
                    .map(|step| step.principal.id),
            )
            .filter(|id| self.account(*id).is_none())
            .map(|id| id.as_uuid());
        settled.extend(accounts);
        if let Some(credential) = verified.principal.credential_id
            && !self.keys.contains_key(&credential)
        {
            settled.insert(credential);
        }
    }
}

/// An account's distinct bound role names, in name order.
fn role_names(names: Option<Vec<RoleRef>>) -> Arc<[RoleRef]> {
    let mut names = names.unwrap_or_default();
    names.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    names.dedup();
    names.into()
}

/// One tenant's cached snapshot and its freshness.
struct Entry {
    snapshot: Arc<TenantAuthority>,
    /// The last successful confirmation, or `None` once a malformed
    /// notification made it unknowable.
    confirmed_at: Option<Instant>,
    /// The highest revision this process has observed for the tenant, by
    /// notification, confirmation or load. The snapshot serves only while it
    /// is at least this current; an older load never lowers it.
    observed: i64,
}

impl Entry {
    /// Whether the snapshot may serve: current with every observed revision
    /// and confirmed within [`FRESHNESS_BOUND`].
    fn usable(&self) -> bool {
        self.snapshot.revision >= self.observed
            && self
                .confirmed_at
                .is_some_and(|confirmed| confirmed.elapsed() <= FRESHNESS_BOUND)
    }
}

/// Process-local current authority, one snapshot per tenant.
#[derive(Default)]
pub struct AuthorityCache {
    entries: Mutex<HashMap<DataTenantId, Entry>>,
    compiles: AtomicU64,
}

impl std::fmt::Debug for AuthorityCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthorityCache")
            .field("compiles", &self.compiles())
            .finish_non_exhaustive()
    }
}

impl AuthorityCache {
    fn entries(&self) -> MutexGuard<'_, HashMap<DataTenantId, Entry>> {
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// How many snapshots this cache has compiled, so tests can prove warm
    /// requests and unchanged confirmations never recompile.
    pub fn compiles(&self) -> u64 {
        self.compiles.load(Ordering::Relaxed)
    }

    /// The tenant's usable snapshot from memory, or `None` when it must be
    /// reloaded: absent, a newer revision is known, or freshness expired.
    #[must_use]
    pub fn cached(&self, tenant: DataTenantId) -> Option<Arc<TenantAuthority>> {
        let entries = self.entries();
        let entry = entries.get(&tenant)?;
        entry.usable().then(|| Arc::clone(&entry.snapshot))
    }

    /// The tenant's usable snapshot, reloading it with one statement when
    /// [`Self::cached`] has none.
    ///
    /// # Errors
    ///
    /// [`WyrdError::AuthorizationUnavailable`] when the reload fails.
    pub async fn acquire(
        &self,
        postgres: &WyrdPostgres,
        tenant: DataTenantId,
    ) -> Result<Arc<TenantAuthority>, WyrdError> {
        if let Some(snapshot) = self.cached(tenant) {
            return Ok(snapshot);
        }
        self.reload(postgres, tenant).await
    }

    /// Reloads the tenant's snapshot with one statement and installs it.
    ///
    /// # Errors
    ///
    /// [`WyrdError::AuthorizationUnavailable`] when the reload fails or reads
    /// a revision older than one this process has already observed.
    async fn reload(
        &self,
        postgres: &WyrdPostgres,
        tenant: DataTenantId,
    ) -> Result<Arc<TenantAuthority>, WyrdError> {
        let loaded = async {
            let mut conn = postgres.tenant_conn(tenant).await.ok()?;
            let snapshot = TenantAuthority::load(&mut conn).await;
            drop(conn.commit().await);
            snapshot
                .inspect_err(|error| tracing::warn!(%tenant, %error, "authority reload failed"))
                .ok()
        }
        .await
        .ok_or_else(|| WyrdError::authorization_unavailable("reload_failed"))?;
        self.install(tenant, loaded)
            .ok_or_else(|| WyrdError::authorization_unavailable("reload_superseded"))
    }

    /// Authorizes a verified token against the tenant's current authority.
    ///
    /// A token issued no earlier than the snapshot's load second may name an
    /// account, actor or key committed after the load, before this process
    /// has applied its notification; such a miss reloads once, the
    /// sanctioned cache-miss statement. The reloaded snapshot then answers
    /// for that token, so its later requests in the same second stay in
    /// memory. An older token refused by the snapshot is refused without a
    /// reload, so a removed account's tokens never cost a statement per
    /// request.
    ///
    /// # Errors
    ///
    /// [`WyrdError::AuthorizationUnavailable`] when no usable snapshot can be
    /// acquired, and [`WyrdError::PermissionUnauthenticated`] when the
    /// caller, an actor or its credential is no longer live.
    pub async fn authorize(
        &self,
        postgres: &WyrdPostgres,
        verified: &VerifiedToken,
    ) -> Result<Principal, WyrdError> {
        let tenant = verified.principal.tenant_id;
        let mut snapshot = self.acquire(postgres, tenant).await?;
        if snapshot.predates(verified) {
            snapshot = self.reload(postgres, tenant).await?;
            snapshot.settle(verified);
        }
        snapshot
            .authorize(verified)
            .ok_or_else(|| WyrdError::PermissionUnauthenticated {
                message: "the token's account or credential is no longer active".to_owned(),
                details: serde_json::json!({ "reason": "inactive" }),
            })
    }

    /// Installs a freshly loaded snapshot, confirmed now, unless a newer one
    /// is already cached; returns the snapshot now serving, or `None` when
    /// it is older than a revision this process has observed, so a load that
    /// started before a change can never make the obsolete state usable.
    pub fn install(
        &self,
        tenant: DataTenantId,
        loaded: TenantAuthority,
    ) -> Option<Arc<TenantAuthority>> {
        self.compiles.fetch_add(1, Ordering::Relaxed);
        let mut entries = self.entries();
        let entry = match entries.entry(tenant) {
            Slot::Occupied(slot) => {
                let entry = slot.into_mut();
                if entry.snapshot.revision <= loaded.revision {
                    entry.snapshot = Arc::new(loaded);
                    entry.confirmed_at = Some(Instant::now());
                }
                entry
            }
            Slot::Vacant(slot) => slot.insert(Entry {
                observed: loaded.revision,
                snapshot: Arc::new(loaded),
                confirmed_at: Some(Instant::now()),
            }),
        };
        entry.usable().then(|| Arc::clone(&entry.snapshot))
    }

    /// Records that the tenant's committed revision is at least `revision`,
    /// so no older snapshot serves until a load reaches it.
    fn observe(&self, tenant: DataTenantId, revision: i64) {
        if let Some(entry) = self.entries().get_mut(&tenant) {
            entry.observed = entry.observed.max(revision);
        }
    }

    /// Applies a committed-revision notification `<tenant>:<revision>`: a
    /// newer revision stops the cached snapshot from serving until reloaded.
    pub fn notify(&self, payload: &str) {
        let parsed = payload.split_once(':').and_then(|(tenant, revision)| {
            Some((
                DataTenantId::new(tenant.parse().ok()?).ok()?,
                revision.parse::<i64>().ok()?,
            ))
        });
        if let Some((tenant, revision)) = parsed {
            self.observe(tenant, revision);
        } else {
            // The revision is unknown, so no snapshot serves until a reload
            // confirms it again; observed revisions still fence older loads.
            tracing::warn!(payload, "malformed authorization notification");
            for entry in self.entries().values_mut() {
                entry.confirmed_at = None;
            }
        }
    }

    /// Confirms every cached tenant against its committed revision: an
    /// unchanged revision renews freshness without recompiling, a changed one
    /// stops the old snapshot serving at once and reloads. A failed read
    /// leaves the entry to age toward [`FRESHNESS_BOUND`]; a failed reload
    /// after a changed revision leaves it unusable.
    pub async fn confirm(&self, postgres: &WyrdPostgres) {
        let tenants: Vec<(DataTenantId, i64)> = self
            .entries()
            .iter()
            .map(|(tenant, entry)| (*tenant, entry.snapshot.revision))
            .collect();
        for (tenant, cached) in tenants {
            let Ok(mut conn) = postgres.tenant_conn(tenant).await else {
                continue;
            };
            let committed = current_authorization_revision(&mut conn).await;
            match committed {
                Ok(revision) if revision == cached => {
                    drop(conn.commit().await);
                    if let Some(entry) = self.entries().get_mut(&tenant)
                        && entry.snapshot.revision == revision
                    {
                        entry.confirmed_at = Some(Instant::now());
                    }
                }
                Ok(revision) => {
                    self.observe(tenant, revision);
                    let loaded = TenantAuthority::load(&mut conn).await;
                    drop(conn.commit().await);
                    match loaded {
                        Ok(loaded) => {
                            self.install(tenant, loaded);
                        }
                        Err(error) => tracing::warn!(%tenant, %error, "authority reload failed"),
                    }
                }
                Err(error) => {
                    tracing::warn!(%tenant, %error, "authority confirmation failed");
                }
            }
        }
    }

    /// Ages a tenant's confirmation by `age`, so a test can observe the
    /// freshness bound without waiting it out.
    #[cfg(test)]
    fn age(&self, tenant: DataTenantId, age: Duration) {
        if let Some(confirmed) = self
            .entries()
            .get_mut(&tenant)
            .and_then(|entry| entry.confirmed_at.as_mut())
        {
            *confirmed -= age;
        }
    }
}

/// The authority an open stream was admitted under, revalidated before each
/// later output.
///
/// Reacquiring through [`AuthorityCache::acquire`] answers from memory while
/// the snapshot is warm; a known newer revision or expired freshness costs
/// the sanctioned reload, and holding the admitted snapshot never extends its
/// freshness. The stream stops when authority is unavailable or the caller's
/// authority changed.
pub struct StreamAuthority {
    cache: Arc<AuthorityCache>,
    postgres: WyrdPostgres,
    principal: Principal,
    chain: Vec<DelegationStep>,
    admitted: Arc<TenantAuthority>,
}

impl StreamAuthority {
    /// Binds a stream to `principal`'s current authority, as the request
    /// that opened it was authorized.
    ///
    /// # Errors
    ///
    /// [`WyrdError::AuthorizationUnavailable`] when no usable snapshot can be
    /// acquired, and [`WyrdError::PermissionUnauthenticated`] when the
    /// caller is not a live account holding the snapshot's grants.
    pub async fn admit(
        cache: Arc<AuthorityCache>,
        postgres: WyrdPostgres,
        principal: Principal,
        chain: Vec<DelegationStep>,
    ) -> Result<Self, WyrdError> {
        let admitted = cache.acquire(&postgres, principal.tenant_id).await?;
        let current = admitted
            .account(principal.id)
            .is_some_and(|account| *account.grants == *principal.effective_permissions.grants());
        if !current {
            return Err(WyrdError::PermissionUnauthenticated {
                message: "the caller's account is not live in current authority".to_owned(),
                details: serde_json::json!({ "reason": "inactive" }),
            });
        }
        Ok(Self {
            cache,
            postgres,
            principal,
            chain,
            admitted,
        })
    }

    /// Confirms the stream may release its next output.
    ///
    /// # Errors
    ///
    /// [`WyrdError::AuthorizationUnavailable`] when no usable snapshot can be
    /// acquired, and [`WyrdError::PermissionUnauthenticated`] when the
    /// caller's authority changed since admission.
    pub async fn revalidate(&mut self) -> Result<(), WyrdError> {
        let current = self
            .cache
            .acquire(&self.postgres, self.principal.tenant_id)
            .await?;
        if Arc::ptr_eq(&current, &self.admitted) {
            return Ok(());
        }
        if !current.unchanged_for(&self.admitted, &self.principal, &self.chain) {
            return Err(authority_changed());
        }
        self.admitted = current;
        Ok(())
    }
}

/// The refusal for a stream whose caller's authority changed.
fn authority_changed() -> WyrdError {
    WyrdError::PermissionUnauthenticated {
        message: "the caller's authority changed while the stream was open".to_owned(),
        details: serde_json::json!({ "reason": "authority_changed" }),
    }
}

#[cfg(test)]
mod pg_tests {
    use std::time::Duration;

    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::error::WyrdError;

    use super::{AuthorityCache, FRESHNESS_BOUND};

    /// With PostgreSQL unreachable, a confirmed snapshot keeps serving until
    /// the freshness bound and is then refused as unavailable authority, as
    /// is a tenant this process never loaded; neither reads as a missing
    /// permission.
    ///
    /// # Panics
    /// Panics when the fixture fails, the snapshot stops serving inside the
    /// bound, or a refusal is anything but `AuthorizationUnavailable`.
    #[tokio::test]
    async fn an_outage_serves_only_within_the_bound_then_refuses_as_unavailable() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let other = fixture
            .seed_additional_tenant("authority-outage-other")
            .await
            .expect("tenant seeds");
        let (postgres, _vala) = fixture.fresh_runtime_handles().await.expect("handles");
        let cache = AuthorityCache::default();
        cache.acquire(&postgres, tenant).await.expect("first load");

        postgres.app_pool().close().await;
        cache.age(
            tenant,
            FRESHNESS_BOUND.saturating_sub(Duration::from_secs(1)),
        );
        cache
            .acquire(&postgres, tenant)
            .await
            .expect("inside the bound the confirmed snapshot serves");

        cache.age(tenant, Duration::from_secs(2));
        for (tenant, case) in [(tenant, "expired"), (other, "cold")] {
            let refused = cache.acquire(&postgres, tenant).await.expect_err(case);
            assert!(
                matches!(refused, WyrdError::AuthorizationUnavailable { .. }),
                "{case}: {refused:?}"
            );
        }
    }
    /// A token whose reload already settled what it names keeps answering
    /// from memory in its issuance second, so a later PostgreSQL outage does
    /// not refuse it as unavailable and no request recompiles authority.
    ///
    /// # Panics
    /// Panics when the fixture fails, a settled request reloads, or it is
    /// refused as unavailable.
    #[tokio::test]
    async fn a_settled_token_needs_no_reload_during_an_outage() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let (postgres, _vala) = fixture.fresh_runtime_handles().await.expect("handles");
        let cache = AuthorityCache::default();
        let token = wyrd_auth_verify::VerifiedToken {
            principal: wyrd_runtime::Principal::new(
                wyrd_runtime::PrincipalId::new(uuid::Uuid::now_v7()),
                wyrd_runtime::PrincipalKind::User,
                tenant,
                wyrd_runtime::permission::PermissionSet::new(),
            )
            .with_credential_id(Some(uuid::Uuid::now_v7())),
            permission_ceiling: None,
            delegation_chain: Vec::new(),
            exp: chrono::Utc::now(),
            iat: chrono::Utc::now(),
        };
        let first = cache
            .authorize(&postgres, &token)
            .await
            .expect_err("unknown");
        assert!(matches!(first, WyrdError::PermissionUnauthenticated { .. }));
        let compiled = cache.compiles();

        postgres.app_pool().close().await;
        for _ in 0..3 {
            let again = cache
                .authorize(&postgres, &token)
                .await
                .expect_err("unknown");
            assert!(
                matches!(again, WyrdError::PermissionUnauthenticated { .. }),
                "answered from memory: {again:?}"
            );
        }
        assert_eq!(cache.compiles(), compiled, "no request recompiled");
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;
    use sqlx::types::Json;
    use uuid::Uuid;
    use wyrd_spec::DataTenantId;
    use wyrd_sql::queries::auth::{AuthorityBinding, AuthorityState};

    use super::{AuthorityCache, AuthorityError, FRESHNESS_BOUND, TenantAuthority};

    fn state(revision: i64, user: Uuid) -> AuthorityState {
        AuthorityState {
            revision,
            bindings: Json(vec![AuthorityBinding {
                role: "viewer".to_owned(),
                operations: json!([{"resource": "cards", "action": "read"}]),
                scope: json!({"card": {"family": {"kind": "Repository", "name": "payments"}}}),
                user_id: Some(user),
                service_account_id: None,
                group_id: None,
            }]),
            memberships: Json(Vec::new()),
            users: Json(vec![user]),
            service_accounts: Json(Vec::new()),
            api_keys: Json(Vec::new()),
        }
    }

    /// Proves a snapshot stops serving on a known newer revision and after
    /// the freshness bound, and serves again only when reinstalled.
    ///
    /// # Panics
    /// Panics when stale or invalidated state keeps serving.
    #[test]
    fn snapshots_serve_only_while_fresh_and_current() {
        let tenant = DataTenantId::new_v7();
        let user = Uuid::now_v7();
        let cache = AuthorityCache::default();
        assert!(cache.cached(tenant).is_none(), "cold state is unavailable");

        cache.install(
            tenant,
            TenantAuthority::compile(state(3, user)).expect("compiles"),
        );
        assert!(cache.cached(tenant).is_some());
        assert_eq!(cache.compiles(), 1);

        cache.notify(&format!("{tenant}:3"));
        assert!(
            cache.cached(tenant).is_some(),
            "an equal revision changes nothing"
        );
        cache.notify(&format!("{tenant}:4"));
        assert!(
            cache.cached(tenant).is_none(),
            "a known newer revision refuses"
        );

        cache.install(
            tenant,
            TenantAuthority::compile(state(4, user)).expect("compiles"),
        );
        assert!(cache.cached(tenant).is_some());
        cache.age(tenant, FRESHNESS_BOUND + Duration::from_secs(1));
        assert!(cache.cached(tenant).is_none(), "expired freshness refuses");
    }

    /// While confirmation is interrupted, a snapshot serves only until the
    /// freshness bound since its last confirmation: serving requests never
    /// extends it, and only an installed reload confirms it again.
    ///
    /// # Panics
    /// Panics when a request extends the bound or a reload does not renew it.
    #[test]
    fn requests_never_extend_an_unconfirmed_snapshot() {
        let (tenant, user) = (DataTenantId::new_v7(), Uuid::now_v7());
        let cache = AuthorityCache::default();
        cache.install(
            tenant,
            TenantAuthority::compile(state(1, user)).expect("compiles"),
        );
        cache.age(
            tenant,
            FRESHNESS_BOUND.saturating_sub(Duration::from_secs(1)),
        );
        for _ in 0..3 {
            assert!(cache.cached(tenant).is_some(), "inside the bound it serves");
        }
        cache.age(tenant, Duration::from_secs(2));
        assert!(
            cache.cached(tenant).is_none(),
            "serving requests did not renew the confirmation"
        );
        cache.install(
            tenant,
            TenantAuthority::compile(state(1, user)).expect("compiles"),
        );
        assert!(cache.cached(tenant).is_some(), "a reload confirms it again");
    }

    /// A token naming a credential the snapshot does not hold predates it
    /// only when issued no earlier than its load, so the cache reloads once
    /// for a key committed after the load; an older token's unknown
    /// credential, such as a consumed refresh row, is not a key and is
    /// authorized with no key ceiling.
    ///
    /// # Panics
    /// Panics when a new unknown credential does not predate the snapshot, or
    /// an older one predates it or is refused.
    #[test]
    fn an_unknown_credential_reloads_only_for_a_newer_token() {
        let (tenant, user) = (DataTenantId::new_v7(), Uuid::now_v7());
        let authority = TenantAuthority::compile(state(1, user)).expect("compiles");
        let token = |credential: Option<Uuid>, iat| wyrd_auth_verify::VerifiedToken {
            principal: wyrd_runtime::Principal::new(
                wyrd_runtime::PrincipalId::new(user),
                wyrd_runtime::PrincipalKind::User,
                tenant,
                wyrd_runtime::permission::PermissionSet::new(),
            )
            .with_credential_id(credential),
            permission_ceiling: None,
            delegation_chain: Vec::new(),
            exp: chrono::Utc::now(),
            iat,
        };
        let earlier = authority.loaded_at - chrono::Duration::seconds(5);
        let unknown = Some(Uuid::now_v7());
        assert!(!authority.predates(&token(None, chrono::Utc::now())));
        assert!(authority.predates(&token(unknown, chrono::Utc::now())));
        assert!(!authority.predates(&token(unknown, earlier)));
        assert!(authority.authorize(&token(unknown, earlier)).is_some());
    }

    /// A load that read revision N before this process observed N+1, by
    /// notification or by confirmation, can never make N usable again: it
    /// installs as unusable until a load reaches N+1. A malformed
    /// notification stops every snapshot serving without losing that fence.
    ///
    /// # Panics
    /// Panics when an older load serves after a newer revision was observed.
    #[test]
    fn an_older_load_never_serves_after_a_newer_revision_is_observed() {
        let (tenant, user) = (DataTenantId::new_v7(), Uuid::now_v7());
        let cache = AuthorityCache::default();
        let load = |revision| TenantAuthority::compile(state(revision, user)).expect("compiles");
        assert!(cache.install(tenant, load(3)).is_some());

        cache.notify(&format!("{tenant}:4"));
        assert!(
            cache.install(tenant, load(3)).is_none(),
            "an in-flight load of 3 does not serve once 4 is observed"
        );
        assert!(cache.cached(tenant).is_none());

        // Confirmation read 5 and then failed to load it.
        cache.observe(tenant, 5);
        assert!(cache.install(tenant, load(4)).is_none());
        assert!(cache.cached(tenant).is_none(), "no obsolete grants serve");

        assert!(cache.install(tenant, load(5)).is_some());
        assert_eq!(cache.cached(tenant).map(|s| s.revision()), Some(5));
        assert!(
            cache
                .install(tenant, load(4))
                .is_some_and(|s| s.revision() == 5),
            "a late older load leaves the current snapshot serving"
        );

        cache.notify(&format!("{tenant}:6"));
        cache.notify("malformed");
        assert!(cache.cached(tenant).is_none(), "malformed: nothing serves");
        assert!(
            cache.install(tenant, load(5)).is_none(),
            "a malformed notification keeps the observed fence"
        );
        assert!(cache.install(tenant, load(6)).is_some());
    }

    /// A refreshed human token names its consumed refresh row, which is
    /// never a stored key. Once a reload for it has settled that, later
    /// requests in the same issuance second answer from memory, while a
    /// different unknown credential, possibly a key committed after the
    /// load, still predates the snapshot.
    ///
    /// # Panics
    /// Panics when a settled token still predates the snapshot or an
    /// unsettled credential stops predating it.
    #[test]
    fn a_settled_token_stops_reloading_in_its_issuance_second() {
        let (tenant, user) = (DataTenantId::new_v7(), Uuid::now_v7());
        let authority = TenantAuthority::compile(state(1, user)).expect("compiles");
        let token = |credential| wyrd_auth_verify::VerifiedToken {
            principal: wyrd_runtime::Principal::new(
                wyrd_runtime::PrincipalId::new(user),
                wyrd_runtime::PrincipalKind::User,
                tenant,
                wyrd_runtime::permission::PermissionSet::new(),
            )
            .with_credential_id(Some(credential)),
            permission_ceiling: None,
            delegation_chain: Vec::new(),
            exp: chrono::Utc::now(),
            iat: authority.loaded_at,
        };
        let refreshed = token(Uuid::now_v7());
        assert!(authority.predates(&refreshed), "first sight reloads once");
        authority.settle(&refreshed);
        for _ in 0..3 {
            assert!(!authority.predates(&refreshed), "settled: no reload");
        }
        assert!(authority.authorize(&refreshed).is_some());
        assert!(
            authority.predates(&token(Uuid::now_v7())),
            "a new key still reloads before it is trusted"
        );
    }

    /// A stream's caller keeps its authority across an unrelated revision,
    /// and loses it when its grants narrow, widen, or its account goes.
    ///
    /// # Panics
    /// Panics when a change is missed or an unrelated revision stops it.
    #[test]
    fn a_stream_keeps_only_unchanged_authority() {
        let (tenant, user) = (DataTenantId::new_v7(), Uuid::now_v7());
        let admitted = TenantAuthority::compile(state(1, user)).expect("compiles");
        let principal = wyrd_runtime::Principal::new(
            wyrd_runtime::PrincipalId::new(user),
            wyrd_runtime::PrincipalKind::User,
            tenant,
            wyrd_runtime::permission::PermissionSet::new(),
        );
        let unrelated = TenantAuthority::compile(state(2, user)).expect("compiles");
        assert!(unrelated.unchanged_for(&admitted, &principal, &[]));

        let mut widened = state(3, user);
        widened.bindings.0[0].operations = json!([
            {"resource": "cards", "action": "read"},
            {"resource": "cards", "action": "write"}
        ]);
        let widened = TenantAuthority::compile(widened).expect("compiles");
        assert!(!widened.unchanged_for(&admitted, &principal, &[]));

        let gone = TenantAuthority::compile(state(4, Uuid::now_v7())).expect("compiles");
        assert!(!gone.unchanged_for(&admitted, &principal, &[]));
    }

    /// A binding whose role operations or scope do not decode makes only its
    /// own subject corrupt, naming the role and holding no authority, while
    /// every other account in the tenant still compiles and holds its grants.
    ///
    /// # Panics
    /// Panics when a corrupt row grants anything, names another role, or
    /// fails another account.
    #[test]
    fn corrupt_bindings_name_their_role() {
        let (user, other) = (Uuid::now_v7(), Uuid::now_v7());
        for (operations, scope) in [
            (
                json!([{"resource": "cards", "action": "unknown"}]),
                json!("all"),
            ),
            (
                json!([{"resource": "cards", "action": "read"}]),
                json!({"nowhere": {}}),
            ),
        ] {
            let mut loaded = state(1, user);
            loaded.bindings.0[0].role = "bad_role".to_owned();
            loaded.bindings.0[0].operations = operations;
            loaded.bindings.0[0].scope = scope;
            loaded.bindings.0.extend(state(1, other).bindings.0);
            loaded.users.0.push(other);
            let authority = TenantAuthority::compile(loaded).expect("compiles");
            match authority.check(wyrd_runtime::PrincipalId::new(user)) {
                Err(AuthorityError::Corrupt { role }) => assert_eq!(role, "bad_role"),
                other => panic!("expected a corrupt role, got {other:?}"),
            }
            assert!(
                authority
                    .account(wyrd_runtime::PrincipalId::new(user))
                    .is_none()
            );
            authority
                .check(wyrd_runtime::PrincipalId::new(other))
                .expect("another account is not corrupt");
            assert!(
                authority
                    .account(wyrd_runtime::PrincipalId::new(other))
                    .is_some_and(|account| !account.grants.is_empty()),
                "another account keeps its grants"
            );
        }
    }
}
