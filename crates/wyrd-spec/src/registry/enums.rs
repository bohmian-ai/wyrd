//! Registration lifecycle enums.

use serde::{Deserialize, Serialize};

/// Server-managed lifecycle state for a Card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum CardLifecycleStatus {
    /// Registration is waiting for artifact upload and verification.
    Pending,
    /// Card is available for use.
    Active,
    /// Card has been superseded by a newer version.
    Deprecated,
    /// Registration failed and awaits cleanup or retry.
    Failed,
    /// Registration expired before completion.
    Expired,
    /// Card was soft-deleted.
    Deleted,
}

/// Result of a registration write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum RegisterOutcome {
    /// A new registration row was created.
    Created,
    /// The same idempotent request was replayed.
    IdempotentNoop,
    /// An active card with identical content already exists.
    Deduplicated,
}

#[cfg(test)]
mod tests {
    use super::{CardLifecycleStatus, RegisterOutcome};
    use crate::error::WyrdError;

    #[test]
    fn lifecycle_enums_use_snake_case_wire_names() {
        assert_eq!(
            serde_json::to_string(&CardLifecycleStatus::Pending).unwrap(),
            "\"pending\""
        );
        assert_eq!(
            serde_json::to_string(&RegisterOutcome::IdempotentNoop).unwrap(),
            "\"idempotent_noop\""
        );
    }

    #[test]
    fn registry_error_variants_expose_stable_public_codes() {
        // Task 01 requires every registry-facing WyrdError variant to expose a
        // stable public code so language-agnostic clients can route on it. The
        // review flagged this as a missing verification gate (V-006); each
        // entry below assert-locks the (code, status) pair for one variant.
        let cases: &[(WyrdError, &str, u16)] = &[
            (
                WyrdError::RegistryInvalidCardSpec {
                    message: "bad".into(),
                    details: serde_json::json!({}),
                },
                "WYRD_REG_400_INVALID_CARD_SPEC",
                400,
            ),
            (
                WyrdError::RegistryInvalidVersionBlock {
                    message: "bad".into(),
                    details: serde_json::json!({}),
                },
                "WYRD_REG_400_INVALID_VERSION_BLOCK",
                400,
            ),
            (
                WyrdError::RegistrySpecTooLarge {
                    message: "too large".into(),
                    details: serde_json::json!({}),
                },
                "WYRD_REG_400_SPEC_TOO_LARGE",
                400,
            ),
            (
                WyrdError::RegistryCardNotFound {
                    message: "missing".into(),
                    details: serde_json::json!({}),
                },
                "WYRD_REG_404_CARD_NOT_FOUND",
                404,
            ),
            (
                WyrdError::RegistrySpecDrift {
                    message: "drift".into(),
                    details: serde_json::json!({}),
                },
                "WYRD_REG_409_SPEC_DRIFT",
                409,
            ),
            (
                WyrdError::RegistryIdempotencyConflict {
                    message: "conflict".into(),
                    details: serde_json::json!({}),
                },
                "WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT",
                409,
            ),
            (
                WyrdError::RegistryOperationExpired {
                    message: "expired".into(),
                    details: serde_json::json!({}),
                },
                "WYRD_REGISTRY_410_OPERATION_EXPIRED",
                410,
            ),
            (
                WyrdError::RegistryArtifactVerifyFailed {
                    message: "hash mismatch".into(),
                    details: serde_json::json!({}),
                },
                "WYRD_REGISTRY_507_ARTIFACT_VERIFY_FAILED",
                507,
            ),
        ];
        for (error, expected_code, expected_status) in cases {
            assert_eq!(error.code(), *expected_code);
            assert_eq!(error.status(), *expected_status);
        }
    }
}
