//! Row mirrors for `wyrd.cards`.
#![deny(missing_docs)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use wyrd_semver::VersionBlock;

use wyrd_runtime::principal::PrincipalId;
use wyrd_spec::envelope::{CardKind, Spec};
use wyrd_spec::ids::{CardName, CardUid, SpaceName};
use wyrd_spec::metadata::{Annotations, Labels};

use crate::error::SqlError;

/// Shared SELECT column list for `wyrd.cards` queries.
///
/// Must stay in lockstep with the positional `FromRow` decode order on [`CardRow`].
pub const CARD_ROW_COLUMNS: &str = "card_uid, data_tenant_id, kind, space, name, version, \
     spec, spec_hash, artifact_hash, labels, annotations, \
     status, created_by, created_at, updated_at";

/// Live `wyrd.cards` row, decoded for handler return.
///
/// `spec`, `labels`, and `annotations` decode lazily through [`serde_json::Value`]
/// so metadata-only queries skip per-kind deserialization. Convert via
/// [`ParsedCardRow::try_from`] when the typed [`Spec`] is needed.
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct CardRow {
    /// Stable per-card UUID (PK).
    pub card_uid: Uuid,
    /// Tenant isolation key.
    pub data_tenant_id: Uuid,
    /// Wire-name kind string (`"Data"`, `"Model"`, …).
    pub kind: String,
    /// Space slug.
    pub space: String,
    /// Card name slug.
    pub name: String,
    /// Canonical version block string (resolved semver pin).
    pub version: String,
    /// Raw JSONB spec payload (lazy decode).
    #[sqlx(json)]
    pub spec: serde_json::Value,
    /// BLAKE3-256 of the canonical spec bytes (lowercase hex, 64 chars).
    pub spec_hash: String,
    /// BLAKE3-256 of the artifact blob, when present.
    pub artifact_hash: Option<String>,
    /// Display labels JSONB (lazy decode).
    #[sqlx(json)]
    pub labels: serde_json::Value,
    /// Free-form annotations JSONB (lazy decode).
    #[sqlx(json)]
    pub annotations: serde_json::Value,
    /// Lifecycle status literal (`"active"`, `"deprecated"`, `"deleted"`).
    pub status: String,
    /// Creating principal UUID, if known.
    pub created_by: Option<Uuid>,
    /// Row creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Row update timestamp.
    pub updated_at: DateTime<Utc>,
}

impl CardRow {
    /// Borrow the raw JSON spec without materializing the typed [`Spec`].
    #[must_use]
    pub fn spec_value(&self) -> &serde_json::Value {
        &self.spec
    }
}

/// Owned, typed representation of a parsed `wyrd.cards` row.
///
/// Returned from `get_card_by_uid` / `get_card_by_ref` after kind-discriminated
/// [`Spec`] deserialization runs. Use [`CardRow`] for metadata-only paths.
#[derive(Debug, Clone)]
pub struct ParsedCardRow {
    /// Typed Card UID.
    pub card_uid: CardUid,
    /// Tenant isolation key.
    pub data_tenant_id: Uuid,
    /// Card kind discriminator.
    pub kind: CardKind,
    /// Typed space slug.
    pub space: SpaceName,
    /// Typed card name slug.
    pub name: CardName,
    /// Parsed version block.
    pub version: VersionBlock,
    /// Typed kind-specific spec.
    pub spec: Spec,
    /// BLAKE3-256 of the canonical spec bytes.
    pub spec_hash: String,
    /// BLAKE3-256 of the artifact blob, when present.
    pub artifact_hash: Option<String>,
    /// Display labels.
    pub labels: Labels,
    /// Free-form annotations.
    pub annotations: Annotations,
    /// Lifecycle status.
    pub status: CardStatus,
    /// Creating principal UUID, when known.
    pub created_by: Option<PrincipalId>,
    /// Row creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Row update timestamp.
    pub updated_at: DateTime<Utc>,
}

impl TryFrom<CardRow> for ParsedCardRow {
    type Error = SqlError;

    fn try_from(row: CardRow) -> Result<Self, Self::Error> {
        let kind =
            CardKind::from_wire_name(&row.kind).ok_or_else(|| SqlError::InvariantViolation {
                detail: format!("cards.kind has invalid value {:?}", row.kind),
            })?;
        let space = SpaceName::new(row.space).map_err(SqlError::from_id_error)?;
        let name = CardName::new(row.name).map_err(SqlError::from_id_error)?;
        let version: VersionBlock = row.version.parse().map_err(SqlError::from_version)?;
        let spec =
            Spec::from_kind_and_value(&kind, row.spec).map_err(SqlError::from_spec_decode)?;
        let labels: Labels =
            serde_json::from_value(row.labels).map_err(|e| SqlError::InvariantViolation {
                detail: format!("cards.labels decode failed: {e}"),
            })?;
        let annotations: Annotations =
            serde_json::from_value(row.annotations).map_err(|e| SqlError::InvariantViolation {
                detail: format!("cards.annotations decode failed: {e}"),
            })?;
        let status = CardStatus::from_db_str(&row.status)?;
        let card_uid = CardUid::from_uuid(row.card_uid).map_err(SqlError::from_id_error)?;
        let created_by = row.created_by.map(PrincipalId::new);
        Ok(Self {
            card_uid,
            data_tenant_id: row.data_tenant_id,
            kind,
            space,
            name,
            version,
            spec,
            spec_hash: row.spec_hash,
            artifact_hash: row.artifact_hash,
            labels,
            annotations,
            status,
            created_by,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

/// `wyrd.cards.status` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardStatus {
    /// Card is registered and discoverable.
    Active,
    /// Card is retained but hidden from default queries.
    Deprecated,
    /// Card is tombstoned for audit retention.
    Deleted,
}

impl CardStatus {
    /// Return the DB CHECK literal for this status.
    #[must_use]
    pub fn as_db_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Deprecated => "deprecated",
            Self::Deleted => "deleted",
        }
    }

    /// Parse a DB CHECK literal into a status.
    ///
    /// # Errors
    /// Returns [`SqlError::InvariantViolation`] when the literal does not match a known variant.
    pub fn from_db_str(value: &str) -> Result<Self, SqlError> {
        match value {
            "active" => Ok(Self::Active),
            "deprecated" => Ok(Self::Deprecated),
            "deleted" => Ok(Self::Deleted),
            other => Err(SqlError::InvariantViolation {
                detail: format!("cards.status has invalid value {other:?}"),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_status_db_roundtrip() {
        for (variant, literal) in [
            (CardStatus::Active, "active"),
            (CardStatus::Deprecated, "deprecated"),
            (CardStatus::Deleted, "deleted"),
        ] {
            assert_eq!(variant.as_db_str(), literal);
            assert_eq!(CardStatus::from_db_str(literal).unwrap(), variant);
        }
        assert!(CardStatus::from_db_str("unknown").is_err());
    }
}
