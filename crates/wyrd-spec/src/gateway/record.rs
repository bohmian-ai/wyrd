//! Accounting ledger, call capture, and attempt-span record contracts.
//!
//! The accounting ledger is the always-on Postgres authority for budgets and
//! cost. The call payload and attempt-span fields are opt-in analytical
//! projections of the same evidence. None of these records carries a
//! credential; only the call payload may carry redacted request or response
//! content, and only when capture policy selects it.

use std::collections::BTreeSet;
use std::num::{NonZeroU32, NonZeroU64};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{
    CurrencyCode, GatewayContractError, GatewayDecimal, GatewayOperation, GatewayPolicySubject,
    ModelRef,
};
use crate::auth::PrincipalId;
use crate::ids::{DataTenantId, ProviderDeploymentName};

/// Declares one UUIDv7-backed gateway record identifier.
macro_rules! gateway_uuid_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Debug,
            Clone,
            Copy,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Hash,
            Serialize,
            Deserialize,
            schemars::JsonSchema,
        )]
        #[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
        #[serde(transparent)]
        pub struct $name(uuid::Uuid);

        impl $name {
            /// Generates a new UUIDv7 identifier.
            #[must_use]
            pub fn new_v7() -> Self {
                Self(uuid::Uuid::now_v7())
            }

            /// Wraps an existing UUID.
            #[must_use]
            pub const fn from_uuid(value: uuid::Uuid) -> Self {
                Self(value)
            }

            /// Borrows the underlying UUID.
            #[must_use]
            pub const fn as_uuid(&self) -> uuid::Uuid {
                self.0
            }
        }
    };
}

/// Fixed ceiling, in serialized UTF-8 bytes, of every inline canonical JSON
/// field in the accounting, capture, and span records.
///
/// It equals the default one-MiB Wyrd request-body limit but is deliberately
/// not tied to that setting: an operator changing the HTTP limit never changes
/// which records are valid.
pub const GATEWAY_JSON_MAX_BYTES: usize = 1_048_576;

gateway_uuid_id!(GatewayCallId, "Logical gateway call identifier.");
gateway_uuid_id!(
    GatewayAccountingEntryId,
    "Append-only accounting ledger entry identifier and idempotency fence."
);
gateway_uuid_id!(
    GatewayBudgetReservationId,
    "Budget reservation identifier and idempotency fence."
);

/// Terminal outcome of a logical call or attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum GatewayCallOutcome {
    /// Completed successfully.
    Succeeded,
    /// Failed.
    Failed,
    /// Cancelled by the caller or shutdown.
    Cancelled,
    /// Exceeded its deadline.
    TimedOut,
}

/// One normalized usage quantity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct GatewayUsageAmount {
    /// Open usage dimension, such as `output_tokens`.
    #[schemars(length(min = 1, max = 128))]
    pub dimension: String,
    /// Usage unit, such as `tokens`.
    #[schemars(length(min = 1, max = 128))]
    pub unit: String,
    /// Non-negative quantity.
    pub quantity: GatewayDecimal,
}

impl GatewayUsageAmount {
    /// Rejects an empty, over-long, or control-bearing dimension or unit.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayContractError`] naming `field.dimension` or
    /// `field.unit`.
    pub fn validate(&self, field: &str) -> Result<(), GatewayContractError> {
        super::validate_text(&format!("{field}.dimension"), &self.dimension)?;
        super::validate_text(&format!("{field}.unit"), &self.unit)
    }
}

/// One append-only, tenant-scoped accounting ledger entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum GatewayAccountingEntryV1 {
    /// A bounded cost reservation admitted with a budget check.
    BudgetReservationCreated {
        /// Entry identity.
        entry_id: GatewayAccountingEntryId,
        /// Reservation identity.
        reservation_id: GatewayBudgetReservationId,
        /// Logical call.
        call_id: GatewayCallId,
        /// Tenant.
        data_tenant_id: DataTenantId,
        /// Budgeted subject.
        subject: GatewayPolicySubject,
        /// Positive reserved cost.
        reserved_cost: GatewayDecimal,
        /// Reservation currency.
        currency: CurrencyCode,
        /// Admission time.
        created_at: DateTime<Utc>,
        /// Start of the UTC calendar period fixed at admission.
        period_start: DateTime<Utc>,
        /// End of that period.
        period_end: DateTime<Utc>,
        /// Time after which reconciliation may release it.
        expires_at: DateTime<Utc>,
    },
    /// Settlement of a reservation with actual cost.
    BudgetReservationSettled {
        /// Entry identity.
        entry_id: GatewayAccountingEntryId,
        /// Settled reservation.
        reservation_id: GatewayBudgetReservationId,
        /// Logical call.
        call_id: GatewayCallId,
        /// Tenant.
        data_tenant_id: DataTenantId,
        /// Non-negative actual cost.
        actual_cost: GatewayDecimal,
        /// Non-negative released remainder.
        released_cost: GatewayDecimal,
        /// Settlement currency.
        currency: CurrencyCode,
        /// Settlement time.
        settled_at: DateTime<Utc>,
    },
    /// Accounting for one upstream attempt.
    AttemptAccounted {
        /// Entry identity.
        entry_id: GatewayAccountingEntryId,
        /// Logical call.
        call_id: GatewayCallId,
        /// Tenant.
        data_tenant_id: DataTenantId,
        /// One-based attempt ordinal.
        #[cfg_attr(feature = "server", schema(value_type = u32, minimum = 1))]
        attempt_ordinal: NonZeroU32,
        /// Deployment attempted.
        deployment: ProviderDeploymentName,
        /// Model attempted.
        model: ModelRef,
        /// Attempt outcome.
        outcome: GatewayCallOutcome,
        /// Canonical bounded provider-usage JSON from the typed adapter usage.
        /// Bounded by the fixed ceiling of 1,048,576 serialized UTF-8 bytes. `maxLength`
        /// counts characters, so a multibyte value within it can still exceed the
        /// byte ceiling and is rejected.
        #[schemars(length(max = 1_048_576))]
        provider_usage_json: Option<String>,
        /// Normalized usage, or null when unknown.
        normalized_usage: Option<Vec<GatewayUsageAmount>>,
        /// Cost, or null when unknown.
        cost: Option<GatewayDecimal>,
        /// Currency, present exactly when cost is.
        currency: Option<CurrencyCode>,
        /// Pricing version, present exactly when cost is.
        #[schemars(length(min = 1, max = 128))]
        pricing_version: Option<String>,
        /// Accounting time.
        accounted_at: DateTime<Utc>,
    },
    /// Accounting for the logical call.
    CallAccounted {
        /// Entry identity.
        entry_id: GatewayAccountingEntryId,
        /// Logical call.
        call_id: GatewayCallId,
        /// Tenant.
        data_tenant_id: DataTenantId,
        /// Verified caller.
        caller_principal_id: PrincipalId,
        /// Call outcome.
        outcome: GatewayCallOutcome,
        /// Normalized usage, or null when unknown.
        normalized_usage: Option<Vec<GatewayUsageAmount>>,
        /// Cost, or null when unknown.
        cost: Option<GatewayDecimal>,
        /// Currency, present exactly when cost is.
        currency: Option<CurrencyCode>,
        /// Pricing versions used by billed attempts; empty when unpriced.
        #[schemars(inner(length(min = 1, max = 128)))]
        pricing_versions: BTreeSet<String>,
        /// Accounting time.
        accounted_at: DateTime<Utc>,
    },
}

impl GatewayAccountingEntryV1 {
    /// Rejects a zero reservation, invalid nested usage, non-canonical or
    /// over-bound provider usage JSON, and inconsistent billing groups.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayContractError`] naming the first offending field.
    pub fn validate(&self) -> Result<(), GatewayContractError> {
        match self {
            Self::BudgetReservationCreated { reserved_cost, .. } if reserved_cost.is_zero() => Err(
                GatewayContractError::new("reserved_cost", "must be positive"),
            ),
            Self::AttemptAccounted {
                provider_usage_json,
                normalized_usage,
                cost,
                currency,
                pricing_version,
                ..
            } => {
                validate_canonical_json("provider_usage_json", provider_usage_json.as_deref())?;
                validate_usage("normalized_usage", normalized_usage.as_deref())?;
                validate_attempt_billing(
                    cost.as_ref(),
                    currency.as_ref(),
                    pricing_version.as_deref(),
                )
            }
            Self::CallAccounted {
                normalized_usage,
                cost,
                currency,
                pricing_versions,
                ..
            } => {
                validate_usage("normalized_usage", normalized_usage.as_deref())?;
                validate_cost_pair(cost.as_ref(), currency.as_ref())?;
                validate_pricing_versions(cost.as_ref(), pricing_versions)
            }
            _ => Ok(()),
        }
    }
}

/// Validates every element of an optional normalized usage list.
///
/// # Errors
///
/// Returns [`GatewayContractError`] naming `field[index]` for the first invalid
/// amount.
fn validate_usage(
    field: &str,
    usage: Option<&[GatewayUsageAmount]>,
) -> Result<(), GatewayContractError> {
    usage
        .into_iter()
        .flatten()
        .enumerate()
        .try_for_each(|(index, amount)| amount.validate(&format!("{field}[{index}]")))
}

/// Rejects a per-attempt billing group that is not all present or all null,
/// or whose pricing version is not bounded text.
///
/// # Errors
///
/// Returns [`GatewayContractError`] for field `cost` or `pricing_version`.
fn validate_attempt_billing(
    cost: Option<&GatewayDecimal>,
    currency: Option<&CurrencyCode>,
    pricing_version: Option<&str>,
) -> Result<(), GatewayContractError> {
    if cost.is_some() != currency.is_some() || cost.is_some() != pricing_version.is_some() {
        return Err(GatewayContractError::new(
            "cost",
            "cost, currency, and pricing_version are all present or all null",
        ));
    }
    pricing_version.map_or(Ok(()), |version| {
        super::validate_text("pricing_version", version)
    })
}

/// Rejects a call pricing-version set containing invalid text, or one that
/// is empty for a priced call or non-empty for an unpriced call.
///
/// # Errors
///
/// Returns [`GatewayContractError`] for field `pricing_versions`.
fn validate_pricing_versions(
    cost: Option<&GatewayDecimal>,
    versions: &BTreeSet<String>,
) -> Result<(), GatewayContractError> {
    versions
        .iter()
        .enumerate()
        .try_for_each(|(index, version)| {
            super::validate_text(&format!("pricing_versions[{index}]"), version)
        })?;
    if cost.is_some() == versions.is_empty() {
        return Err(GatewayContractError::new(
            "pricing_versions",
            "must be non-empty exactly when cost is present",
        ));
    }
    Ok(())
}

/// Rejects an inline JSON field that is not RFC 8785 canonical JSON of at most
/// [`GATEWAY_JSON_MAX_BYTES`] serialized UTF-8 bytes.
///
/// # Errors
///
/// Returns [`GatewayContractError`] naming `field` when the text is over the
/// ceiling, is not JSON, or differs from its canonical serialization.
fn validate_canonical_json(field: &str, value: Option<&str>) -> Result<(), GatewayContractError> {
    let Some(text) = value else {
        return Ok(());
    };
    if text.len() > GATEWAY_JSON_MAX_BYTES {
        return Err(GatewayContractError::new(
            field,
            "must be at most 1048576 serialized UTF-8 bytes",
        ));
    }
    let parsed: serde_json::Value = serde_json::from_str(text)
        .map_err(|_| GatewayContractError::new(field, "must be valid JSON"))?;
    match serde_jcs::to_string(&parsed) {
        Ok(canonical) if canonical == text => Ok(()),
        _ => Err(GatewayContractError::new(
            field,
            "must be RFC 8785 canonical JSON",
        )),
    }
}

/// Rejects an optional error code that is not a stable `WYRD_` catalog code.
///
/// # Errors
///
/// Returns [`GatewayContractError`] for field `error_code`.
fn validate_error_code(code: Option<&str>) -> Result<(), GatewayContractError> {
    let Some(code) = code else {
        return Ok(());
    };
    super::validate_text("error_code", code)?;
    let stable = code.starts_with("WYRD_")
        && code
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_');
    if stable {
        Ok(())
    } else {
        Err(GatewayContractError::new(
            "error_code",
            "must be a stable WYRD_ error code",
        ))
    }
}

/// Rejects a cost without currency or a currency without cost.
///
/// # Errors
///
/// Returns [`GatewayContractError`] for field `cost`.
fn validate_cost_pair(
    cost: Option<&GatewayDecimal>,
    currency: Option<&CurrencyCode>,
) -> Result<(), GatewayContractError> {
    if cost.is_some() == currency.is_some() {
        Ok(())
    } else {
        Err(GatewayContractError::new(
            "cost",
            "cost and currency are both present or both null",
        ))
    }
}

/// Canonical prefix of every captured payload object digest.
const PAYLOAD_OBJECT_DIGEST_PREFIX: &str = "sha256:";

/// Hexadecimal characters a SHA-256 digest carries after its prefix.
const PAYLOAD_OBJECT_DIGEST_HEX_LEN: usize = 64;

/// Longest accepted media type of one captured payload object.
const PAYLOAD_OBJECT_CONTENT_TYPE_MAX_BYTES: usize = 255;

/// Typed reference standing in for one captured binary payload object.
///
/// Large or binary request and response content never enters Bifrost. The
/// bytes live in Wyrd's existing object storage under a server-derived,
/// tenant-qualified key, and this reference is what the call row and the
/// selected payload JSON carry in their place. Within one verified tenant the
/// digest is both the object identity and the idempotency key, so identical
/// bytes converge on one stored object; the storage key itself is never
/// exposed or persisted. Field order is the sort key of
/// [`GatewayCallPayloadV1::payload_object_refs`], so the derived ordering is
/// the contract's ordering.
///
/// The configured storage bucket lifecycle owns expiration, so a retained
/// reference may outlive its object; retrieval then reports the object as
/// absent.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct GatewayPayloadObjectRefV1 {
    /// Canonical `sha256:` digest of the exact stored bytes.
    #[schemars(regex(pattern = r"^sha256:[0-9a-f]{64}$"))]
    pub digest: String,
    /// Media type the caller or provider gave the content, kept as
    /// descriptive metadata rather than a retrieval contract.
    #[schemars(length(min = 1, max = 255))]
    pub content_type: String,
    /// Exact stored byte length.
    pub size_bytes: u64,
}

impl GatewayPayloadObjectRefV1 {
    /// Rejects a non-canonical digest or an empty, over-long, or
    /// control-character-bearing media type.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayContractError`] naming the first offending field.
    pub fn validate(&self) -> Result<(), GatewayContractError> {
        let canonical = self
            .digest
            .strip_prefix(PAYLOAD_OBJECT_DIGEST_PREFIX)
            .is_some_and(|hex| {
                hex.len() == PAYLOAD_OBJECT_DIGEST_HEX_LEN
                    && hex
                        .chars()
                        .all(|character| character.is_ascii_hexdigit() && !character.is_uppercase())
            });
        if !canonical {
            return Err(GatewayContractError::new(
                "digest",
                "must be sha256: followed by 64 lowercase hexadecimal characters",
            ));
        }
        if self.content_type.is_empty()
            || self.content_type.len() > PAYLOAD_OBJECT_CONTENT_TYPE_MAX_BYTES
            || self.content_type.chars().any(char::is_control)
        {
            return Err(GatewayContractError::new(
                "content_type",
                "must be 1..=255 bytes without control characters",
            ));
        }
        Ok(())
    }
}

/// Rejects an invalid, unsorted, or duplicated captured-object reference.
///
/// The list is the authoritative projection of every object the row still
/// names, so lifecycle and audit surfaces read it instead of parsing arbitrary
/// payload JSON. Strict ordering gives both the sort and the duplicate-free
/// guarantee in one comparison.
///
/// # Errors
///
/// Returns [`GatewayContractError`] for field `payload_object_refs`, or for the
/// indexed member whose own contract fails.
fn validate_payload_object_refs(
    refs: &[GatewayPayloadObjectRefV1],
) -> Result<(), GatewayContractError> {
    for (index, reference) in refs.iter().enumerate() {
        reference.validate().map_err(|error| {
            GatewayContractError::new(
                format!("payload_object_refs[{index}].{}", error.field),
                error.reason,
            )
        })?;
    }
    if refs.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(GatewayContractError::new(
            "payload_object_refs",
            "must be sorted and duplicate-free by digest, content type, and size",
        ));
    }
    Ok(())
}

/// Exact version-1 payload of one `vala.gateway.calls` row.
///
/// Bifrost appends its canonical managed envelope; the payload never
/// duplicates managed identity and never substitutes zero for unknowns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct GatewayCallPayloadV1 {
    /// Always `1`.
    pub schema_version: u8,
    /// Logical call.
    pub call_id: GatewayCallId,
    /// Verified caller whose invocation caused the call.
    pub caller_principal_id: PrincipalId,
    /// Requested operation.
    pub operation: GatewayOperation,
    /// Ingress dialect, such as `openai_chat_completions`.
    #[schemars(length(min = 1, max = 128))]
    pub ingress_dialect: String,
    /// Requested model.
    pub requested_model: ModelRef,
    /// Resolved model after fallback, if resolved.
    pub resolved_model: Option<ModelRef>,
    /// Resolved deployment, if resolved.
    pub resolved_deployment: Option<ProviderDeploymentName>,
    /// Whether the call streamed.
    pub streaming: bool,
    /// Admission time.
    pub started_at: DateTime<Utc>,
    /// Terminal time.
    pub terminal_at: DateTime<Utc>,
    /// Terminal outcome.
    pub outcome: GatewayCallOutcome,
    /// Stable Wyrd error code on failure.
    #[schemars(length(min = 1, max = 128), regex(pattern = r"^WYRD_[A-Z0-9_]+$"))]
    pub error_code: Option<String>,
    /// Upstream attempts made.
    pub attempt_count: u32,
    /// Normalized usage, or null when unknown.
    pub usage: Option<Vec<GatewayUsageAmount>>,
    /// Cost, or null when unknown.
    pub cost: Option<GatewayDecimal>,
    /// Currency, present exactly when cost is.
    pub currency: Option<CurrencyCode>,
    /// Pricing versions used by billed attempts; empty when unpriced.
    #[schemars(inner(length(min = 1, max = 128)))]
    pub pricing_versions: BTreeSet<String>,
    /// Capture policy version admitted for the call.
    #[cfg_attr(feature = "server", schema(value_type = u64, minimum = 1))]
    pub capture_policy_version: NonZeroU64,
    /// Canonical redacted request JSON when selected.
    /// Bounded by the fixed ceiling of 1,048,576 serialized UTF-8 bytes. `maxLength`
    /// counts characters, so a multibyte value within it can still exceed the
    /// byte ceiling and is rejected.
    #[schemars(length(max = 1_048_576))]
    pub request_payload_json: Option<String>,
    /// Canonical redacted response JSON when selected.
    /// Bounded by the fixed ceiling of 1,048,576 serialized UTF-8 bytes. `maxLength`
    /// counts characters, so a multibyte value within it can still exceed the
    /// byte ceiling and is rejected.
    #[schemars(length(max = 1_048_576))]
    pub response_payload_json: Option<String>,
    /// Every captured binary payload object the row still names, sorted and
    /// duplicate-free. Metadata-only and disabled capture produce an empty
    /// list. The same serialized reference shape stands in place of each
    /// selected binary value inside the payload JSON above, so no bytes,
    /// base64, storage path, or backend locator ever reaches Bifrost.
    ///
    /// The field is additive, so an absent list decodes as empty and a payload
    /// serialized before captured objects existed still round-trips.
    #[serde(default)]
    pub payload_object_refs: Vec<GatewayPayloadObjectRefV1>,
}

impl GatewayCallPayloadV1 {
    /// Rejects a schema version other than 1, an invalid dialect, error code,
    /// usage amount, or pricing-version set, an inconsistent cost/currency
    /// pair, non-canonical or over-bound payload JSON, and an invalid,
    /// unsorted, or duplicated captured-object reference.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayContractError`] naming the first offending field.
    pub fn validate(&self) -> Result<(), GatewayContractError> {
        if self.schema_version != 1 {
            return Err(GatewayContractError::new("schema_version", "must be 1"));
        }
        super::validate_text("ingress_dialect", &self.ingress_dialect)?;
        validate_error_code(self.error_code.as_deref())?;
        validate_usage("usage", self.usage.as_deref())?;
        validate_cost_pair(self.cost.as_ref(), self.currency.as_ref())?;
        validate_pricing_versions(self.cost.as_ref(), &self.pricing_versions)?;
        validate_canonical_json("request_payload_json", self.request_payload_json.as_deref())?;
        validate_canonical_json(
            "response_payload_json",
            self.response_payload_json.as_deref(),
        )?;
        validate_payload_object_refs(&self.payload_object_refs)
    }
}

/// Exact attributes carried by each linked GenAI attempt span.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct GatewayAttemptSpanFieldsV1 {
    /// Logical call.
    pub gateway_call_id: GatewayCallId,
    /// One-based attempt ordinal.
    #[cfg_attr(feature = "server", schema(value_type = u32, minimum = 1))]
    pub attempt_ordinal: NonZeroU32,
    /// Deployment attempted.
    pub deployment: ProviderDeploymentName,
    /// Model attempted.
    pub model: ModelRef,
    /// Attempt outcome.
    pub outcome: GatewayCallOutcome,
    /// Stable Wyrd error code on failure.
    #[schemars(length(min = 1, max = 128), regex(pattern = r"^WYRD_[A-Z0-9_]+$"))]
    pub error_code: Option<String>,
    /// Attempt start.
    pub started_at: DateTime<Utc>,
    /// Attempt end.
    pub terminal_at: DateTime<Utc>,
    /// Canonical provider-native usage JSON.
    /// Bounded by the fixed ceiling of 1,048,576 serialized UTF-8 bytes. `maxLength`
    /// counts characters, so a multibyte value within it can still exceed the
    /// byte ceiling and is rejected.
    #[schemars(length(max = 1_048_576))]
    pub provider_usage_json: Option<String>,
    /// Normalized usage, or null when unknown.
    pub normalized_usage: Option<Vec<GatewayUsageAmount>>,
    /// Cost, or null when unknown.
    pub cost: Option<GatewayDecimal>,
    /// Currency, present exactly when cost is.
    pub currency: Option<CurrencyCode>,
    /// Pricing version, present exactly when cost is.
    #[schemars(length(min = 1, max = 128))]
    pub pricing_version: Option<String>,
}

impl GatewayAttemptSpanFieldsV1 {
    /// Rejects an invalid error code, usage amount, or billing group, and
    /// non-canonical or over-bound provider usage JSON.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayContractError`] naming the first offending field.
    pub fn validate(&self) -> Result<(), GatewayContractError> {
        validate_error_code(self.error_code.as_deref())?;
        validate_canonical_json("provider_usage_json", self.provider_usage_json.as_deref())?;
        validate_usage("normalized_usage", self.normalized_usage.as_deref())?;
        validate_attempt_billing(
            self.cost.as_ref(),
            self.currency.as_ref(),
            self.pricing_version.as_deref(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        GATEWAY_JSON_MAX_BYTES, GatewayAccountingEntryV1, GatewayAttemptSpanFieldsV1,
        GatewayCallPayloadV1,
    };
    use serde_json::{Value, json};

    /// One otherwise-minimal valid call payload carrying `refs`.
    fn payload_with_refs(refs: Value) -> Value {
        json!({
            "schema_version": 1,
            "call_id": "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b01",
            "caller_principal_id": "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b03",
            "operation": "chat_completions",
            "ingress_dialect": "openai_chat_completions",
            "requested_model": {"provider": "openai", "model": "gpt-4o"},
            "resolved_model": null,
            "resolved_deployment": null,
            "streaming": false,
            "started_at": "2026-09-13T00:00:00Z",
            "terminal_at": "2026-09-13T00:00:01Z",
            "outcome": "succeeded",
            "error_code": null,
            "attempt_count": 1,
            "usage": null,
            "cost": null,
            "currency": null,
            "pricing_versions": [],
            "capture_policy_version": 1,
            "request_payload_json": null,
            "response_payload_json": null,
            "payload_object_refs": refs,
        })
    }

    /// Proves the captured-object reference grammar and the row's sorted,
    /// duplicate-free reference list.
    ///
    /// Strict ordering is what gives lifecycle and audit surfaces one
    /// authoritative projection of the objects a row names, so an unsorted or
    /// repeated member is a contract violation rather than a formatting
    /// preference.
    #[test]
    fn payload_object_refs_are_canonical_sorted_and_duplicate_free() {
        let digest = |fill: &str| format!("sha256:{}", fill.repeat(64));
        let reference = |digest: String| {
            json!({
                "digest": digest,
                "content_type": "image/png",
                "size_bytes": 3,
            })
        };
        let decoded = |refs: Value| {
            serde_json::from_value::<GatewayCallPayloadV1>(payload_with_refs(refs))
                .expect("decodes")
        };
        decoded(json!([]))
            .validate()
            .expect("metadata capture names no object");
        decoded(json!([reference(digest("a")), reference(digest("b"))]))
            .validate()
            .expect("sorted and distinct");
        let unsorted = decoded(json!([reference(digest("b")), reference(digest("a"))]));
        assert_eq!(
            unsorted.validate().expect_err("unsorted is refused").field,
            "payload_object_refs"
        );
        let duplicated = decoded(json!([reference(digest("a")), reference(digest("a"))]));
        assert_eq!(
            duplicated
                .validate()
                .expect_err("a repeated object is refused")
                .field,
            "payload_object_refs"
        );
        for rejected in [
            digest("A"),
            "sha256:abc".to_owned(),
            format!("sha1:{}", "a".repeat(64)),
            "a".repeat(64),
        ] {
            assert_eq!(
                decoded(json!([reference(rejected.clone())]))
                    .validate()
                    .expect_err("non-canonical digest is refused")
                    .field,
                "payload_object_refs[0].digest",
                "{rejected}"
            );
        }
        let untyped = json!([{
            "digest": digest("a"),
            "content_type": "",
            "size_bytes": 3,
        }]);
        assert_eq!(
            decoded(untyped)
                .validate()
                .expect_err("an empty media type is refused")
                .field,
            "payload_object_refs[0].content_type"
        );
    }

    /// Proves the attempt entry's cost triple must be all-present or all-null
    /// and unknown usage stays null rather than zero.
    #[test]
    fn attempt_cost_triple_is_all_or_nothing() {
        let attempt =
            |cost: serde_json::Value, currency: serde_json::Value, version: serde_json::Value| {
                json!({"attempt_accounted": {
                    "entry_id": "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00",
                    "call_id": "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b01",
                    "data_tenant_id": "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b02",
                    "attempt_ordinal": 1,
                    "deployment": "primary",
                    "model": {"provider": "openai", "model": "gpt-4o"},
                    "outcome": "failed",
                    "provider_usage_json": null,
                    "normalized_usage": null,
                    "cost": cost, "currency": currency, "pricing_version": version,
                    "accounted_at": "2026-09-13T00:00:00Z",
                }})
            };
        let unpriced: GatewayAccountingEntryV1 =
            serde_json::from_value(attempt(json!(null), json!(null), json!(null)))
                .expect("decodes");
        unpriced.validate().expect("unknown cost stays null");
        let priced: GatewayAccountingEntryV1 =
            serde_json::from_value(attempt(json!("0.01"), json!("USD"), json!("v1")))
                .expect("decodes");
        priced.validate().expect("complete triple");
        let partial: GatewayAccountingEntryV1 =
            serde_json::from_value(attempt(json!("0.01"), json!(null), json!("v1")))
                .expect("decodes");
        assert!(partial.validate().is_err());
        assert!(
            serde_json::from_value::<GatewayAccountingEntryV1>(json!({"attempt_refunded": {}}))
                .is_err()
        );
    }

    /// A valid logical-call payload fixture that negative cases mutate.
    fn call_payload() -> Value {
        json!({
            "schema_version": 1,
            "call_id": "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b01",
            "caller_principal_id": "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b03",
            "operation": "chat_completions",
            "ingress_dialect": "openai_chat_completions",
            "requested_model": {"provider": "openai", "model": "gpt-4o"},
            "resolved_model": null,
            "resolved_deployment": null,
            "streaming": false,
            "started_at": "2026-09-13T00:00:00Z",
            "terminal_at": "2026-09-13T00:00:01Z",
            "outcome": "succeeded",
            "error_code": null,
            "attempt_count": 1,
            "usage": [{"dimension": "input_tokens", "unit": "tokens", "quantity": "12"}],
            "cost": "0.01",
            "currency": "USD",
            "pricing_versions": ["v1"],
            "capture_policy_version": 1,
            "request_payload_json": "{\"a\":1,\"b\":[true]}",
            "response_payload_json": null,
        })
    }

    /// Decodes `value` as a call payload and returns its validation field.
    fn call_rejection(value: Value) -> Option<String> {
        serde_json::from_value::<GatewayCallPayloadV1>(value)
            .expect("payload decodes")
            .validate()
            .err()
            .map(|error| error.field)
    }

    /// Proves nested usage, pricing sets, error codes, and inline JSON are
    /// validated on the call payload, including the fixed JSON ceiling.
    #[test]
    fn call_payload_rejects_invalid_nested_values_and_json() {
        assert_eq!(call_rejection(call_payload()), None);
        let cases: [(&str, Value, &str); 7] = [
            ("/usage/0/dimension", json!(""), "usage[0].dimension"),
            ("/usage/0/unit", json!(""), "usage[0].unit"),
            ("/pricing_versions", json!([""]), "pricing_versions[0]"),
            ("/pricing_versions", json!([]), "pricing_versions"),
            ("/error_code", json!("not stable"), "error_code"),
            (
                "/request_payload_json",
                json!("{\"b\":1,\"a\":2}"),
                "request_payload_json",
            ),
            (
                "/response_payload_json",
                json!("{not json"),
                "response_payload_json",
            ),
        ];
        for (pointer, replacement, field) in cases {
            let mut value = call_payload();
            *value.pointer_mut(pointer).expect("fixture field") = replacement;
            assert_eq!(call_rejection(value).as_deref(), Some(field), "{pointer}");
        }
        let mut unpriced = call_payload();
        unpriced["cost"] = Value::Null;
        unpriced["currency"] = Value::Null;
        assert_eq!(
            call_rejection(unpriced).as_deref(),
            Some("pricing_versions")
        );

        let filler = GATEWAY_JSON_MAX_BYTES - "[\"\"]".len();
        let at_ceiling = format!("[\"{}\"]", "a".repeat(filler));
        let mut value = call_payload();
        value["request_payload_json"] = json!(at_ceiling);
        assert_eq!(
            call_rejection(value),
            None,
            "exactly the ceiling is accepted"
        );
        let over = format!("[\"{}\"]", "a".repeat(filler + 1));
        let mut value = call_payload();
        value["request_payload_json"] = json!(over);
        assert_eq!(
            call_rejection(value).as_deref(),
            Some("request_payload_json")
        );
        let multibyte = format!("[\"{}\"]", "\u{e9}".repeat(filler / 2 + 1));
        assert!(multibyte.chars().count() <= GATEWAY_JSON_MAX_BYTES);
        let mut value = call_payload();
        value["request_payload_json"] = json!(multibyte);
        assert_eq!(
            call_rejection(value).as_deref(),
            Some("request_payload_json"),
            "a value under the character bound but over the byte ceiling is rejected"
        );
    }

    /// Proves attempt spans validate usage JSON, billing triples, and codes.
    #[test]
    fn attempt_span_fields_enforce_record_invariants() {
        let span = |patch: Value| {
            let mut value = json!({
                "gateway_call_id": "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b01",
                "attempt_ordinal": 1,
                "deployment": "primary",
                "model": {"provider": "openai", "model": "gpt-4o"},
                "outcome": "failed",
                "error_code": "WYRD_GATEWAY_502_UPSTREAM",
                "started_at": "2026-09-13T00:00:00Z",
                "terminal_at": "2026-09-13T00:00:01Z",
                "provider_usage_json": "{\"prompt_tokens\":3}",
                "normalized_usage": null,
                "cost": null, "currency": null, "pricing_version": null,
            });
            for (key, field) in patch.as_object().expect("patch object") {
                value[key] = field.clone();
            }
            serde_json::from_value::<GatewayAttemptSpanFieldsV1>(value)
                .expect("span decodes")
                .validate()
                .err()
                .map(|error| error.field)
        };
        assert_eq!(span(json!({})), None);
        assert_eq!(
            span(json!({"provider_usage_json": "{ \"prompt_tokens\": 3 }"})).as_deref(),
            Some("provider_usage_json")
        );
        assert_eq!(
            span(json!({"cost": "1", "currency": "USD", "pricing_version": null})).as_deref(),
            Some("cost")
        );
        assert_eq!(
            span(json!({"cost": "1", "currency": "USD", "pricing_version": ""})).as_deref(),
            Some("pricing_version")
        );
        assert_eq!(
            span(json!({"normalized_usage": [{"dimension": "x", "unit": "", "quantity": "1"}]}))
                .as_deref(),
            Some("normalized_usage[0].unit")
        );
    }

    /// Proves the generated record schemas publish the fixed JSON ceiling and
    /// the non-empty text bounds.
    #[test]
    fn record_schemas_publish_json_ceiling_and_text_bounds() {
        let schema = serde_json::to_value(schemars::schema_for!(GatewayCallPayloadV1))
            .expect("schema encodes");
        let properties = &schema["properties"];
        assert_eq!(properties["request_payload_json"]["maxLength"], 1_048_576);
        assert_eq!(properties["response_payload_json"]["maxLength"], 1_048_576);
        for property in [
            &properties["request_payload_json"],
            &properties["response_payload_json"],
        ] {
            assert!(
                property["description"]
                    .as_str()
                    .is_some_and(|text| text.contains("1,048,576 serialized UTF-8 bytes")),
                "{property}"
            );
        }
        assert_eq!(properties["ingress_dialect"]["minLength"], 1);
        assert_eq!(properties["pricing_versions"]["items"]["minLength"], 1);
        let span = serde_json::to_value(schemars::schema_for!(GatewayAttemptSpanFieldsV1))
            .expect("schema encodes");
        assert_eq!(
            span["properties"]["provider_usage_json"]["maxLength"],
            1_048_576
        );
    }
}
