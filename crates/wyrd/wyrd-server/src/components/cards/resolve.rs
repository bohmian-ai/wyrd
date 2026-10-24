//! Resolve non-sibling `CardRef` values before declaration writes begin.

use std::collections::{BTreeSet, HashMap};

use serde_json::json;
use wyrd_spec::card::data::{ArrowFormat, DataInterface};
use wyrd_spec::card::drift::{DriftSignal, DriftSpec};
use wyrd_spec::card::operator::OperatorSpec;
use wyrd_spec::card::trigger::{TriggerActivation, TriggerSpec};
use wyrd_spec::card::verifier::{InputSlot, SourceField, VerificationBinding, VerifierSpec};
use wyrd_spec::envelope::Spec;
use wyrd_spec::error::WyrdError;
use wyrd_spec::graph::BindingSite;
use wyrd_spec::ids::{CardName, CardUid};
use wyrd_spec::operator_connection::OperatorConnectionStatus;
use wyrd_spec::reference::{CardRef, CardRefIdentity, InlineableRef, Ref};
use wyrd_spec::refs::{ReferenceSlotVisitor, SlotValue};
use wyrd_spec::registry::CardSubmission;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::cards::{get_card_by_uid, select_card_uids_by_ref_batch};
use wyrd_sql::queries::operator_connections::find_connection;

use crate::state::registry_db_error;
use wyrd_sql::queries::verification::BindingSchedule;

/// Identity key used to look up a resolved external reference.
pub type ResolvedRefs = Vec<(CardRef, CardUid)>;

/// Reject a schedule binding whose Verifier judges no recorded samples.
///
/// A schedule occurrence supplies only recorded measurements for a window, so
/// it can activate only a Verifier with a `samples` slot; the Verifier's own
/// samples declarations supply every such slot.
///
/// # Errors
/// Returns `WYRD_SPEC_400_TRIGGER_ACTIVATION_MISMATCH` when the Verifier has
/// no samples slot.
fn check_activation(verifier: &VerifierSpec, field: &str) -> Result<(), WyrdError> {
    let samples = verifier
        .inputs
        .values()
        .any(|slot| matches!(slot, InputSlot::Samples { .. }));
    if samples {
        return Ok(());
    }
    Err(WyrdError::SpecTriggerActivationMismatch {
        message: format!("{field}.runs_on cannot activate a Verifier without a samples slot"),
        details: serde_json::json!({ "field": format!("{field}.runs_on") }),
    })
}

/// Validate an Operator's shape and the exact connection authority it names.
///
/// `verifier` is the bound Verifier when the Operator is attached to a
/// binding, so a template field that Verifier cannot fill is refused; a
/// standalone Operator Card passes `None`.
///
/// Reads only the tenant's redacted connection row (never the secret) under
/// the caller's RLS. Missing, disabled, wrong-provider, and mismatched
/// authority all answer the same refusal, so a tenant cannot probe which one
/// applies, and another tenant's connection is indistinguishable from none.
///
/// # Errors
/// Returns `WYRD_SPEC_400_INVALID_OPERATOR` for a shape or template violation
/// (including a template field the bound Verifier cannot fill),
/// `WYRD_SPEC_400_OPERATOR_CONNECTION_UNAVAILABLE` when no active matching
/// connection exists, and a registry error when the read fails.
async fn check_operator(
    conn: &mut TenantConn<'_>,
    operator: &OperatorSpec,
    field: &str,
    verifier: Option<&VerifierSpec>,
) -> Result<(), WyrdError> {
    operator.validate(field, verifier)?;
    let Some((provider, name)) = operator.connection() else {
        return Ok(());
    };
    let usable = find_connection(conn, provider, name)
        .await
        .map_err(registry_db_error)?
        .is_some_and(|stored| {
            stored.view.status == OperatorConnectionStatus::Active
                && operator.matches_authority(&stored.view.config)
        });
    if usable {
        return Ok(());
    }
    Err(WyrdError::SpecOperatorConnectionUnavailable {
        message: format!(
            "{field}: no active {provider} connection named {name} with matching authority"
        ),
        details: serde_json::json!({ "field": field, "provider": provider, "connection": name }),
    })
}

/// Find the one binding of `site` whose Verifier reference is named `name`.
///
/// A group source resolves only within its own binding site, by the bound
/// Verifier's `metadata.name` — the name its reference carries — so
/// registration and binding projection pin the same source.
///
/// # Errors
/// Returns the number of matching bindings when it is not exactly one.
pub(super) fn source_binding<'a>(
    site: &'a [VerificationBinding],
    name: &CardName,
) -> Result<&'a VerificationBinding, usize> {
    let matches: Vec<_> = site
        .iter()
        .filter(|binding| {
            binding
                .verifier
                .as_card_ref()
                .is_some_and(|verifier| &verifier.name == name)
        })
        .collect();
    match matches.as_slice() {
        [one] => Ok(one),
        _ => Err(matches.len()),
    }
}

/// Each top-level binding site's subject Card identity, in
/// [`Spec::binding_sites`] order.
///
/// The owner-level site is `None`; a component site is its component's Card
/// reference identity, or `None` for an inline component.
fn site_subjects(spec: &Spec) -> Vec<Option<CardRefIdentity>> {
    let mut subjects = vec![None];
    if let Spec::Service(service) = spec {
        subjects.extend(
            service
                .components
                .iter()
                .map(|component| component.card_ref.as_card_ref().map(CardRef::identity_key)),
        );
    }
    subjects
}

/// The owner's top-level binding sites seen from the one being validated.
#[derive(Clone, Copy)]
struct GroupSite<'s, 'a> {
    /// Every top-level binding site of the owner spec.
    sites: &'s [BindingSite<'a>],
    /// Each site's subject identity, aligned with `sites`.
    subjects: &'s [Option<CardRefIdentity>],
    /// Index of the site holding the binding being validated.
    site: usize,
}

impl GroupSite<'_, '_> {
    /// Whether another site with the same subject Card also binds `source`'s
    /// exact Verifier, making their recorded rows indistinguishable.
    fn ambiguous(&self, source: &VerificationBinding) -> bool {
        let Some(subject) = &self.subjects[self.site] else {
            return false;
        };
        let identity = source.verifier.as_card_ref().map(CardRef::identity_key);
        self.sites.iter().enumerate().any(|(index, site)| {
            index != self.site
                && self.subjects[index].as_ref() == Some(subject)
                && site.bindings.iter().any(|other| {
                    other.verifier.as_card_ref().map(CardRef::identity_key) == identity
                })
        })
    }
}

/// Registration preflight: effective spec bodies for referenced targets,
/// keyed by provenance and identity.
///
/// [`EffectiveSpecs::resolve`] is the preflight entry point. It owns the
/// request's resolved external UIDs alongside the decoded bodies, so binding
/// and baseline validation are methods on this owner rather than a
/// call graph that re-threads the resolution table per lookup. It borrows the
/// caller's tenant connection for each read; acquiring and committing that
/// connection, audit, and every durable write stay with the caller. A
/// `Sibling` reference reads only the request's own submissions and an
/// external `Ref` only the registry body at its resolved UID, loaded once and
/// cached, so a submitted sibling never stands in for an external dependency
/// with the same identity.
pub(super) struct EffectiveSpecs {
    /// Decoded submitted spec per exact Card identity.
    siblings: HashMap<CardRefIdentity, Spec>,
    /// Registry spec per exact identity, loaded at its resolved external UID.
    externals: HashMap<CardRefIdentity, Spec>,
    /// External references this request already resolved to a `CardUid`.
    resolved: ResolvedRefs,
}

impl EffectiveSpecs {
    /// Resolve every external (non-sibling) `CardRef` to its `CardUid` under RLS.
    ///
    /// Walks each submitted spec with the canonical [`ReferenceSlotVisitor`],
    /// rejects loader-only paths and siblings the request did not submit, batches
    /// the remaining identities into one RLS-scoped registry read, and then checks
    /// every verification binding and Drift baseline against the
    /// effective specs those refs name. All
    /// of it runs before the caller opens its write transaction, so any refusal
    /// here persists nothing. Cancellation may stop after completed registry reads
    /// but always before Card persistence.
    ///
    /// # Errors
    /// Returns `WYRD_REGISTRY_*_UNRESOLVED_PATH_REF` or
    /// `WYRD_REGISTRY_*_UNRESOLVED_DEPENDENCY` when a reference is a loader-only
    /// path, names an unsubmitted sibling, or has no Card in this tenant; the
    /// binding refusals listed on [`validate_bindings`](Self::validate_bindings);
    /// and the underlying registry error when the batch read or a by-UID read
    /// fails.
    pub(super) async fn resolve(
        conn: &mut TenantConn<'_>,
        submissions: &[CardSubmission],
    ) -> Result<ResolvedRefs, WyrdError> {
        let siblings = sibling_identities(submissions);
        let mut refs = Vec::new();

        for submission in submissions {
            let spec = Spec::from_kind_and_value(&submission.kind, submission.spec.clone())
                .map_err(|error| WyrdError::registry_invalid_card_spec(error.to_string()))?;
            validate_and_collect_refs(&spec, &siblings, &mut refs)?;
            if let Spec::Operator(operator) = &spec {
                check_operator(conn, operator, "spec", None).await?;
            }
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

        let mut effective = Self::new(submissions, resolved_refs)?;
        effective.validate_bindings(conn, submissions).await?;
        effective.validate_baselines(conn, submissions).await?;
        Ok(effective.resolved)
    }

    /// Decode every pinned submission into the identity cache.
    ///
    /// Submissions without a resolved version cannot be a sibling
    /// target, so they are skipped rather than cached under a partial identity.
    ///
    /// # Errors
    /// Returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC` for an undecodable spec.
    fn new(submissions: &[CardSubmission], resolved: ResolvedRefs) -> Result<Self, WyrdError> {
        let mut siblings = HashMap::new();
        for submission in submissions {
            let Some(version) = submission.metadata.resolved_pin() else {
                continue;
            };
            let identity = CardRef {
                kind: submission.kind.clone(),
                name: submission.metadata.name.clone(),
                version: version.clone(),
                uid: None,
            }
            .identity_key();
            let spec = Spec::from_kind_and_value(&submission.kind, submission.spec.clone())
                .map_err(|error| WyrdError::registry_invalid_card_spec(error.to_string()))?;
            siblings.insert(identity, spec);
        }
        Ok(Self {
            siblings,
            externals: HashMap::new(),
            resolved,
        })
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
    /// Trigger cannot activate the effective Verifier,
    /// `WYRD_REGISTRY_400_INVALID_CARD_SPEC` when a `schedule` Trigger cannot
    /// be armed,
    /// the Operator shape and connection refusals of `check_operator`, and
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
            let sites: Vec<_> = spec
                .binding_sites()
                .into_iter()
                .filter(|site| !site.nested)
                .collect();
            let subjects = site_subjects(&spec);
            for (site_index, site) in sites.iter().enumerate() {
                for (index, binding) in site.bindings.iter().enumerate() {
                    let field = format!("{}[{index}]", site.field);
                    let group = GroupSite {
                        sites: &sites,
                        subjects: &subjects,
                        site: site_index,
                    };
                    self.validate_binding(conn, binding, &field, group).await?;
                }
            }
        }
        Ok(())
    }

    /// Check one binding's effective Trigger pairing, schedule, group sources,
    /// and reaction Operators.
    ///
    /// A binding without `runs_on` is explicit-only: it has no Trigger to pair,
    /// so only its Verifier and Operators are checked.
    ///
    /// A `schedule` Trigger must parse as a five-field cron in a known IANA
    /// zone with a future occurrence, because the projected binding arms its
    /// cursor from that schedule on the owner's first machine exchange. A
    /// Verifier with source inputs is bound only by a `group_end` Trigger,
    /// which reacts through `on_verdict_change` alone and resolves its sources
    /// through [`Self::validate_sources`]; `on_verdict_change` belongs to a
    /// `group_end` binding only.
    ///
    /// # Errors
    /// Returns the activation-mismatch and Operator refusals
    /// documented on [`EffectiveSpecs::validate_bindings`],
    /// `WYRD_REGISTRY_400_INVALID_CARD_SPEC` for an unarmable schedule, the
    /// source refusals of [`Self::validate_sources`], plus any registry read
    /// failure raised while loading a referenced body.
    async fn validate_binding(
        &mut self,
        conn: &mut TenantConn<'_>,
        binding: &VerificationBinding,
        field: &str,
        group: GroupSite<'_, '_>,
    ) -> Result<(), WyrdError> {
        let verifier = self.load(conn, Some(&binding.verifier)).await?;
        let trigger = match &binding.runs_on {
            None => None,
            Some(InlineableRef::Inline(trigger)) => Some(Spec::Trigger((**trigger).clone())),
            Some(reference) => self.load(conn, reference.to_durable().as_ref()).await?,
        };
        let group_end = matches!(
            &trigger,
            Some(Spec::Trigger(TriggerSpec {
                activation: TriggerActivation::GroupEnd { .. },
                ..
            }))
        );
        let mismatch = |field: String, reason: &str| WyrdError::SpecTriggerActivationMismatch {
            message: format!("{field} {reason}"),
            details: json!({ "field": field }),
        };
        if group_end && !binding.on_failure.is_empty() {
            return Err(mismatch(
                format!("{field}.on_failure"),
                "is not allowed on a group_end binding; react through on_verdict_change",
            ));
        }
        if !group_end && !binding.on_verdict_change.is_empty() {
            return Err(mismatch(
                format!("{field}.on_verdict_change"),
                "requires a group_end Trigger",
            ));
        }
        if let Some(Spec::Verifier(verifier)) = &verifier {
            let sources = verifier
                .inputs
                .values()
                .any(|slot| matches!(slot, InputSlot::Source(_)));
            if sources != group_end {
                return Err(mismatch(
                    format!("{field}.runs_on"),
                    "must be a group_end Trigger exactly when the Verifier declares source inputs",
                ));
            }
            if group_end {
                self.validate_sources(conn, binding, verifier, field, group)
                    .await?;
            }
        }
        if let (Some(Spec::Verifier(verifier)), Some(Spec::Trigger(trigger))) =
            (&verifier, &trigger)
            && matches!(trigger.activation, TriggerActivation::Schedule { .. })
        {
            check_activation(verifier, field)?;
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
        for (list, operators) in binding.operator_lists() {
            for (index, operator) in operators.iter().enumerate() {
                let operator = match operator {
                    InlineableRef::Inline(operator) => Some(Spec::Operator((**operator).clone())),
                    reference => self.load(conn, reference.to_durable().as_ref()).await?,
                };
                let field = format!("{field}.{list}[{index}]");
                if let Some(Spec::Operator(operator)) = operator {
                    let verifier = match &verifier {
                        Some(Spec::Verifier(verifier)) => Some(verifier),
                        _ => None,
                    };
                    check_operator(conn, &operator, &field, verifier).await?;
                }
            }
        }
        Ok(())
    }

    /// Resolve and check every source input of a `group_end` binding's Verifier.
    ///
    /// Each `verifiers.<name>...` source resolves through [`source_binding`]
    /// to exactly one binding of the same site, whose exact Verifier must
    /// declare the referenced slot or requirement. Following sources from
    /// there must never lead back to this binding's own Verifier. The matched
    /// binding's recorded tuple (subject Card, Verifier) must also be unique
    /// across the owner's sites, because recorded rows carry no occurrence
    /// and could not be told apart. Only the bindings a group lookup uses are
    /// checked.
    ///
    /// # Errors
    /// Returns `WYRD_VALIDATION` with rule `source` at the declaring input for
    /// zero or several matching bindings, an absent slot or requirement, a
    /// cycle, or an ambiguous recorded tuple, plus any registry read failure.
    async fn validate_sources(
        &mut self,
        conn: &mut TenantConn<'_>,
        binding: &VerificationBinding,
        verifier: &VerifierSpec,
        field: &str,
        group: GroupSite<'_, '_>,
    ) -> Result<(), WyrdError> {
        let site = group.sites[group.site].bindings;
        let own = binding.verifier.as_card_ref().map(|own| &own.name);
        for (slot, input) in &verifier.inputs {
            let InputSlot::Source(text) = input else {
                continue;
            };
            let refuse = |reason: String| WyrdError::Validation {
                message: format!("{field}: inputs.{slot} {reason}"),
                details: json!({ "rule": "source", "path": format!("inputs.{slot}"), "field": field }),
            };
            let reference = text.parse().map_err(|error| refuse(error.to_string()))?;
            let source = source_binding(site, &reference.verifier).map_err(|count| {
                refuse(format!(
                    "matches {count} bindings of Verifier {} on this site; exactly one is required",
                    reference.verifier
                ))
            })?;
            let Some(Spec::Verifier(target)) = self.load(conn, Some(&source.verifier)).await?
            else {
                return Err(refuse(format!(
                    "names Verifier {} whose spec is not available",
                    reference.verifier
                )));
            };
            let declared = match &reference.field {
                SourceField::Input(name) => target.inputs.contains_key(name),
                SourceField::Outcome(id) => target.all_requirements().any(|r| &r.id == id),
            };
            if !declared {
                return Err(refuse(format!(
                    "names a slot or requirement Verifier {} does not declare",
                    reference.verifier
                )));
            }
            if Some(&reference.verifier) == own || self.reaches(conn, site, &target, own).await? {
                return Err(refuse("forms a source cycle".to_owned()));
            }
            if group.ambiguous(source) {
                return Err(refuse(format!(
                    "names Verifier {} whose subject and version are bound on another site too",
                    reference.verifier
                )));
            }
        }
        Ok(())
    }

    /// Whether following `start`'s sources within `site` reaches `target`.
    ///
    /// Each Verifier is visited once, so the walk ends after at most one load
    /// per binding of the site. A source that does not resolve to exactly one
    /// binding is that binding's own refusal and ends its branch.
    ///
    /// # Errors
    /// Returns the registry error from loading a referenced Verifier.
    async fn reaches(
        &mut self,
        conn: &mut TenantConn<'_>,
        site: &[VerificationBinding],
        start: &VerifierSpec,
        target: Option<&CardName>,
    ) -> Result<bool, WyrdError> {
        let Some(target) = target else {
            return Ok(false);
        };
        let mut seen = BTreeSet::new();
        let mut pending = vec![start.clone()];
        while let Some(verifier) = pending.pop() {
            for input in verifier.inputs.values() {
                let InputSlot::Source(text) = input else {
                    continue;
                };
                let Ok(reference) = text.parse() else {
                    continue;
                };
                if &reference.verifier == target {
                    return Ok(true);
                }
                let Ok(source) = source_binding(site, &reference.verifier) else {
                    continue;
                };
                if seen.insert(reference.verifier)
                    && let Some(Spec::Verifier(next)) =
                        self.load(conn, Some(&source.verifier)).await?
                {
                    pending.push(next);
                }
            }
        }
        Ok(false)
    }

    /// Check every submitted PSI or SPC drift check against its effective
    /// baseline Data.
    ///
    /// The fitter reads the Data Card's declared Parquet artifact, so the
    /// baseline must use an interface whose artifact is Parquet — Pandas,
    /// Polars, Parquet, or Arrow saved as Parquet — and must declare every
    /// monitored feature as a schema column. Checking here, before the write
    /// transaction, refuses a baseline that could never fit without persisting
    /// a Verifier whose status would only ever fail.
    ///
    /// # Errors
    /// Returns `WYRD_DRIFT_400_VALIDATION` when the baseline is not a Data
    /// Card, is not stored as Parquet, or lacks a monitored feature column,
    /// `WYRD_REGISTRY_400_INVALID_CARD_SPEC` for an undecodable submission,
    /// and the registry error from loading an external baseline.
    async fn validate_baselines(
        &mut self,
        conn: &mut TenantConn<'_>,
        submissions: &[CardSubmission],
    ) -> Result<(), WyrdError> {
        for submission in submissions {
            let Spec::Verifier(verifier) =
                Spec::from_kind_and_value(&submission.kind, submission.spec.clone())
                    .map_err(|error| WyrdError::registry_invalid_card_spec(error.to_string()))?
            else {
                continue;
            };
            for (_, check) in verifier.drift_checks() {
                self.validate_baseline(conn, &check.spec).await?;
            }
        }
        Ok(())
    }

    /// Check one PSI or SPC drift check against its effective baseline Data.
    ///
    /// # Errors
    /// Returns the refusals documented on
    /// [`validate_baselines`](Self::validate_baselines).
    async fn validate_baseline(
        &mut self,
        conn: &mut TenantConn<'_>,
        drift: &DriftSpec,
    ) -> Result<(), WyrdError> {
        let DriftSignal::Distribution {
            baseline_ref,
            features,
        } = &drift.signal
        else {
            return Ok(());
        };
        let Some(Spec::Data(data)) = self.load(conn, Some(baseline_ref)).await? else {
            return Err(baseline_error(
                "signal.baseline_ref must name a registered Data Card",
                json!({ "field": "signal.baseline_ref" }),
            ));
        };
        let parquet = match &data.interface {
            DataInterface::Pandas(_) | DataInterface::Polars(_) | DataInterface::Parquet(_) => true,
            DataInterface::Arrow(meta) => meta.format == ArrowFormat::Parquet,
            _ => false,
        };
        if !parquet {
            return Err(baseline_error(
                format!(
                    "baseline Data Card interface {} is not stored as Parquet",
                    data.interface.kind()
                ),
                json!({ "field": "signal.baseline_ref", "interface": data.interface.kind(), "expected": "Parquet artifact" }),
            ));
        }
        if let Some(missing) = features.iter().find(|feature| {
            !data
                .schema
                .columns
                .iter()
                .any(|column| column.name.as_str() == feature.as_str())
        }) {
            return Err(baseline_error(
                format!("baseline Data Card has no column for feature {missing}"),
                json!({ "field": "signal.features", "feature": missing.as_str() }),
            ));
        }
        Ok(())
    }

    /// Return the effective spec a reference names from its own provenance.
    ///
    /// A `Sibling` reads only the submitted body; an external `Ref` reads only
    /// the registry body at its resolved UID, loaded once and cached. Returns
    /// `None` for an absent ref, a path, an unsubmitted sibling, or an
    /// unresolved external ref; those were already rejected upstream.
    ///
    /// Cancellation may drop the in-flight by-UID read; nothing is written.
    ///
    /// # Errors
    /// Returns the registry error from loading an external Card by UID.
    async fn load(
        &mut self,
        conn: &mut TenantConn<'_>,
        reference: Option<&Ref>,
    ) -> Result<Option<Spec>, WyrdError> {
        let card_ref = match reference {
            None | Some(Ref::Path(_)) => return Ok(None),
            Some(Ref::Sibling { sibling }) => {
                return Ok(self.siblings.get(&sibling_key(sibling)).cloned());
            }
            Some(Ref::Ref(card_ref)) => card_ref,
        };
        let identity = card_ref.identity_key();
        if let Some(spec) = self.externals.get(&identity) {
            return Ok(Some(spec.clone()));
        }
        let Some(uid) = external_uid(card_ref, &self.resolved) else {
            return Ok(None);
        };
        let row = get_card_by_uid(conn, &uid).await?;
        self.externals.insert(identity, row.spec.clone());
        Ok(Some(row.spec))
    }
}

/// Build the Drift validation refusal for an unusable baseline.
fn baseline_error(message: impl Into<String>, details: serde_json::Value) -> WyrdError {
    WyrdError::DriftValidation {
        message: message.into(),
        details,
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
/// sibling reference carries an exact version before declaration resolves.
fn sibling_identities(submissions: &[CardSubmission]) -> BTreeSet<CardRefIdentity> {
    submissions
        .iter()
        .filter_map(|submission| {
            submission.metadata.resolved_pin().cloned().map(|version| {
                CardRef {
                    kind: submission.kind.clone(),
                    name: submission.metadata.name.clone(),
                    version,
                    uid: None,
                }
                .identity_key()
            })
        })
        .collect()
}

/// Return the exact identity used for sibling matching and UID binding.
fn sibling_key(card_ref: &CardRef) -> CardRefIdentity {
    card_ref.identity_key()
}

/// Format a reference for deterministic comparison and actionable errors.
fn display_ref(card_ref: &CardRef) -> String {
    format!(
        "{}/{}@{}",
        card_ref.kind.wire_name(),
        card_ref.name,
        card_ref.version
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
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
            uid: None,
        }
    }

    /// A Prompt submission `name` pinned to `1.0.0` with a
    /// minimal portable spec.
    ///
    /// # Panics
    /// Panics when a static identity field is invalid.
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
                uid: None,
                labels: BTreeMap::default(),
                annotations: BTreeMap::default(),
                spec_hash: None,
                artifact_hash: None,
                origin: None,
            },
            spec: serde_json::json!({
                "provider": "openai",
                "model": "gpt-4o",
                "messages": [{"role": "user", "content": "hello"}]
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

    /// An Agent's sibling Prompt reference inside the submission set is
    /// accepted and collects no external reference to resolve.
    ///
    /// # Panics
    /// Panics when a fixture is invalid, validation refuses the sibling, or an
    /// external reference is collected.
    #[test]
    fn sibling_prompt_reference_is_not_external() {
        let prompt = prompt_ref("child");
        let spec = Spec::from_kind_and_value(
            &CardKind::Agent,
            serde_json::json!({
                "prompt": {"sibling": prompt.clone()},
                "execution_route": {"kind": "native"}
            }),
        )
        .expect("test_setup: sibling agent spec is valid");
        let siblings = BTreeSet::from([prompt.identity_key()]);
        let mut refs = Vec::new();

        validate_and_collect_refs(&spec, &siblings, &mut refs)
            .expect("test_setup: sibling prompt is accepted");

        assert!(refs.is_empty());
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

        let identities = sibling_identities(&[first, second]);
        assert_eq!(identities.len(), 2);
    }

    /// Scoped submissions remain registrable but cannot be sibling targets
    /// until the server resolves their exact version.
    #[test]
    fn sibling_identities_skip_scoped_submissions() {
        let mut scoped = submission("scoped");
        scoped.metadata.version =
            Some(VersionSpec::parse("1").expect("test_setup: scope is valid"));

        let identities = sibling_identities(&[scoped]);

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
