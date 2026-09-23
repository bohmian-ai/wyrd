//! Resolve non-sibling `CardRef` values before registration writes begin.

use std::collections::{BTreeSet, HashMap};

use wyrd_spec::card::operator::{OperatorAction, OperatorSpec};
use wyrd_spec::card::trigger::{TriggerActivation, TriggerSpec};
use wyrd_spec::card::verifier::{VerificationBinding, VerifierImplementation, VerifierSpec};
use wyrd_spec::envelope::Spec;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;
use wyrd_spec::reference::{CardRef, CardRefIdentity, InlineableRef, Ref};
use wyrd_spec::refs::{ReferenceSlotVisitor, SlotValue};
use wyrd_spec::registry::CardSubmission;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::cards::{get_card_by_uid, select_card_uids_by_ref_batch};
use wyrd_sql::queries::verification::BindingSchedule;

/// Identity key used to look up a resolved external reference.
pub type ResolvedRefs = Vec<(CardRef, CardUid)>;

/// Resolve every external (non-sibling) `CardRef` to its `CardUid` under RLS.
///
/// Walks each submitted spec with the canonical [`ReferenceSlotVisitor`],
/// rejects loader-only paths and siblings the request did not submit, batches
/// the remaining identities into one RLS-scoped registry read, and then checks
/// every verification binding against the effective specs those refs name. All
/// of it runs before the caller opens its write transaction, so any refusal
/// here persists nothing.
///
/// # Errors
/// Returns `WYRD_REGISTRY_*_UNRESOLVED_PATH_REF` or
/// `WYRD_REGISTRY_*_UNRESOLVED_DEPENDENCY` when a reference is a loader-only
/// path, names an unsubmitted sibling, or has no Card in this tenant; the
/// binding refusals listed on [`EffectiveSpecs::validate_bindings`]; and the
/// underlying registry error when the batch read or a by-UID read fails.
pub async fn resolve_card_references(
    conn: &mut TenantConn<'_>,
    submissions: &[CardSubmission],
) -> Result<ResolvedRefs, WyrdError> {
    let siblings = sibling_identities(submissions)?;
    let mut refs = Vec::new();

    for submission in submissions {
        let spec = Spec::from_kind_and_value(&submission.kind, submission.spec.clone())
            .map_err(|error| WyrdError::registry_invalid_card_spec(error.to_string()))?;
        validate_and_collect_refs(&spec, &siblings, &mut refs)?;
    }

    refs.sort_by_key(display_ref);
    refs.dedup_by(|left, right| left.same_identity(right));

    let resolved_refs = select_card_uids_by_ref_batch(conn, &refs).await?;
    if let Some(missing) = refs.iter().find(|card_ref| {
        !resolved_refs
            .iter()
            .any(|(resolved, _)| resolved.same_identity(card_ref))
    }) {
        let identity = display_ref(missing);
        return Err(WyrdError::RegistryUnresolvedDependency {
            message: format!("card dependency {identity} was not found"),
            details: serde_json::json!({ "card_ref": identity }),
        });
    }

    let mut effective = EffectiveSpecs::new(submissions, resolved_refs)?;
    effective.validate_bindings(conn, submissions).await?;
    Ok(effective.resolved)
}

/// Reject a Trigger activation that cannot run the Verifier's implementation.
///
/// # Errors
/// Returns `WYRD_SPEC_400_TRIGGER_ACTIVATION_MISMATCH` unless a drift
/// Verifier runs on `schedule` or an eval Verifier runs on `observations_ready`.
fn check_activation(
    verifier: &VerifierSpec,
    trigger: &TriggerSpec,
    field: &str,
) -> Result<(), WyrdError> {
    let compatible = matches!(
        (&verifier.implementation, &trigger.activation),
        (
            VerifierImplementation::Drift(_),
            TriggerActivation::Schedule { .. }
        ) | (
            VerifierImplementation::Eval(_),
            TriggerActivation::ObservationsReady {}
        )
    );
    if compatible {
        return Ok(());
    }
    Err(WyrdError::SpecTriggerActivationMismatch {
        message: format!(
            "{field}.runs_on cannot activate a {} Verifier",
            verifier.implementation.kind_name()
        ),
        details: serde_json::json!({
            "field": format!("{field}.runs_on"),
            "implementation": verifier.implementation.kind_name(),
        }),
    })
}

/// Effective spec bodies for referenced binding targets, keyed by identity.
///
/// Owns the request's resolved external UIDs alongside the decoded bodies, so
/// binding validation is one method on this handle rather than a call graph
/// that re-threads the connection and the resolution table per lookup. Seeded
/// with the request's own submissions, so a sibling ref never reads the
/// registry; an external ref is loaded once by its resolved UID and cached.
struct EffectiveSpecs {
    /// Decoded spec per exact Card identity.
    specs: HashMap<CardRefIdentity, Spec>,
    /// External references this request already resolved to a `CardUid`.
    resolved: ResolvedRefs,
}

impl EffectiveSpecs {
    /// Decode every pinned submission into the identity cache.
    ///
    /// Submissions without a resolved space and version cannot be a sibling
    /// target, so they are skipped rather than cached under a partial identity.
    ///
    /// # Errors
    /// Returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC` for an undecodable spec.
    fn new(submissions: &[CardSubmission], resolved: ResolvedRefs) -> Result<Self, WyrdError> {
        let mut specs = HashMap::new();
        for submission in submissions {
            let (Some(space), Some(version)) = (
                &submission.metadata.space,
                submission.metadata.resolved_pin(),
            ) else {
                continue;
            };
            let identity = CardRef {
                kind: submission.kind.clone(),
                name: submission.metadata.name.clone(),
                version: version.clone(),
                space: Some(space.clone()),
                uid: None,
            }
            .identity_key();
            let spec = Spec::from_kind_and_value(&submission.kind, submission.spec.clone())
                .map_err(|error| WyrdError::registry_invalid_card_spec(error.to_string()))?;
            specs.insert(identity, spec);
        }
        Ok(Self { specs, resolved })
    }

    /// Check every submitted binding against the effective specs its refs name.
    ///
    /// Binding locations come from the single `Spec::binding_sites` owner, so
    /// an inline-Agent binding was already refused by request validation and
    /// only the three legal top-level locations reach here. Inline `runs_on`
    /// and `on_failure` bodies are checked in place; referenced bodies come
    /// from a sibling submission or this tenant's registry. This runs before
    /// the write transaction, so a rejection persists nothing.
    ///
    /// # Errors
    /// Returns `WYRD_SPEC_400_TRIGGER_ACTIVATION_MISMATCH` when the effective
    /// Trigger cannot run the effective Verifier implementation,
    /// `WYRD_REGISTRY_400_INVALID_CARD_SPEC` when a `schedule` Trigger cannot
    /// be armed,
    /// `WYRD_SPEC_400_UNSUPPORTED_OPERATOR_ACTION` when an effective
    /// `on_failure` Operator uses the non-invocable `workflow` action, and
    /// `WYRD_REGISTRY_400_INVALID_CARD_SPEC` for an undecodable submission.
    /// Registry read failures propagate unchanged.
    async fn validate_bindings(
        &mut self,
        conn: &mut TenantConn<'_>,
        submissions: &[CardSubmission],
    ) -> Result<(), WyrdError> {
        for submission in submissions {
            let spec = Spec::from_kind_and_value(&submission.kind, submission.spec.clone())
                .map_err(|error| WyrdError::registry_invalid_card_spec(error.to_string()))?;
            for site in spec.binding_sites().iter().filter(|site| !site.nested) {
                for (index, binding) in site.bindings.iter().enumerate() {
                    let field = format!("{}[{index}]", site.field);
                    self.validate_binding(conn, binding, &field).await?;
                }
            }
        }
        Ok(())
    }

    /// Check one binding's effective Trigger pairing, schedule, and `on_failure` actions.
    ///
    /// A `schedule` Trigger must parse as a five-field cron in a known IANA
    /// zone with a future occurrence, because the projected binding arms its
    /// cursor from that schedule on the owner's first machine exchange.
    ///
    /// # Errors
    /// Returns the activation-mismatch and unsupported-action refusals
    /// documented on [`EffectiveSpecs::validate_bindings`],
    /// `WYRD_REGISTRY_400_INVALID_CARD_SPEC` for an unarmable schedule, plus
    /// any registry read failure raised while loading a referenced body.
    async fn validate_binding(
        &mut self,
        conn: &mut TenantConn<'_>,
        binding: &VerificationBinding,
        field: &str,
    ) -> Result<(), WyrdError> {
        let verifier = self.load(conn, binding.verifier.as_card_ref()).await?;
        let trigger = match &binding.runs_on {
            InlineableRef::Inline(trigger) => Some(Spec::Trigger((**trigger).clone())),
            reference => self.load(conn, reference.as_card_ref()).await?,
        };
        if let (Some(Spec::Verifier(verifier)), Some(Spec::Trigger(trigger))) =
            (&verifier, &trigger)
        {
            check_activation(verifier, trigger, field)?;
        }
        if let Some(Spec::Trigger(TriggerSpec {
            activation: TriggerActivation::Schedule { cron, tz },
            ..
        })) = &trigger
        {
            BindingSchedule::parse(cron, tz.as_deref())
                .and_then(|schedule| schedule.next_after(chrono::Utc::now()))
                .map_err(|error| {
                    WyrdError::registry_invalid_card_spec(format!("{field}.runs_on: {error}"))
                })?;
        }
        for (index, operator) in binding.on_failure.iter().enumerate() {
            let operator = match operator {
                InlineableRef::Inline(operator) => Some(Spec::Operator((**operator).clone())),
                reference => self.load(conn, reference.as_card_ref()).await?,
            };
            if let Some(Spec::Operator(OperatorSpec {
                action: OperatorAction::Workflow { .. },
                ..
            })) = operator
            {
                let field = format!("{field}.on_failure[{index}]");
                return Err(WyrdError::SpecUnsupportedOperatorAction {
                    message: format!("{field} uses the workflow action, which is not invocable"),
                    details: serde_json::json!({ "field": field, "action": "workflow" }),
                });
            }
        }
        Ok(())
    }

    /// Return the effective spec a resolved ref names, loading it once if external.
    ///
    /// Returns `None` for an absent ref; unresolved refs were already rejected.
    ///
    /// # Errors
    /// Returns the registry error from loading an external Card by UID.
    async fn load(
        &mut self,
        conn: &mut TenantConn<'_>,
        card_ref: Option<&CardRef>,
    ) -> Result<Option<Spec>, WyrdError> {
        let Some(card_ref) = card_ref else {
            return Ok(None);
        };
        let identity = sibling_key(card_ref);
        if let Some(spec) = self.specs.get(&identity) {
            return Ok(Some(spec.clone()));
        }
        let Some(uid) = external_uid(card_ref, &self.resolved) else {
            return Ok(None);
        };
        let row = get_card_by_uid(conn, &uid).await?;
        self.specs.insert(identity, row.spec.clone());
        Ok(Some(row.spec))
    }
}

/// Collect the typed `CardRef` fields from one decoded spec.
#[cfg(test)]
fn collect_card_refs(spec: &Spec, output: &mut Vec<CardRef>) {
    let mut spec = spec.clone();
    ReferenceSlotVisitor::visit(&mut spec, |slot| match slot.value {
        SlotValue::Durable(reference) => {
            if let Ref::Ref(card_ref) = reference {
                output.push(card_ref.clone());
            }
        }
        SlotValue::InlineablePrompt(reference) => {
            if let InlineableRef::Ref(card_ref) = reference {
                output.push(card_ref.clone());
            }
        }
        SlotValue::InlineableAgent(reference) => {
            if let InlineableRef::Ref(card_ref) = reference {
                output.push(card_ref.clone());
            }
        }
        SlotValue::InlineableTrigger(reference) => {
            if let InlineableRef::Ref(card_ref) = reference {
                output.push(card_ref.clone());
            }
        }
        SlotValue::InlineableOperator(reference) => {
            if let InlineableRef::Ref(card_ref) = reference {
                output.push(card_ref.clone());
            }
        }
    });
}

/// Reject loader-only paths, validate typed sibling targets, and collect external refs.
fn validate_and_collect_refs(
    spec: &Spec,
    siblings: &BTreeSet<CardRefIdentity>,
    output: &mut Vec<CardRef>,
) -> Result<(), WyrdError> {
    let mut spec = spec.clone();
    let mut result = Ok(());
    ReferenceSlotVisitor::visit(&mut spec, |slot| {
        if result.is_err() {
            return;
        }
        match slot.value {
            SlotValue::Durable(reference) => match reference {
                Ref::Ref(card_ref) => output.push(card_ref.clone()),
                Ref::Sibling { sibling } => {
                    result = validate_sibling(sibling, siblings);
                }
                Ref::Path(path) => {
                    result = Err(unresolved_path_error(path));
                }
            },
            SlotValue::InlineablePrompt(reference) => {
                result = collect_inline_ref(reference, siblings, output);
            }
            SlotValue::InlineableAgent(reference) => {
                result = collect_inline_ref(reference, siblings, output);
            }
            SlotValue::InlineableTrigger(reference) => {
                result = collect_inline_ref(reference, siblings, output);
            }
            SlotValue::InlineableOperator(reference) => {
                result = collect_inline_ref(reference, siblings, output);
            }
        }
    });
    result
}

/// Collect one inlineable slot's external ref, or validate its sibling target.
///
/// Inline bodies carry no separate identity; their nested refs are visited as
/// their own slots by [`ReferenceSlotVisitor`].
///
/// # Errors
/// Returns `WYRD_REGISTRY_*_UNRESOLVED_DEPENDENCY` for an unsubmitted sibling
/// and `WYRD_REGISTRY_*_UNRESOLVED_PATH_REF` for a loader-only path.
fn collect_inline_ref<T>(
    reference: &InlineableRef<T>,
    siblings: &BTreeSet<CardRefIdentity>,
    output: &mut Vec<CardRef>,
) -> Result<(), WyrdError> {
    match reference {
        InlineableRef::Ref(card_ref) => {
            output.push(card_ref.clone());
            Ok(())
        }
        InlineableRef::Sibling { sibling } => validate_sibling(sibling, siblings),
        InlineableRef::Path(path) => Err(unresolved_path_error(path)),
        InlineableRef::Inline(_) => Ok(()),
    }
}

fn validate_sibling(
    sibling: &CardRef,
    siblings: &BTreeSet<CardRefIdentity>,
) -> Result<(), WyrdError> {
    if siblings.contains(&sibling_key(sibling)) {
        Ok(())
    } else {
        Err(WyrdError::RegistryUnresolvedDependency {
            message: format!(
                "sibling card dependency {} was not submitted",
                display_ref(sibling)
            ),
            details: serde_json::json!({ "card_ref": display_ref(sibling) }),
        })
    }
}

fn unresolved_path_error(path: &std::path::Path) -> WyrdError {
    WyrdError::RegistryUnresolvedPathRef {
        message: format!(
            "card reference path `{}` must be rewritten by the loader",
            path.display()
        ),
        details: serde_json::json!({ "path": path.display().to_string() }),
    }
}

/// Bind external and already-minted sibling UIDs into every embedded reference.
pub fn bind_card_references(
    spec: &mut Spec,
    external: &ResolvedRefs,
    siblings: &HashMap<CardRefIdentity, CardUid>,
) -> Result<(), WyrdError> {
    let mut result = Ok(());
    ReferenceSlotVisitor::visit(spec, |slot| {
        if result.is_err() {
            return;
        }
        result = match slot.value {
            SlotValue::Durable(reference) => bind_ref(reference, external, siblings),
            SlotValue::InlineablePrompt(reference) => {
                bind_inline_ref(reference, external, siblings)
            }
            SlotValue::InlineableAgent(reference) => bind_inline_ref(reference, external, siblings),
            SlotValue::InlineableTrigger(reference) => {
                bind_inline_ref(reference, external, siblings)
            }
            SlotValue::InlineableOperator(reference) => {
                bind_inline_ref(reference, external, siblings)
            }
        };
    });
    result
}

fn bind_ref(
    reference: &mut Ref,
    external: &ResolvedRefs,
    siblings: &HashMap<CardRefIdentity, CardUid>,
) -> Result<(), WyrdError> {
    match reference {
        Ref::Ref(card_ref) => {
            card_ref.uid = external_uid(card_ref, external);
            require_uid(card_ref)
        }
        Ref::Sibling { sibling } => {
            let uid = siblings.get(&sibling_key(sibling)).cloned();
            let mut card_ref = sibling.clone();
            card_ref.uid = uid;
            require_uid(&card_ref)?;
            *reference = Ref::Ref(card_ref);
            Ok(())
        }
        Ref::Path(path) => Err(unresolved_path_error(path)),
    }
}

fn bind_inline_ref<T>(
    reference: &mut InlineableRef<T>,
    external: &ResolvedRefs,
    siblings: &HashMap<CardRefIdentity, CardUid>,
) -> Result<(), WyrdError> {
    match reference {
        InlineableRef::Ref(card_ref) => {
            card_ref.uid = external_uid(card_ref, external);
            require_uid(card_ref)
        }
        InlineableRef::Sibling { sibling } => {
            let uid = siblings.get(&sibling_key(sibling)).cloned();
            let mut card_ref = sibling.clone();
            card_ref.uid = uid;
            require_uid(&card_ref)?;
            *reference = InlineableRef::Ref(card_ref);
            Ok(())
        }
        InlineableRef::Inline(_) => Ok(()),
        InlineableRef::Path(path) => Err(unresolved_path_error(path)),
    }
}

fn external_uid(card_ref: &CardRef, external: &ResolvedRefs) -> Option<CardUid> {
    external
        .iter()
        .find(|(resolved, _)| resolved.same_identity(card_ref))
        .map(|(_, uid)| uid.clone())
}

fn require_uid(card_ref: &CardRef) -> Result<(), WyrdError> {
    if card_ref.uid.is_some() {
        Ok(())
    } else {
        Err(WyrdError::RegistryUnresolvedDependency {
            message: format!("card dependency {} was not resolved", display_ref(card_ref)),
            details: serde_json::json!({ "card_ref": display_ref(card_ref) }),
        })
    }
}

/// Collect exact submitted identities eligible for sibling references.
///
/// Auto-versioned and scoped submissions cannot be sibling targets because a
/// sibling reference carries an exact version before registration resolves.
fn sibling_identities(
    submissions: &[CardSubmission],
) -> Result<BTreeSet<CardRefIdentity>, WyrdError> {
    submissions
        .iter()
        .map(|submission| {
            let space = submission.metadata.space.as_ref().ok_or_else(|| {
                WyrdError::registry_invalid_card_spec("metadata.space is required")
            })?;
            Ok(submission.metadata.resolved_pin().cloned().map(|version| {
                CardRef {
                    kind: submission.kind.clone(),
                    name: submission.metadata.name.clone(),
                    version,
                    space: Some(space.clone()),
                    uid: None,
                }
                .identity_key()
            }))
        })
        .collect::<Result<Vec<_>, WyrdError>>()
        .map(|identities| identities.into_iter().flatten().collect())
}

/// Return the exact identity used for sibling matching and UID binding.
fn sibling_key(card_ref: &CardRef) -> CardRefIdentity {
    card_ref.identity_key()
}

/// Format a reference for deterministic comparison and actionable errors.
fn display_ref(card_ref: &CardRef) -> String {
    format!(
        "{}/{}/{}@{}",
        card_ref.kind.wire_name(),
        card_ref
            .space
            .as_ref()
            .map_or("<missing-space>", |space| space.as_str()),
        card_ref.name,
        card_ref.version
    )
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap};

    use uuid::Uuid;
    use wyrd_semver::{VersionBlock, VersionSpec};
    use wyrd_spec::api_version::ApiVersion;
    use wyrd_spec::envelope::{CardKind, Metadata, Spec};
    use wyrd_spec::ids::CardUid;
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::registry::CardSubmission;

    use super::{
        bind_card_references, collect_card_refs, sibling_identities, validate_and_collect_refs,
    };

    fn prompt_ref(name: &str) -> CardRef {
        CardRef {
            kind: CardKind::Prompt,
            name: name.parse().expect("test_setup: reference name is valid"),
            version: VersionBlock::parse("1.0.0").expect("test_setup: reference version is valid"),
            space: Some(
                "default"
                    .parse()
                    .expect("test_setup: reference space is valid"),
            ),
            uid: None,
        }
    }

    fn submission(name: &str) -> CardSubmission {
        CardSubmission {
            api_version: ApiVersion::v1(),
            kind: CardKind::Prompt,
            metadata: Metadata {
                name: name.parse().expect("test_setup: submission name is valid"),
                version: Some(VersionSpec::Pin(
                    VersionBlock::parse("1.0.0").expect("test_setup: submission version is valid"),
                )),
                bump: None,
                space: Some(
                    "default"
                        .parse()
                        .expect("test_setup: submission space is valid"),
                ),
                uid: None,
                labels: Default::default(),
                annotations: Default::default(),
                spec_hash: None,
                artifact_hash: None,
                origin: None,
            },
            spec: serde_json::json!({
                "provider": "openai",
                "model": "gpt-4o",
                "messages": ["hello"]
            }),
            artifacts: Vec::new(),
        }
    }

    fn service_spec(card_ref: CardRef) -> Spec {
        Spec::from_kind_and_value(
            &CardKind::Service,
            serde_json::json!({
                "components": [{"alias": "child", "ref": card_ref}]
            }),
        )
        .expect("test_setup: service spec is valid")
    }

    fn sibling_service_spec(card_ref: CardRef) -> Spec {
        Spec::from_kind_and_value(
            &CardKind::Service,
            serde_json::json!({
                "components": [{
                    "alias": "child",
                    "ref": {"sibling": card_ref}
                }]
            }),
        )
        .expect("test_setup: sibling service spec is valid")
    }

    #[test]
    fn sibling_prompt_reference_is_not_external() {
        let prompt = prompt_ref("child");
        let spec = Spec::from_kind_and_value(
            &CardKind::Agent,
            serde_json::json!({"prompt": {"sibling": prompt.clone()}}),
        )
        .expect("test_setup: sibling agent spec is valid");
        let siblings = BTreeSet::from([prompt.identity_key()]);
        let mut refs = Vec::new();

        validate_and_collect_refs(&spec, &siblings, &mut refs)
            .expect("test_setup: sibling prompt is accepted");

        assert!(refs.is_empty());
    }

    #[test]
    fn sibling_identity_requires_space() {
        let mut child = submission("child");
        child.metadata.space = None;

        let error =
            sibling_identities(&[child]).expect_err("test_setup: missing space must be rejected");

        assert_eq!(error.code(), "WYRD_REGISTRY_400_INVALID_CARD_SPEC");
    }

    #[test]
    fn collect_typed_card_refs() {
        let expected = prompt_ref("child");
        let spec = service_spec(expected.clone());
        let mut refs = Vec::new();

        collect_card_refs(&spec, &mut refs);

        assert_eq!(refs, vec![expected]);
    }

    #[test]
    fn bind_sibling_uid_without_json_shape_sniffing() {
        let expected = prompt_ref("child");
        let uid = CardUid::from_uuid(Uuid::now_v7()).expect("test_setup: UUIDv7 is valid");
        let mut spec = sibling_service_spec(expected.clone());
        let siblings = HashMap::from([(expected.identity_key(), uid.clone())]);

        bind_card_references(&mut spec, &Vec::new(), &siblings)
            .expect("test_setup: binding succeeds");

        let mut refs = Vec::new();
        collect_card_refs(&spec, &mut refs);
        assert_eq!(refs[0].uid, Some(uid));
    }

    #[test]
    fn bind_external_uid_for_exact_reference() {
        let expected = prompt_ref("external");
        let uid = CardUid::from_uuid(Uuid::now_v7()).expect("test_setup: UUIDv7 is valid");
        let mut spec = service_spec(expected.clone());

        bind_card_references(
            &mut spec,
            &vec![(expected.clone(), uid.clone())],
            &HashMap::new(),
        )
        .expect("test_setup: binding succeeds");

        let mut refs = Vec::new();
        collect_card_refs(&spec, &mut refs);
        assert_eq!(refs[0].uid, Some(uid));
    }

    #[test]
    fn external_and_sibling_same_identity_bind_from_separate_sources() {
        let expected = prompt_ref("shared");
        let external_uid = CardUid::from_uuid(Uuid::now_v7()).expect("external UUIDv7 is valid");
        let sibling_uid = CardUid::from_uuid(Uuid::now_v7()).expect("sibling UUIDv7 is valid");
        let mut spec = Spec::from_kind_and_value(
            &CardKind::Service,
            serde_json::json!({
                "components": [
                    {"alias": "external", "ref": expected.clone()},
                    {"alias": "sibling", "ref": {"sibling": expected.clone()}}
                ]
            }),
        )
        .expect("test_setup: mixed service spec is valid");

        bind_card_references(
            &mut spec,
            &vec![(expected.clone(), external_uid.clone())],
            &HashMap::from([(expected.identity_key(), sibling_uid.clone())]),
        )
        .expect("test_setup: mixed binding succeeds");

        let mut refs = Vec::new();
        collect_card_refs(&spec, &mut refs);
        assert_eq!(refs[0].uid, Some(external_uid));
        assert_eq!(refs[1].uid, Some(sibling_uid));
    }

    #[test]
    fn sibling_identity_includes_version() {
        let first = submission("child");
        let mut second = submission("child");
        second.metadata.version = Some(VersionSpec::Pin(
            VersionBlock::parse("2.0.0").expect("test_setup: version is valid"),
        ));

        let identities = sibling_identities(&[first, second]).expect("identities resolve");
        assert_eq!(identities.len(), 2);
    }

    /// Scoped submissions remain registrable but cannot be sibling targets
    /// until the server resolves their exact version.
    #[test]
    fn sibling_identities_skip_scoped_submissions() {
        let mut scoped = submission("scoped");
        scoped.metadata.version =
            Some(VersionSpec::parse("1").expect("test_setup: scope is valid"));

        let identities = sibling_identities(&[scoped]).expect("scoped submission is accepted");

        assert!(identities.is_empty());
    }

    #[test]
    fn sibling_binding_does_not_match_a_different_version() {
        let expected = prompt_ref("child");
        let mut other_version = expected.clone();
        other_version.version = VersionBlock::parse("2.0.0").expect("version is valid");
        let uid = CardUid::from_uuid(Uuid::now_v7()).expect("test_setup: UUIDv7 is valid");
        let mut spec = sibling_service_spec(other_version);

        let error = bind_card_references(&mut spec, &vec![(expected, uid)], &HashMap::new())
            .expect_err("different sibling version must be rejected");
        assert_eq!(error.code(), "WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY");
    }
}
