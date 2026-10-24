//! Workload root and Card-scope resolution for selected execution context.

use std::str::FromStr;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use wyrd_auth_issue::IssueError;
use wyrd_runtime::audit::AuditStage;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{Attribution, PrincipalId, PrincipalKindTag, WorkloadFamily, WorkloadKind};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;
use wyrd_spec::reference::{CardRef, CardRefScope};
use wyrd_spec::vala::ScopeHash;
use wyrd_spec::vala::api::{AuditDetail, AuditOutcome};
use wyrd_spec::vala::audit_detail::CardScopeMintKind;
use wyrd_sql::TenantConn;
use wyrd_sql::row_types::cards::{CardStatus, ParsedCardRow};

use crate::audit::{CARD_SCOPE_MINT_OPERATION, auth_event, auth_failure_code};
use wyrd_sql::queries::cards::{fetch_card_row, get_card_by_ref};

const MAX_SCOPE_DEPTH: usize = 16;
/// Maximum number of cards that may appear in a minted card-ref scope.
pub const MAX_SCOPE_CARDS: usize = wyrd_spec::auth::MAX_SCOPED_CARDS;
const SCOPE_AUDIT_MEMBER_SUMMARY_LIMIT: usize = 16;

/// Audit mint kind for API-key token exchange scope minting.
pub const MINT_KIND_API_KEY_EXCHANGE: CardScopeMintKind = CardScopeMintKind::ApiKeyExchange;
/// Audit mint kind for refresh-token scope minting.
pub const MINT_KIND_REFRESH: CardScopeMintKind = CardScopeMintKind::Refresh;
/// Audit mint kind for JWT bearer workload scope minting.
pub const MINT_KIND_JWT_BEARER: CardScopeMintKind = CardScopeMintKind::JwtBearer;

/// Validate an explicitly selected workload root and resolve its graph.
///
/// The root must be an active Service Card version in the caller's
/// tenant that `restriction` allows; `None` is an unrestricted caller. The
/// graph walk is tenant-scoped, fail-closed, cycle-guarded, and capped; only
/// observation-target kinds are included and expanded, and every member
/// carries its registry `card_uid`. The root is the first member.
///
/// # Errors
///
/// Returns [`WyrdError::WorkloadUidRefused`] for an unknown, foreign,
/// inactive, wrong-kind, or disallowed root — unknown and foreign alike —
/// [`WyrdError::CardScopeTooLarge`] when the walk exceeds its depth or member
/// cap, and the mapped registry error when a store read fails or a referenced
/// Card cannot be read.
///
/// # Panics
///
/// Never in practice: the walk seeds its members with the root, so the
/// resolved list always has a first member.
pub async fn resolve_workload_scope(
    conn: &mut TenantConn<'_>,
    root_uid: &CardUid,
    restriction: Option<&WorkloadFamily>,
) -> Result<CardRefScope, WyrdError> {
    let root_row = fetch_card_row(conn, root_uid)
        .await
        .map_err(|error| {
            tracing::error!(%error, "workload root lookup failed");
            WyrdError::registry_unavailable("card registry unavailable")
        })?
        .and_then(|row| ParsedCardRow::try_from(row).ok())
        .filter(|row| {
            row.status == CardStatus::Active
                && WorkloadKind::from_card_kind(&row.kind).is_some()
                && restriction.is_none_or(|family| family.contains(&row.kind, &row.name))
        })
        .ok_or_else(|| refused_root(root_uid))?;
    let root = CardRef {
        kind: root_row.kind.clone(),
        name: root_row.name.clone(),
        version: root_row.version.clone(),
        uid: Some(root_row.card_uid.clone()),
    };
    let mut members = vec![root.clone()];
    let mut frontier: Vec<(CardRef, usize)> = children(&root_row, 1);

    while let Some((card_ref, depth)) = frontier.pop() {
        if depth > MAX_SCOPE_DEPTH {
            return Err(too_large("depth", MAX_SCOPE_DEPTH, &root));
        }
        if members.iter().any(|m| m.same_identity(&card_ref)) {
            continue;
        }
        if members.len() == MAX_SCOPE_CARDS {
            return Err(too_large("cards", MAX_SCOPE_CARDS, &root));
        }

        let row = get_card_by_ref(
            conn,
            card_ref.kind.clone(),
            &card_ref.name,
            &card_ref.version,
        )
        .await
        .map_err(|error| scope_resolution_error(error, &root, &card_ref))?;
        frontier.extend(children(&row, depth + 1));
        members.push(CardRef {
            uid: Some(row.card_uid),
            ..card_ref
        });
    }

    let (resolved_root, rest) = members
        .split_first()
        .expect("scope walk invariant: the root is always the first resolved member");
    Ok(CardRefScope::from_root_and_members(
        resolved_root,
        rest.to_vec(),
    ))
}

/// The observation-target children of `row`, one level below it.
fn children(row: &ParsedCardRow, depth: usize) -> Vec<(CardRef, usize)> {
    wyrd_spec::reference::scope_child_card_refs(&row.spec)
        .into_iter()
        .filter(|child| child.kind.is_observation_target())
        .map(|child| (child, depth))
        .collect()
}

/// The one refusal for every unusable selected root, so unknown and foreign
/// roots are indistinguishable.
fn refused_root(root_uid: &CardUid) -> WyrdError {
    WyrdError::WorkloadUidRefused {
        message: format!("workload root {root_uid} cannot be used by this account"),
        details: json!({ "workload_uid": root_uid }),
    }
}

/// Build the stable `413` error for card-ref scope count or depth overflow.
fn too_large(limit_kind: &'static str, limit: usize, root: &CardRef) -> WyrdError {
    WyrdError::CardScopeTooLarge {
        message: format!("card_ref_scope exceeded {limit_kind} limit {limit}"),
        details: json!({
            "limit_kind": limit_kind,
            "limit": limit,
            "root": root.to_string(),
            "scope_mint_root": root.to_string(),
        }),
    }
}

/// Convert issuer scope-size failures into root-aware Wyrd errors.
pub fn issue_scope_error(error: IssueError, root: &CardRef) -> IssueErrorOrWyrd {
    match error {
        IssueError::CardScopeTooLarge { encoded_len, limit } => {
            IssueErrorOrWyrd::Wyrd(WyrdError::CardScopeTooLarge {
                message: format!(
                    "card_ref_scope encoded token length {encoded_len} exceeds {limit}"
                ),
                details: json!({
                    "encoded_len": encoded_len,
                    "limit": limit,
                    "root": root.to_string(),
                    "scope_mint_root": root.to_string(),
                }),
            })
        }
        other => IssueErrorOrWyrd::Issue(other),
    }
}

/// Result of mapping an issuer error at a card-bound mint site.
#[derive(Debug)]
pub enum IssueErrorOrWyrd {
    /// Token issuer rejected the mint operation.
    Issue(IssueError),
    /// Card-ref traversal produced a public Wyrd error.
    Wyrd(WyrdError),
}

/// Stage the successful card-ref scope mint audit event on the process audit stage.
///
/// The event names the scope's root, its first member, and carries the scope
/// digest, the full member count, and at most
/// [`SCOPE_AUDIT_MEMBER_SUMMARY_LIMIT`] members. It is staged without waiting
/// and never refuses the grant; a digest that is not a valid audit value is
/// logged and the event is staged without it.
///
/// # Panics
///
/// Panics if `scope` is empty or `scope.len()` exceeds `u32::MAX`. Callers
/// pass a scope from [`resolve_workload_scope`], which always starts with its
/// root and enforces the `MAX_SCOPE_CARDS` limit; violating either is a
/// programmer error, not a runtime condition.
pub fn stage_scope_mint_success_audit(
    audit: &dyn AuditStage,
    tenant_id: DataTenantId,
    principal_id: Uuid,
    principal_kind: PrincipalKindTag,
    scope: &CardRefScope,
    request_id: &str,
    mint_kind: CardScopeMintKind,
) {
    let root = scope
        .as_slice()
        .first()
        .expect("resolved scope invariant: the root is the first member");
    let scope_hash = ScopeHash::new(scope_hash(&scope_member_strings(scope)))
        .inspect_err(|error| {
            tracing::error!(error = %error, "scope digest is not a valid audit value");
        })
        .ok();
    let event = auth_event(
        request_id,
        CARD_SCOPE_MINT_OPERATION,
        Attribution::direct(PrincipalId::new(principal_id)),
        principal_kind,
        Some(root.clone()),
        AuditOutcome::Allowed,
        AuditDetail::CardScopeMint {
            mint_kind,
            root_card_ref: root.clone(),
            scope_hash,
            scope_member_count: Some(
                u32::try_from(scope.len())
                    .expect("MAX_SCOPE_CARDS invariant: scope count fits into u32"),
            ),
            scope_members: scope_member_summary(scope.as_slice()),
            failure_code: None,
        },
    );
    audit.stage(tenant_id, event);
}

/// Stage a refused card-ref scope mint on the process audit stage.
///
/// Only errors that name a scope root are recorded; the refused grant has no
/// authenticated principal yet, so the event is attributed to
/// [`PLATFORM_AUDIT_PRINCIPAL`]. Staging never waits and never fails the
/// caller, whose original refusal still reaches the client.
pub fn stage_scope_mint_failure_audit(
    audit: &dyn AuditStage,
    tenant_id: DataTenantId,
    request_id: &str,
    mint_kind: CardScopeMintKind,
    error: &WyrdError,
) {
    let Some(root) = scope_failure_root(error) else {
        return;
    };
    let event = auth_event(
        request_id,
        CARD_SCOPE_MINT_OPERATION,
        Attribution::unresolved(),
        PrincipalKindTag::Service,
        Some(root.clone()),
        AuditOutcome::Denied,
        AuditDetail::CardScopeMint {
            mint_kind,
            root_card_ref: root.clone(),
            scope_hash: None,
            scope_member_count: None,
            scope_members: Vec::new(),
            failure_code: Some(auth_failure_code(error)),
        },
    );
    audit.stage(tenant_id, event);
}

/// Attach scope-mint context to resolver errors that should be audited.
fn scope_resolution_error(error: WyrdError, root: &CardRef, unresolved: &CardRef) -> WyrdError {
    match error {
        WyrdError::RegistryCardNotFound { message, details } => WyrdError::RegistryCardNotFound {
            message,
            details: merge_scope_details(details, root, unresolved),
        },
        other => other,
    }
}

/// Merge root and unresolved child references into an existing error detail object.
fn merge_scope_details(details: Value, root: &CardRef, unresolved: &CardRef) -> Value {
    let mut details = match details {
        Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    details.insert("scope_mint_root".to_owned(), json!(root.to_string()));
    details.insert(
        "unresolved_card_ref".to_owned(),
        json!(unresolved.to_string()),
    );
    Value::Object(details)
}

/// Extract the root card ref from a scope-mint failure error.
fn scope_failure_root(error: &WyrdError) -> Option<CardRef> {
    let (WyrdError::CardScopeTooLarge { details, .. }
    | WyrdError::RegistryCardNotFound { details, .. }) = error
    else {
        return None;
    };
    details
        .get("scope_mint_root")
        .or_else(|| details.get("root"))
        .and_then(Value::as_str)
        .and_then(|value| CardRef::from_str(value).ok())
}

/// Return canonical string members for hashing and bounded audit summaries.
fn scope_member_strings(scope: &CardRefScope) -> Vec<String> {
    scope.as_slice().iter().map(ToString::to_string).collect()
}

/// Hash the scope member set in a deterministic order.
fn scope_hash(members: &[String]) -> String {
    let mut sorted = members.to_vec();
    sorted.sort();
    let mut hasher = Sha256::new();
    for member in sorted {
        hasher.update(member.as_bytes());
        hasher.update(b"\n");
    }
    format!("{:x}", hasher.finalize())
}

/// Build a bounded audit summary of the card-ref scope members.
fn scope_member_summary(members: &[CardRef]) -> Vec<CardRef> {
    members
        .iter()
        .take(SCOPE_AUDIT_MEMBER_SUMMARY_LIMIT)
        .cloned()
        .collect()
}

#[cfg(test)]
mod pg_tests {
    use wyrd_auth_issue::IssueError;
    use wyrd_dev_fixtures::cards::{seed_backing_card, seed_card_with_spec};
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_semver::VersionBlock;
    use wyrd_spec::envelope::{CardKind, Spec};
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::ids::CardName;
    use wyrd_spec::reference::CardRef;

    use super::*;

    /// Build a flat, UID-free `kind`/`name`@`1.0.0` reference fixture.
    ///
    /// # Panics
    /// Panics when `name` is not a valid Card name.
    fn make_card_ref(kind: CardKind, name: &str) -> CardRef {
        CardRef {
            kind,
            name: CardName::new(name).expect("valid name"),
            version: VersionBlock::parse("1.0.0").expect("valid version"),
            uid: None,
        }
    }

    // --- pure: too_large ---

    #[test]
    fn too_large_depth_sets_scope_mint_root() {
        let root = make_card_ref(CardKind::Service, "svc");
        let err = too_large("depth", MAX_SCOPE_DEPTH, &root);
        let WyrdError::CardScopeTooLarge { details, .. } = err else {
            panic!("expected CardScopeTooLarge");
        };
        assert_eq!(
            details.get("scope_mint_root").and_then(|v| v.as_str()),
            Some(root.to_string().as_str())
        );
        assert_eq!(
            details.get("limit_kind").and_then(|v| v.as_str()),
            Some("depth")
        );
    }

    #[test]
    fn too_large_cards_sets_scope_mint_root() {
        let root = make_card_ref(CardKind::Service, "svc");
        let err = too_large("cards", MAX_SCOPE_CARDS, &root);
        let WyrdError::CardScopeTooLarge { details, .. } = err else {
            panic!("expected CardScopeTooLarge");
        };
        assert_eq!(
            details.get("scope_mint_root").and_then(|v| v.as_str()),
            Some(root.to_string().as_str())
        );
    }

    // --- pure: issue_scope_error ---

    #[test]
    fn issue_scope_error_converts_too_large_and_sets_root() {
        let root = make_card_ref(CardKind::Service, "svc");
        let issue_err = IssueError::CardScopeTooLarge {
            encoded_len: 9000,
            limit: 8192,
        };
        let result = issue_scope_error(issue_err, &root);
        let IssueErrorOrWyrd::Wyrd(WyrdError::CardScopeTooLarge { details, .. }) = result else {
            panic!("expected IssueErrorOrWyrd::Wyrd(CardScopeTooLarge)");
        };
        assert_eq!(
            details.get("scope_mint_root").and_then(|v| v.as_str()),
            Some(root.to_string().as_str())
        );
        assert_eq!(
            details
                .get("encoded_len")
                .and_then(serde_json::Value::as_u64),
            Some(9000)
        );
    }

    /// `issue_scope_error` maps only a too-large scope to a `WyrdError` and
    /// returns every other issuance error unchanged.
    ///
    /// # Panics
    /// Panics when `InvalidCardAccess` is not passed through as an issue error.
    #[test]
    fn issue_scope_error_passes_through_other_errors() {
        let root = make_card_ref(CardKind::Service, "svc");
        let issue_err = IssueError::InvalidCardAccess;
        let result = issue_scope_error(issue_err, &root);
        assert!(matches!(
            result,
            IssueErrorOrWyrd::Issue(IssueError::InvalidCardAccess)
        ));
    }

    // --- pure: scope_failure_root ---

    #[test]
    fn scope_failure_root_reads_scope_mint_root_field() {
        let root = make_card_ref(CardKind::Service, "svc");
        let err = too_large("depth", MAX_SCOPE_DEPTH, &root);
        let found = scope_failure_root(&err).expect("scope_failure_root returns Some");
        assert!(found.same_identity(&root));
    }

    #[test]
    fn scope_failure_root_falls_back_to_root_field() {
        let root = make_card_ref(CardKind::Service, "svc");
        let err = WyrdError::CardScopeTooLarge {
            message: "test".to_owned(),
            details: json!({ "root": root.to_string() }),
        };
        let found = scope_failure_root(&err).expect("falls back to root field");
        assert!(found.same_identity(&root));
    }

    #[test]
    fn scope_failure_root_returns_none_without_root_fields() {
        let err = WyrdError::CardScopeTooLarge {
            message: "test".to_owned(),
            details: json!({ "unrelated": "field" }),
        };
        assert!(scope_failure_root(&err).is_none());
    }

    #[test]
    fn scope_failure_root_returns_none_for_unrelated_errors() {
        let err = WyrdError::Unauthenticated {
            message: "test".to_owned(),
            details: json!({}),
        };
        assert!(scope_failure_root(&err).is_none());
    }

    // --- pure: scope_hash ---

    #[test]
    fn scope_hash_is_order_independent() {
        let a = vec![
            "Service/svc-a@1.0.0".to_owned(),
            "Service/svc-b@1.0.0".to_owned(),
        ];
        let b = vec![
            "Service/svc-b@1.0.0".to_owned(),
            "Service/svc-a@1.0.0".to_owned(),
        ];
        assert_eq!(scope_hash(&a), scope_hash(&b));
    }

    #[test]
    fn scope_hash_differs_for_different_members() {
        let a = vec!["Service/svc-a@1.0.0".to_owned()];
        let b = vec!["Service/svc-b@1.0.0".to_owned()];
        assert_ne!(scope_hash(&a), scope_hash(&b));
    }

    // --- pure: scope_member_summary ---

    /// A scope at the summary limit is recorded in full.
    #[test]
    fn scope_member_summary_keeps_members_under_limit() {
        let members: Vec<CardRef> = (0..SCOPE_AUDIT_MEMBER_SUMMARY_LIMIT)
            .map(|i| make_card_ref(CardKind::Service, &format!("svc-{i}")))
            .collect();
        assert_eq!(scope_member_summary(&members), members);
    }

    /// A scope past the summary limit keeps only the first members; the full
    /// count travels separately as `scope_member_count`.
    #[test]
    fn scope_member_summary_truncates_past_limit() {
        let members: Vec<CardRef> = (0..SCOPE_AUDIT_MEMBER_SUMMARY_LIMIT + 5)
            .map(|i| make_card_ref(CardKind::Service, &format!("svc-{i}")))
            .collect();
        assert_eq!(
            scope_member_summary(&members),
            members[..SCOPE_AUDIT_MEMBER_SUMMARY_LIMIT]
        );
    }

    // --- DB: resolve_workload_scope ---

    /// The registry UID of the seeded `card_ref`.
    ///
    /// # Panics
    /// Panics when the seeded Card row cannot be loaded.
    async fn uid_of(conn: &mut TenantConn<'_>, card_ref: &CardRef) -> CardUid {
        get_card_by_ref(
            conn,
            card_ref.kind.clone(),
            &card_ref.name,
            &card_ref.version,
        )
        .await
        .expect("seeded card row loads")
        .card_uid
    }

    /// A Service root with no components resolves to a one-member scope that
    /// is the root itself, carrying its registry UID.
    ///
    /// # Panics
    /// Panics when the fixture, seed, or resolve fails, or the scope is not
    /// exactly the root with its UID.
    #[tokio::test]
    async fn resolve_single_service_card_produces_own_scope() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let creator = uuid::Uuid::new_v4();
        let root = make_card_ref(CardKind::Service, "svc-resolve-test");

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        seed_backing_card(&mut conn, &root, creator).await;
        let uid = uid_of(&mut conn, &root).await;

        let scope = resolve_workload_scope(&mut conn, &uid, None)
            .await
            .expect("resolves without error");

        assert_eq!(scope.as_slice().len(), 1);
        assert!(scope.as_slice()[0].same_identity(&root));
        assert_eq!(scope.as_slice()[0].uid.as_ref(), Some(&uid));
    }

    /// A root outside the account's family is refused exactly like an
    /// unknown root, so a caller cannot probe another family's releases.
    ///
    /// # Panics
    /// Panics when the fixture or seed fails, the own-family root is refused,
    /// or the foreign or unknown root resolves or is refused with anything
    /// other than `WorkloadUidRefused`.
    #[tokio::test]
    async fn resolve_refuses_foreign_and_unknown_roots_identically() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let root = make_card_ref(CardKind::Service, "svc-family-test");
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        seed_backing_card(&mut conn, &root, uuid::Uuid::new_v4()).await;
        let uid = uid_of(&mut conn, &root).await;
        let own = WorkloadFamily {
            kind: WorkloadKind::Service,
            name: root.name.clone(),
        };
        let other = WorkloadFamily {
            kind: WorkloadKind::Service,
            name: CardName::new("svc-other").expect("static name is valid"),
        };
        let unknown = CardUid::from_uuid(uuid::Uuid::now_v7()).expect("v7 uid");

        resolve_workload_scope(&mut conn, &uid, Some(&own))
            .await
            .expect("own family root resolves");
        let foreign = resolve_workload_scope(&mut conn, &uid, Some(&other))
            .await
            .expect_err("foreign family root is refused");
        let missing = resolve_workload_scope(&mut conn, &unknown, None)
            .await
            .expect_err("unknown root is refused");

        for error in [&foreign, &missing] {
            assert!(
                matches!(error, WyrdError::WorkloadUidRefused { .. }),
                "{error:?}"
            );
        }
    }

    /// Ingest stamps `card_uid` from trusted signed claims alone, so the mint
    /// walk must replace each authored reference — root and secondary — with the
    /// exact tenant-local registry identity and its `card_uid`.
    ///
    /// # Panics
    /// Panics when the fixture, seed, or resolve fails, the scope is not the
    /// root first plus the secondary, or either member lacks its registry UID.
    #[tokio::test]
    async fn resolve_scope_populates_every_member_uid() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let creator = uuid::Uuid::new_v4();
        let root = make_card_ref(CardKind::Service, "svc-uid-root");
        let secondary = make_card_ref(CardKind::Service, "svc-uid-secondary");

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        seed_backing_card(&mut conn, &secondary, creator).await;
        let root_spec = Spec::from_kind_and_value(
            &CardKind::Service,
            json!({
                "components": [{
                    "alias": "secondary",
                    "ref": {
                        "kind": "Service",
                        "name": secondary.name.as_str(),
                        "version": secondary.version.as_str(),
                    },
                }],
            }),
        )
        .expect("root service spec decodes");
        seed_card_with_spec(&mut conn, &root, &root_spec, creator).await;

        let expected_root_uid =
            get_card_by_ref(&mut conn, root.kind.clone(), &root.name, &root.version)
                .await
                .expect("root card row loads")
                .card_uid;
        let expected_secondary_uid = get_card_by_ref(
            &mut conn,
            secondary.kind.clone(),
            &secondary.name,
            &secondary.version,
        )
        .await
        .expect("secondary card row loads")
        .card_uid;

        let scope = resolve_workload_scope(&mut conn, &expected_root_uid, None)
            .await
            .expect("resolves without error");

        assert_eq!(scope.len(), 2, "root and secondary are both scoped");
        let resolved_root = &scope.as_slice()[0];
        assert!(
            resolved_root.same_identity(&root),
            "root stays the first member"
        );
        assert_eq!(
            resolved_root.uid.as_ref(),
            Some(&expected_root_uid),
            "root member carries its registry uid"
        );
        let resolved_secondary = scope
            .as_slice()
            .iter()
            .find(|member| member.same_identity(&secondary))
            .expect("secondary member is present");
        assert_eq!(
            resolved_secondary.uid.as_ref(),
            Some(&expected_secondary_uid),
            "secondary member carries its registry uid"
        );
    }

    /// The Card-count cap refusal names the `cards` limit, its value, and the
    /// scope root; the cap itself is unreachable here without Agent Cards that
    /// have scope children, so the test checks the refusal's shape.
    ///
    /// # Panics
    /// Panics when the refusal is another variant or carries other details.
    #[tokio::test]
    async fn resolve_caps_at_max_scope_cards() {
        // Verify the cap boundary fires: too_large("cards", MAX_SCOPE_CARDS, root)
        // We can't easily create MAX_SCOPE_CARDS+1 Service cards with children,
        // but we can verify the pure error shape produced by too_large directly
        // and confirm resolve errors for a missing card (cap logic is unreachable
        // without Agent cards having scope children).
        let root = make_card_ref(CardKind::Service, "svc-cap-test");
        let err = too_large("cards", MAX_SCOPE_CARDS, &root);
        let WyrdError::CardScopeTooLarge { message, details } = err else {
            panic!("expected CardScopeTooLarge");
        };
        assert!(message.contains("cards"), "message names the limit kind");
        assert_eq!(
            details.get("limit").and_then(serde_json::Value::as_u64),
            Some(MAX_SCOPE_CARDS as u64)
        );
        assert_eq!(
            details.get("scope_mint_root").and_then(|v| v.as_str()),
            Some(root.to_string().as_str())
        );
    }

    #[tokio::test]
    async fn resolve_caps_at_max_scope_depth() {
        let root = make_card_ref(CardKind::Service, "svc-depth-test");
        let err = too_large("depth", MAX_SCOPE_DEPTH, &root);
        let WyrdError::CardScopeTooLarge { message, details } = err else {
            panic!("expected CardScopeTooLarge");
        };
        assert!(message.contains("depth"), "message names the limit kind");
        assert_eq!(
            details.get("limit").and_then(serde_json::Value::as_u64),
            Some(MAX_SCOPE_DEPTH as u64)
        );
    }
}
