//! Intelligence-layer wire contract — [`Evidence`] and its [`Lineage`].
//!
//! The intelligence substrate ("Fathom") answers questions by running governed
//! accessors over the typed card graph and the observation warehouse. Every
//! accessor returns [`Evidence`]; consumers (the default Fathom workflow or a
//! client-authored workflow) reason over it. Accessors only read — durable
//! writes such as a Finding card are a consumer action through the governed
//! registry, never accessor output.
//!
//! [`Lineage`] is what makes an answer verifiable: each piece of Evidence
//! carries the card uid/version it concerns, the code [`Origin`], the query
//! that produced it, the time range, and any referenced cards. A claim with no
//! lineage is not Evidence.
//!
//! The accessor trait and its implementations live in the owning intelligence
//! crate; `wyrd-spec` defines only these PyO3-free wire types.

use serde::{Deserialize, Serialize};

use crate::ids::CardUid;
use crate::origin::Origin;
use crate::reference::CardRef;

/// Closed time window an analytical accessor read over.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct TimeRange {
    /// Inclusive window start.
    pub start: chrono::DateTime<chrono::Utc>,
    /// Exclusive window end.
    pub end: chrono::DateTime<chrono::Utc>,
}

/// Provenance attached to every piece of [`Evidence`].
///
/// All fields are optional so an accessor cites only what applies: a card
/// lookup fills `card_uid`/`card_version`/`origin`; an analytical query fills
/// `query`/`time_range`; graph expansion fills `source_refs`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct Lineage {
    /// The card this evidence concerns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_uid: Option<CardUid>,
    /// Resolved version pin of the cited card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_version: Option<String>,
    /// Code origin of the cited card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<Origin>,
    /// The query (e.g. SQL) that produced this evidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    /// Time window the evidence was read over.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_range: Option<TimeRange>,
    /// Cards referenced while producing this evidence (graph provenance).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_refs: Vec<CardRef>,
}

/// A single governed fact returned by an intelligence accessor.
///
/// `data` is the accessor-specific payload (a card projection, a result row
/// set, a latency aggregate). `lineage` ties it back to the governed graph and
/// warehouse so the answer is verifiable and version-locked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct Evidence {
    /// Optional human-readable statement this evidence supports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim: Option<String>,
    /// Accessor-specific result payload.
    #[schemars(with = "serde_json::Value")]
    #[cfg_attr(feature = "server", schema(value_type = serde_json::Value))]
    pub data: serde_json::Value,
    /// Provenance for this evidence.
    #[serde(default)]
    pub lineage: Lineage,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::origin::CommitSha;

    #[test]
    fn evidence_round_trips_and_omits_empty_lineage_fields() {
        let evidence = Evidence {
            claim: Some("tool calls time out at 3% p99".to_string()),
            data: serde_json::json!({ "p99_ms": 5200, "timeout_rate": 0.03 }),
            lineage: Lineage {
                origin: Some(Origin {
                    repo: "github.com/org/repo".to_string(),
                    commit: CommitSha::new("deadbeef").expect("valid commit"),
                    path: None,
                    dirty: false,
                }),
                query: Some("SELECT ...".to_string()),
                ..Lineage::default()
            },
        };
        let json = serde_json::to_value(&evidence).expect("serialize");
        assert_eq!(json["lineage"].get("card_uid"), None);
        assert_eq!(json["lineage"].get("source_refs"), None);
        let back: Evidence = serde_json::from_value(json).expect("deserialize");
        assert_eq!(evidence, back);
    }

    #[test]
    fn evidence_defaults_lineage_when_absent() {
        let json = serde_json::json!({ "data": { "ok": true } });
        let evidence: Evidence = serde_json::from_value(json).expect("deserialize");
        assert_eq!(evidence.lineage, Lineage::default());
        assert!(evidence.claim.is_none());
    }

    #[test]
    fn schema_generates() {
        let _ = schemars::schema_for!(Evidence);
    }
}
