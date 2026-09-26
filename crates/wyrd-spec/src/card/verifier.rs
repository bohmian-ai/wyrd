//! Verifier Card spec and the inline verification binding that attaches one.
//!
//! A Verifier is the single registrable verification Card kind. It declares
//! exactly one typed implementation — currently Drift or Eval — and carries no
//! secret material and no mutable runtime health. Subjects attach a Verifier
//! through a [`VerificationBinding`] in their own `verified_by` list; the
//! binding is part of the containing Card version and is never itself a Card.

#[cfg(feature = "server")]
use std::borrow::Cow;

use serde::{Deserialize, Serialize};
#[cfg(feature = "server")]
use utoipa::openapi::RefOr;
#[cfg(feature = "server")]
use utoipa::openapi::schema::{ObjectBuilder, OneOfBuilder, Schema, Type};
#[cfg(feature = "server")]
use utoipa::{PartialSchema, ToSchema};

use crate::card::drift::{DriftSpec, DriftValidationError};
use crate::card::eval::EvalSpec;
use crate::card::operator::OperatorSpec;
use crate::card::trigger::TriggerSpec;
use crate::ids::BindingId;
use crate::reference::{CardRef, InlineableRef, Ref};
use crate::verification::VerificationError;

/// Verifier Card spec body.
///
/// One Verifier declares one implementation. Independent judgments are
/// composed by attaching multiple Verifier Cards, never by nesting
/// implementations or introducing a Verifier DAG.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct VerifierSpec {
    /// Optional human-readable purpose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The single typed implementation this Verifier runs.
    pub implementation: VerifierImplementation,
}

/// The closed set of Verifier implementations.
///
/// Adjacently tagged so that `implementation.kind` selects the variant and
/// the variant's own typed body lives under `implementation.spec`. An unknown
/// `kind` fails deserialization before validation or persistence, so a future
/// implementation cannot be authored until its variant ships.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(
    tag = "kind",
    content = "spec",
    rename_all = "snake_case",
    deny_unknown_fields
)]
// justification: mirrors the `Spec` contract; boxing a variant would change the
// authored wire shape's ergonomics for every consumer that matches on it
#[allow(clippy::large_enum_variant)]
pub enum VerifierImplementation {
    /// Continuous distribution or metric drift over observed records.
    Drift(DriftSpec),
    /// Task-based evaluation over committed observations.
    Eval(EvalSpec),
}

/// Build the opaque OpenAPI object used for `EvalSpec`.
///
/// `EvalSpec` is owned by `vala::eval` and deliberately carries no
/// `utoipa::ToSchema` derive, so the Verifier implementation union describes
/// its authored field names without duplicating the nested task/DAG contract.
#[cfg(feature = "server")]
fn eval_spec_openapi_schema() -> RefOr<Schema> {
    let opaque_object = Schema::Object(ObjectBuilder::new().schema_type(Type::Object).build());
    RefOr::T(Schema::Object(
        ObjectBuilder::new()
            .description(Some("Evaluation Verifier implementation spec."))
            .property("dataset", opaque_object.clone())
            .property("tasks", opaque_object.clone())
            .property("workflow", opaque_object.clone())
            .property("sampling", opaque_object.clone())
            .property("pass_gate", opaque_object.clone())
            .property("context_capture", opaque_object)
            .required("tasks")
            .schema_type(Type::Object)
            .build(),
    ))
}

/// Build one adjacently tagged `{ kind, spec }` branch of the union.
#[cfg(feature = "server")]
fn implementation_branch(kind: &str, spec: RefOr<Schema>) -> RefOr<Schema> {
    RefOr::T(Schema::Object(
        ObjectBuilder::new()
            .schema_type(Type::Object)
            .property(
                "kind",
                Schema::Object(
                    ObjectBuilder::new()
                        .schema_type(Type::String)
                        .enum_values(Some([kind]))
                        .build(),
                ),
            )
            .required("kind")
            .property("spec", spec)
            .required("spec")
            .build(),
    ))
}

#[cfg(feature = "server")]
impl PartialSchema for VerifierImplementation {
    /// Render the adjacently tagged `{ kind, spec }` union inline.
    ///
    /// Written by hand rather than derived because `EvalSpec` carries no
    /// `ToSchema`, so the `eval` branch describes its authored field names
    /// through [`eval_spec_openapi_schema`] instead of a `$ref`.
    fn schema() -> RefOr<Schema> {
        OneOfBuilder::new()
            .item(implementation_branch(
                "drift",
                <DriftSpec as PartialSchema>::schema(),
            ))
            .item(implementation_branch("eval", eval_spec_openapi_schema()))
            .title(Some("VerifierImplementation"))
            .description(Some("One typed Verifier implementation."))
            .into()
    }
}

#[cfg(feature = "server")]
impl ToSchema for VerifierImplementation {
    /// Name the component this union is registered and referenced under.
    fn name() -> Cow<'static, str> {
        Cow::Borrowed("VerifierImplementation")
    }

    /// Register the Drift branch's own component and its dependency closure.
    ///
    /// The union inlines `DriftSpec`'s body under `drift`, and that body still
    /// carries `$ref`s of its own (`DriftMethod`, `DriftSignal`,
    /// `DriftCondition`, `DriftProfile`, and their closure). Pushing
    /// `DriftSpec` alone would therefore leave every one of those references
    /// undefined, so its own dependencies are forwarded here. The `eval`
    /// branch is an opaque inline object (see [`eval_spec_openapi_schema`])
    /// and contributes no references.
    fn schemas(schemas: &mut Vec<(String, RefOr<Schema>)>) {
        schemas.push((
            <DriftSpec as ToSchema>::name().into(),
            <DriftSpec as PartialSchema>::schema(),
        ));
        <DriftSpec as ToSchema>::schemas(schemas);
    }
}

impl VerifierImplementation {
    /// Wire name of this implementation's `kind` discriminant.
    #[must_use]
    pub const fn kind_name(&self) -> &'static str {
        match self {
            Self::Drift(_) => "drift",
            Self::Eval(_) => "eval",
        }
    }
}

impl VerifierSpec {
    /// Run every implementation-local invariant this Verifier owns.
    ///
    /// Delegates to the implementation payload's own validator so that the
    /// Drift and Eval contracts stay the single source of truth for their
    /// method/signal/profile and task rules.
    ///
    /// # Errors
    /// Returns the first [`DriftValidationError`] a Drift implementation
    /// reports. The Eval implementation has no spec-local validator and always
    /// succeeds here.
    pub fn validate(&self) -> Result<(), DriftValidationError> {
        match &self.implementation {
            VerifierImplementation::Drift(drift) => drift.validate(),
            VerifierImplementation::Eval(_) => Ok(()),
        }
    }
}

/// Reserved subject-occurrence key of a Service-level or standalone-Agent
/// verification binding.
///
/// A binding's natural key names its subject occurrence: a Service component
/// binding uses the component alias, and a binding on the owner itself uses
/// this value. Composition validation refuses it as a component alias, so the
/// owner and component domains of the key never collide and the key is never
/// null.
pub const OWNER_OCCURRENCE_KEY: &str = "$owner";

/// Server-derived verification state projected onto `card.status`.
///
/// A Service or standalone Agent owning `verified_by` bindings exposes the
/// stable identity of each projected binding here, so a caller can address a
/// binding without a separate listing resource. The field is read-only: it is
/// derived from the tenant's binding projection on every Card read and is
/// never persisted as Card status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct VerificationStatus {
    /// Stable identities of this owner Card version's projected bindings,
    /// ordered by identity so reordering authored bindings changes nothing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub binding_ids: Vec<BindingId>,
    /// Fitted-baseline readiness of a PSI or SPC Drift Verifier Card version.
    ///
    /// Absent on every other Card, including a Custom Drift Verifier, which
    /// scores its authored scalar baseline and is ready without fitting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline: Option<DriftBaselineStatus>,
}

impl VerificationStatus {
    /// Build the status a Card read serves, or `None` when it has nothing to report.
    #[must_use]
    pub fn derived(
        binding_ids: Vec<BindingId>,
        baseline: Option<DriftBaselineStatus>,
    ) -> Option<Self> {
        (!binding_ids.is_empty() || baseline.is_some()).then_some(Self {
            binding_ids,
            baseline,
        })
    }
}

/// Lifecycle of one PSI or SPC Verifier's server-fitted baseline.
///
/// Registration creates it `pending`; the fitter claims it `building` and
/// settles it `ready` with the persisted fitted profile or `failed` with a
/// structured error. A failed fit is retried through the same record while
/// attempts remain, so `failed` is visible without being necessarily final.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum DriftBaselineState {
    /// Registered and waiting for the fitter.
    Pending,
    /// Claimed by a fitter that is reading and fitting the baseline Data.
    Building,
    /// Fitted and persisted; runs may start.
    Ready,
    /// The last fit attempt failed; see the structured error.
    Failed,
}

impl DriftBaselineState {
    /// Stored and wire value of this state.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Building => "building",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }

    /// Decode a stored state value, or `None` outside the closed set.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "building" => Some(Self::Building),
            "ready" => Some(Self::Ready),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

/// Server-derived fitted-baseline status of a PSI or SPC Drift Verifier.
///
/// Served on `card.status.verification.baseline`; it never mutates the
/// authored spec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct DriftBaselineStatus {
    /// Current fit lifecycle state.
    pub state: DriftBaselineState,
    /// Exact baseline Data Card the profile is fitted from, UID-pinned.
    pub data: CardRef,
    /// Structured error of the last failed fit attempt, when one failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<VerificationError>,
}

/// One inline declaration attaching a Verifier to its containing subject.
///
/// Authored on a Service, a Service component occurrence, or a standalone
/// Agent. Registration resolves and UID-pins the Verifier, the effective
/// Trigger, and every effective Operator before the owning Card persists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct VerificationBinding {
    /// The exact Verifier Card this subject is verified by.
    pub verifier: Ref,
    /// When the bound Verifier runs, inline or as a referenced Trigger Card.
    pub runs_on: InlineableRef<TriggerSpec>,
    /// Operators dispatched independently after a `failed` verdict.
    ///
    /// Empty means a failed result stays durable with no reaction.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on_failure: Vec<InlineableRef<OperatorSpec>>,
}
