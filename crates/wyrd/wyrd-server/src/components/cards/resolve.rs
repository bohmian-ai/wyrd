//! Resolve non-sibling `CardRef` values before registration writes begin.

use std::collections::{BTreeSet, HashMap};

use serde_json::json;
use skald_workflow::{Workflow, card_body_dependencies};
use wyrd_semver::VersionSpec;
use wyrd_spec::api_version::ApiVersion;
use wyrd_spec::card::data::{ArrowFormat, DataInterface};
use wyrd_spec::card::drift::DriftSignal;
use wyrd_spec::card::operator::{OperatorAction, OperatorSpec};
use wyrd_spec::card::trigger::{TriggerActivation, TriggerSpec};
use wyrd_spec::card::verifier::{VerificationBinding, VerifierImplementation, VerifierSpec};
use wyrd_spec::card::workflow::{WorkflowCard, jcs_len};
use wyrd_spec::envelope::{Card, CardKind, Metadata, Relationships, Spec};
use wyrd_spec::error::WyrdError;
use wyrd_spec::graph::graph_ready_submissions;
use wyrd_spec::ids::CardUid;
use wyrd_spec::operator_connection::OperatorConnectionStatus;
use wyrd_spec::reference::{CardRef, CardRefIdentity, InlineableRef, Ref};
use wyrd_spec::refs::{ReferenceSlotVisitor, SlotValue};
use wyrd_spec::registry::CardSubmission;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::cards::{get_card_by_ref, get_card_by_uid, select_card_uids_by_ref_batch};
use wyrd_sql::queries::operator_connections::find_connection;
use wyrd_sql::row_types::cards::{CardStatus, ParsedCardRow};

use crate::state::registry_db_error;
use wyrd_sql::queries::verification::BindingSchedule;

/// Identity key used to look up a resolved external reference.
pub type ResolvedRefs = Vec<(CardRef, CardUid)>;

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

/// Validate an Operator's shape and the exact connection authority it names.
///
/// `implementation` is the bound Verifier's implementation when the Operator
/// is attached to a binding, so a kind-specific template field of another
/// implementation is refused; a standalone Operator Card passes `None`.
///
/// Reads only the tenant's redacted connection row (never the secret) under
/// the caller's RLS. Missing, disabled, wrong-provider, and mismatched
/// authority all answer the same refusal, so a tenant cannot probe which one
/// applies, and another tenant's connection is indistinguishable from none.
///
/// # Errors
/// Returns `WYRD_SPEC_400_INVALID_OPERATOR` for a shape or template violation
/// (including a template field of another Verifier implementation),
/// `WYRD_SPEC_400_OPERATOR_CONNECTION_UNAVAILABLE` when no active matching
/// connection exists, and a registry error when the read fails.
async fn check_operator(
    conn: &mut TenantConn<'_>,
    operator: &OperatorSpec,
    field: &str,
    implementation: Option<&VerifierImplementation>,
) -> Result<(), WyrdError> {
    operator.validate(field, implementation)?;
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

/// Registration preflight: effective spec bodies for referenced targets,
/// keyed by provenance and identity.
///
/// [`EffectiveSpecs::resolve`] is the preflight entry point. It owns the
/// request's resolved external UIDs alongside the decoded bodies, so binding,
/// baseline, and Workflow validation are methods on this owner rather than a
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
    /// every verification binding, Drift baseline, and Workflow graph against the
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
    /// the Workflow graph refusals listed on
    /// [`validate_workflows`](Self::validate_workflows);
    /// and the underlying registry error when the batch read or a by-UID read
    /// fails.
    pub(super) async fn resolve(
        conn: &mut TenantConn<'_>,
        submissions: &[CardSubmission],
    ) -> Result<ResolvedRefs, WyrdError> {
        let siblings = sibling_identities(submissions)?;
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
        effective.validate_workflows(conn, submissions).await?;
        Ok(effective.resolved)
    }

    /// Decode every pinned submission into the identity cache.
    ///
    /// Submissions without a resolved space and version cannot be a sibling
    /// target, so they are skipped rather than cached under a partial identity.
    ///
    /// # Errors
    /// Returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC` for an undecodable spec.
    fn new(submissions: &[CardSubmission], resolved: ResolvedRefs) -> Result<Self, WyrdError> {
        let mut siblings = HashMap::new();
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
        let verifier = self.load(conn, Some(&binding.verifier)).await?;
        let trigger = match &binding.runs_on {
            InlineableRef::Inline(trigger) => Some(Spec::Trigger((**trigger).clone())),
            reference => self.load(conn, reference.to_durable().as_ref()).await?,
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
                reference => self.load(conn, reference.to_durable().as_ref()).await?,
            };
            let field = format!("{field}.on_failure[{index}]");
            match operator {
                Some(Spec::Operator(OperatorSpec {
                    action: OperatorAction::Workflow { .. },
                    ..
                })) => {
                    return Err(WyrdError::SpecUnsupportedOperatorAction {
                        message: format!(
                            "{field} uses the workflow action, which is not invocable"
                        ),
                        details: serde_json::json!({ "field": field, "action": "workflow" }),
                    });
                }
                Some(Spec::Operator(operator)) => {
                    let implementation = match &verifier {
                        Some(Spec::Verifier(verifier)) => Some(&verifier.implementation),
                        _ => None,
                    };
                    check_operator(conn, &operator, &field, implementation).await?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Check every submitted PSI or SPC Verifier against its effective baseline Data.
    ///
    /// The fitter reads the Data Card's registered Parquet artifact, so the
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
            let Spec::Verifier(VerifierSpec {
                implementation: VerifierImplementation::Drift(drift),
                ..
            }) = Spec::from_kind_and_value(&submission.kind, submission.spec.clone())
                .map_err(|error| WyrdError::registry_invalid_card_spec(error.to_string()))?
            else {
                continue;
            };
            let DriftSignal::Distribution {
                baseline_ref,
                features,
            } = &drift.signal
            else {
                continue;
            };
            let Some(Spec::Data(data)) = self.load(conn, Some(baseline_ref)).await? else {
                return Err(baseline_error(
                    "signal.baseline_ref must name a registered Data Card",
                    json!({ "field": "signal.baseline_ref" }),
                ));
            };
            let parquet = match &data.interface {
                DataInterface::Pandas(_) | DataInterface::Polars(_) | DataInterface::Parquet(_) => {
                    true
                }
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
        }
        Ok(())
    }

    /// Validate every submitted Workflow against its effective resolved graph.
    ///
    /// Each Workflow submission's Agent and Prompt references come from the
    /// canonical reference-slot inventory with their authored provenance;
    /// [`load`](Self::load) supplies a sibling from the submissions and an
    /// external ref from the registry body at its resolved UID — the same UID
    /// registration binds. Each loaded Agent contributes its own Prompt
    /// reference, and a Prompt named only inside an already-registered Agent
    /// is resolved through the same Active, tenant-scoped batch lookup before
    /// loading. Skald then runs the pure contract and the declarative
    /// resolved checks (bindings, outputs, Prompt coverage, route dialect)
    /// over exactly those bodies without binding tools, so sibling and
    /// external dependencies fail identically.
    ///
    /// This runs in the preflight transaction before the write transaction:
    /// cancellation may stop after completed registry reads but before any
    /// Card persistence, and a refusal persists nothing.
    ///
    /// # Errors
    /// Returns the `WYRD_WORKFLOW_*` contract and graph refusals of
    /// [`Workflow::validate_card_bodies`],
    /// `WYRD_REGISTRY_*_UNRESOLVED_DEPENDENCY` when a needed body has no
    /// Active Card in this tenant, `WYRD_REGISTRY_400_INVALID_CARD_SPEC` for an
    /// undecodable submission, and registry read failures unchanged.
    async fn validate_workflows(
        &mut self,
        conn: &mut TenantConn<'_>,
        submissions: &[CardSubmission],
    ) -> Result<(), WyrdError> {
        // A fresh root may omit or scope its version; the server allocates the
        // pin only at write time. These transient holders carry the graph-only
        // placeholder pin, which names no dependency and is discarded after
        // validation; the authored submissions stay untouched.
        let holders = graph_ready_submissions(submissions)
            .map_err(|error| WyrdError::registry_invalid_card_spec(error.to_string()))?;
        for holder in holders.iter().filter(|s| s.kind == CardKind::Workflow) {
            let workflow = WorkflowCard::from_envelope(submission_card(holder)?)?;
            let mut pending = card_body_dependencies(&Spec::Workflow(workflow.spec.clone()));
            while let Some(dependency) = pending.pop() {
                if let Ref::Ref(card_ref) = &dependency
                    && external_uid(card_ref, &self.resolved).is_none()
                {
                    let found =
                        select_card_uids_by_ref_batch(conn, std::slice::from_ref(card_ref)).await?;
                    self.resolved.extend(found);
                }
                let spec = self.load(conn, Some(&dependency)).await?.ok_or_else(|| {
                    let identity = dependency
                        .as_card_ref()
                        .map_or_else(|| "<unresolved path>".to_owned(), display_ref);
                    WyrdError::RegistryUnresolvedDependency {
                        message: format!("card dependency {identity} was not found"),
                        details: serde_json::json!({ "card_ref": identity }),
                    }
                })?;
                pending.extend(card_body_dependencies(&spec));
            }
            Workflow::validate_card_bodies(workflow, &|dependency| self.body(dependency))?;
        }
        Ok(())
    }

    /// Return the already-loaded effective spec a reference names from its
    /// own provenance: a `Sibling` from the submissions and an external `Ref`
    /// from the registry body cached at its resolved UID.
    ///
    /// Returns `None` for a path or a body that was never loaded.
    fn body(&self, reference: &Ref) -> Option<Spec> {
        match reference {
            Ref::Sibling { sibling } => self.siblings.get(&sibling_key(sibling)).cloned(),
            Ref::Ref(card_ref) => self.externals.get(&card_ref.identity_key()).cloned(),
            Ref::Path(_) => None,
        }
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

/// Server Workflow admission bounds on a graph's declared shape and the JCS
/// bytes of the bodies it executes.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GraphBounds {
    /// Declared steps one Workflow may have.
    pub(crate) max_steps: usize,
    /// Declared dependency edges, duplicates included.
    pub(crate) max_edges: usize,
    /// JCS bytes of the Workflow spec and every unique pinned Agent and
    /// Prompt spec.
    pub(crate) max_bytes: usize,
}

/// The exact active Workflow graph one accepted server run executes.
///
/// [`PinnedWorkflowGraph::pin`] reads the root Workflow and every Agent and
/// Prompt Card it names at their exact registered identities, refusing any
/// Card that is not active, so a later re-registration, deprecation, or
/// deletion cannot change what an accepted run executes. Bodies are keyed by
/// exact identity and served only to an external reference with that
/// identity.
pub(crate) struct PinnedWorkflowGraph {
    /// The active root Workflow Card.
    workflow: WorkflowCard,
    /// Every pinned Agent and Prompt spec the Workflow names, by identity.
    bodies: HashMap<CardRefIdentity, Spec>,
}

impl PinnedWorkflowGraph {
    /// Read and pin the active Workflow at `workflow_ref` and its Agent and
    /// Prompt closure within `bounds`.
    ///
    /// The step and edge counts are checked on the root spec before any
    /// dependency is read, and every body is charged against the byte budget
    /// before it is retained, so an oversized graph is refused without being
    /// held. Each dependency is read once, at its bound UID when the stored
    /// reference carries one. No Card is written; cancellation may stop
    /// after completed reads.
    ///
    /// # Errors
    /// Returns `WYRD_WORKFLOW_422_RUN_REQUEST` when `workflow_ref` names no
    /// space; `WYRD_REGISTRY_404_CARD_NOT_FOUND` when no Workflow exists at
    /// that identity or UID; `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY` when
    /// the root or a dependency is not active, names a path or sibling, or
    /// its bound UID names another identity; `WYRD_WORKFLOW_413_GRAPH_TOO_LARGE`
    /// when a bound is exceeded; the Workflow envelope errors of
    /// [`WorkflowCard::from_envelope`]; and registry read failures unchanged.
    pub(crate) async fn pin(
        conn: &mut TenantConn<'_>,
        workflow_ref: &CardRef,
        bounds: GraphBounds,
    ) -> Result<Self, WyrdError> {
        if workflow_ref.kind != CardKind::Workflow || workflow_ref.space.is_none() {
            return Err(WyrdError::WorkflowRunRequest {
                message: "workflow must be a Workflow CardRef with an explicit space".to_owned(),
                details: json!({ "field": "workflow" }),
            });
        }
        let root = active_row(conn, workflow_ref).await?;
        let workflow = WorkflowCard::from_envelope(row_card(root))?;
        let steps = workflow.spec.steps.len();
        let edges = workflow.spec.steps.iter().fold(0_usize, |edges, step| {
            edges.saturating_add(step.depends_on.len())
        });
        if steps > bounds.max_steps {
            return Err(graph_too_large("max_steps_per_run", bounds.max_steps));
        }
        if edges > bounds.max_edges {
            return Err(graph_too_large(
                "max_dependency_edges_per_run",
                bounds.max_edges,
            ));
        }
        let mut charged = jcs_len(&workflow.spec);
        let mut bodies = HashMap::new();
        let mut pending = card_body_dependencies(&Spec::Workflow(workflow.spec.clone()));
        while let Some(dependency) = pending.pop() {
            let Ref::Ref(card_ref) = &dependency else {
                return Err(WyrdError::RegistryUnresolvedDependency {
                    message: "a registered Workflow names a dependency outside the registry"
                        .to_owned(),
                    details: json!({}),
                });
            };
            let identity = card_ref.identity_key();
            if bodies.contains_key(&identity) {
                continue;
            }
            let row = active_row(conn, card_ref).await?;
            charged = charged.saturating_add(jcs_len(&row.spec));
            if charged > bounds.max_bytes {
                return Err(graph_too_large(
                    "max_resolved_graph_bytes",
                    bounds.max_bytes,
                ));
            }
            pending.extend(card_body_dependencies(&row.spec));
            bodies.insert(identity, row.spec);
        }
        Ok(Self { workflow, bodies })
    }

    /// The pinned root Workflow Card.
    pub(crate) fn workflow(&self) -> &WorkflowCard {
        &self.workflow
    }

    /// Return the pinned spec an external reference names by its exact
    /// identity, or `None` for a sibling, a path, or an unpinned identity.
    pub(crate) fn body(&self, reference: &Ref) -> Option<Spec> {
        match reference {
            Ref::Ref(card_ref) => self.bodies.get(&card_ref.identity_key()).cloned(),
            Ref::Sibling { .. } | Ref::Path(_) => None,
        }
    }
}

/// Read the active Card `card_ref` names: at its bound UID when it carries
/// one, otherwise at its exact identity.
///
/// # Errors
/// Returns `WYRD_REGISTRY_404_CARD_NOT_FOUND` when no Card exists there,
/// `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY` when the bound UID names another
/// identity or the Card is not active, `WYRD_REGISTRY_400_INVALID_CARD_SPEC`
/// for a reference without a space, and registry read failures unchanged.
async fn active_row(
    conn: &mut TenantConn<'_>,
    card_ref: &CardRef,
) -> Result<ParsedCardRow, WyrdError> {
    let row = match &card_ref.uid {
        Some(uid) => get_card_by_uid(conn, uid).await?,
        None => {
            let space = card_ref.space.as_ref().ok_or_else(|| {
                WyrdError::registry_invalid_card_spec("CardRef.space is required for a card read")
            })?;
            get_card_by_ref(
                conn,
                card_ref.kind.clone(),
                space,
                &card_ref.name,
                &card_ref.version,
            )
            .await?
        }
    };
    let pinned = CardRef {
        kind: row.kind.clone(),
        name: row.name.clone(),
        version: row.version.clone(),
        space: Some(row.space.clone()),
        uid: None,
    };
    let unresolved = |reason: &str| WyrdError::RegistryUnresolvedDependency {
        message: format!("card dependency {} {reason}", display_ref(card_ref)),
        details: json!({ "card_ref": display_ref(card_ref) }),
    };
    if !pinned.same_identity(card_ref) {
        return Err(unresolved("is bound to another Card's UID"));
    }
    if row.status != CardStatus::Active {
        return Err(unresolved("is not active"));
    }
    Ok(row)
}

/// Build the Card envelope of a registry row for typed decoding.
fn row_card(row: ParsedCardRow) -> Card {
    Card {
        api_version: ApiVersion::v1(),
        kind: row.kind,
        metadata: Metadata {
            name: row.name,
            version: Some(VersionSpec::Pin(row.version)),
            bump: None,
            space: Some(row.space),
            uid: Some(row.card_uid),
            labels: row.labels,
            annotations: row.annotations,
            spec_hash: None,
            artifact_hash: None,
            origin: None,
        },
        spec: row.spec,
        relationships: Relationships::default(),
        status: None,
    }
}

/// The graph-size refusal naming the server bound a graph exceeded.
fn graph_too_large(bound: &str, max: usize) -> WyrdError {
    WyrdError::WorkflowGraphTooLarge {
        message: format!("the Workflow graph exceeds workflow.{bound}"),
        details: json!({ "bound": bound, "max": max }),
    }
}

/// Convert a registration submission into a Card envelope for typed decoding.
///
/// # Errors
/// Returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC` for an undecodable spec.
fn submission_card(submission: &CardSubmission) -> Result<Card, WyrdError> {
    let spec = Spec::from_kind_and_value(&submission.kind, submission.spec.clone())
        .map_err(|error| WyrdError::registry_invalid_card_spec(error.to_string()))?;
    Ok(Card {
        api_version: ApiVersion::v1(),
        kind: submission.kind.clone(),
        metadata: submission.metadata.clone(),
        spec,
        relationships: Relationships::default(),
        status: None,
    })
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
