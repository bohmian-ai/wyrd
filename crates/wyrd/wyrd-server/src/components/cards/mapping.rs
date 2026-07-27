//! Projections from registry rows into composite registration responses.

use std::collections::BTreeMap;

use wyrd_spec::envelope::{CardRelationship, Relationships, Spec};
use wyrd_spec::reference::{CardRef, scope_child_card_refs};
use wyrd_spec::registry::{CardLifecycleStatus, CardRegistrationOutcome, RegistrationOutcomeKind};
use wyrd_sql::queries::cards::RegisteredCardRow;
use wyrd_sql::row_types::cards::{CardStatus, ParsedCardRow};

/// Project one durable card row and write classification onto the wire.
#[must_use]
pub fn outcome_row_to_response(
    row: &RegisteredCardRow,
    outcome: RegistrationOutcomeKind,
) -> CardRegistrationOutcome {
    CardRegistrationOutcome {
        card_ref: CardRef {
            kind: row.kind.clone(),
            name: row.name.clone(),
            version: row.version.clone(),
            space: Some(row.space.clone()),
            uid: Some(row.card_uid.clone()),
        },
        spec_hash: row.spec_hash.clone(),
        artifact_hash: row.artifact_hash.clone(),
        status: lifecycle_status(row.status),
        outcome,
        card_blob_uri: None,
    }
}

/// Project an existing row used by pin idempotency or content deduplication.
#[must_use]
pub fn existing_row_to_response(
    row: &ParsedCardRow,
    outcome: RegistrationOutcomeKind,
) -> CardRegistrationOutcome {
    CardRegistrationOutcome {
        card_ref: CardRef {
            kind: row.kind.clone(),
            name: row.name.clone(),
            version: row.version.clone(),
            space: Some(row.space.clone()),
            uid: Some(row.card_uid.clone()),
        },
        spec_hash: row.spec_hash.clone(),
        artifact_hash: row.artifact_hash.clone(),
        status: lifecycle_status(row.status),
        outcome,
        card_blob_uri: row
            .card_blob_uri
            .as_deref()
            .and_then(|uri| uri.parse().ok()),
    }
}

/// Project the normalized outbound Card references from a resolved spec.
pub(crate) fn relationships_from_spec(spec: &Spec) -> Relationships {
    let mut refs = scope_child_card_refs(spec);
    refs.sort_by_key(ToString::to_string);
    refs.dedup();
    let aliases = match spec {
        Spec::Service(service) => service
            .components
            .iter()
            .filter_map(|component| {
                component
                    .card_ref
                    .as_card_ref()
                    .map(|card_ref| (card_ref.to_string(), component.alias.clone()))
            })
            .fold(
                BTreeMap::<String, Vec<String>>::new(),
                |mut aliases, (card_ref, alias)| {
                    aliases.entry(card_ref).or_default().push(alias);
                    aliases
                },
            ),
        _ => BTreeMap::new(),
    };
    let outbound = refs.iter().map(ToString::to_string).collect::<Vec<_>>();
    let outbound_refs = refs
        .iter()
        .flat_map(|card_ref| {
            aliases.get(&card_ref.to_string()).map_or_else(
                || {
                    vec![CardRelationship {
                        card_ref: card_ref.clone(),
                        alias: None,
                    }]
                },
                |aliases| {
                    aliases
                        .iter()
                        .map(|alias| CardRelationship {
                            card_ref: card_ref.clone(),
                            alias: Some(alias.clone()),
                        })
                        .collect()
                },
            )
        })
        .collect::<Vec<_>>();
    Relationships {
        outbound,
        outbound_refs,
        inbound: Vec::new(),
        inbound_refs: Vec::new(),
    }
}

/// Convert the SQL lifecycle enum without stringly response mapping.
const fn lifecycle_status(status: CardStatus) -> CardLifecycleStatus {
    match status {
        CardStatus::Pending => CardLifecycleStatus::Pending,
        CardStatus::Active => CardLifecycleStatus::Active,
        CardStatus::Deprecated => CardLifecycleStatus::Deprecated,
        CardStatus::Deleted => CardLifecycleStatus::Deleted,
        CardStatus::Failed => CardLifecycleStatus::Failed,
        CardStatus::Expired => CardLifecycleStatus::Expired,
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;
    use wyrd_runtime::principal::PrincipalId;
    use wyrd_semver::VersionBlock;
    use wyrd_spec::envelope::{CardKind, Spec};
    use wyrd_spec::ids::{CardName, CardUid, SpaceName};
    use wyrd_spec::registry::{
        CardLifecycleStatus, RegistrationOperationId, RegistrationOutcomeKind,
    };
    use wyrd_sql::queries::cards::RegisteredCardRow;
    use wyrd_sql::row_types::cards::CardStatus;

    use super::outcome_row_to_response;
    use super::relationships_from_spec;

    /// Project a resolved sibling/external reference into deterministic blob data.
    #[test]
    fn relationships_project_uid_bearing_refs() {
        let spec = Spec::from_kind_and_value(
            &CardKind::Agent,
            serde_json::json!({
                "prompt": {
                    "kind": "Prompt",
                    "name": "prompt",
                    "version": "1.0.0",
                    "space": "default",
                    "uid": "018f0000-0000-7000-8000-000000000001"
                }
            }),
        )
        .expect("agent reference fixture decodes");

        let relationships = relationships_from_spec(&spec);

        assert_eq!(
            relationships.outbound,
            vec!["default/Prompt/prompt@1.0.0#018f0000-0000-7000-8000-000000000001"]
        );
        assert_eq!(relationships.outbound_refs.len(), 1);
        assert_eq!(
            relationships.outbound_refs[0].card_ref.to_string(),
            "default/Prompt/prompt@1.0.0#018f0000-0000-7000-8000-000000000001"
        );
        assert!(relationships.inbound.is_empty());
    }

    /// Build one deterministic durable row for response projection tests.
    fn row(status: CardStatus) -> RegisteredCardRow {
        RegisteredCardRow {
            card_uid: CardUid::from_uuid(Uuid::now_v7())
                .expect("test_setup: UUIDv7 is a valid card UID"),
            kind: CardKind::Prompt,
            space: SpaceName::new("default").expect("test_setup: static space is valid"),
            name: CardName::new("projection").expect("test_setup: static name is valid"),
            version: VersionBlock::parse("1.0.0").expect("test_setup: static version is valid"),
            spec_hash: "spec-hash".to_owned(),
            artifact_hash: None,
            status,
            created_at: Utc::now(),
            principal_id: PrincipalId::new(Uuid::now_v7()),
            operation_id: RegistrationOperationId::new(Uuid::now_v7()),
        }
    }

    /// Project every durable lifecycle status through its typed wire variant.
    #[test]
    fn outcome_projects_typed_status() {
        for (stored, expected) in [
            (CardStatus::Pending, CardLifecycleStatus::Pending),
            (CardStatus::Active, CardLifecycleStatus::Active),
            (CardStatus::Deprecated, CardLifecycleStatus::Deprecated),
            (CardStatus::Deleted, CardLifecycleStatus::Deleted),
            (CardStatus::Failed, CardLifecycleStatus::Failed),
            (CardStatus::Expired, CardLifecycleStatus::Expired),
        ] {
            let outcome =
                outcome_row_to_response(&row(stored), RegistrationOutcomeKind::Registered);
            assert_eq!(outcome.status, expected);
        }
    }

    /// Preserve UID-bearing card identity and registration classification.
    #[test]
    fn outcome_projects_registered_identity() {
        let row = row(CardStatus::Pending);
        let projected = outcome_row_to_response(&row, RegistrationOutcomeKind::Registered);

        assert_eq!(projected.card_ref.uid.as_ref(), Some(&row.card_uid));
        assert_eq!(projected.spec_hash, row.spec_hash);
        assert_eq!(projected.outcome, RegistrationOutcomeKind::Registered);
        assert_eq!(projected.status, CardLifecycleStatus::Pending);
    }
}
