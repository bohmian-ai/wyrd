//! Deterministic submission canonicalization.

use std::collections::BTreeMap;

use crate::envelope::{CardRelationship, Relationships, Spec};
use crate::reference::scope_child_card_refs;
use crate::registry::CardSubmission;

/// Derive the canonical outbound relationship projection for a resolved Card spec.
///
/// The server calls this after it has bound every durable reference to a UID,
/// and offline bundle readers use the same projector to reject altered
/// relationship copies. Inbound relationships remain registry-owned and are
/// intentionally absent from this immutable spec projection.
#[must_use]
pub fn relationships_from_spec(spec: &Spec) -> Relationships {
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

/// Return submission indices sorted by `(kind, space, name, version)`.
///
/// The kind key is its stable wire name. Missing authored spaces sort before
/// named spaces; normal registration requests have a resolved space. The
/// source slice is never mutated, so callers can use the indices to construct
/// a reordered request or to feed a canonical hash input.
#[must_use]
pub fn canonical_order(submissions: &[CardSubmission]) -> Vec<usize> {
    let mut indices = (0..submissions.len()).collect::<Vec<_>>();
    indices.sort_by(|left, right| {
        let left_submission = &submissions[*left];
        let right_submission = &submissions[*right];
        (
            left_submission.kind.wire_name(),
            left_submission
                .metadata
                .space
                .as_ref()
                .map_or("", |space| space.as_str()),
            left_submission.metadata.name.as_str(),
        )
            .cmp(&(
                right_submission.kind.wire_name(),
                right_submission
                    .metadata
                    .space
                    .as_ref()
                    .map_or("", |space| space.as_str()),
                right_submission.metadata.name.as_str(),
            ))
            .then_with(|| {
                left_submission
                    .metadata
                    .resolved_pin()
                    .map_or("", |version| version.as_str())
                    .cmp(
                        right_submission
                            .metadata
                            .resolved_pin()
                            .map_or("", |version| version.as_str()),
                    )
            })
            .then_with(|| left.cmp(right))
    });
    indices
}

#[cfg(test)]
mod tests {
    use super::{canonical_order, relationships_from_spec};
    use crate::api_version::ApiVersion;
    use crate::envelope::{CardKind, Metadata, Spec};
    use crate::registry::CardSubmission;
    use serde_json::json;

    /// Project resolved references into deterministic server-owned relationships.
    #[test]
    fn relationships_project_uid_bearing_refs() {
        let spec = Spec::from_kind_and_value(
            &CardKind::Agent,
            json!({
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

    fn submission(kind: CardKind, space: &str, name: &str) -> CardSubmission {
        CardSubmission {
            api_version: ApiVersion::v1(),
            kind,
            metadata: Metadata {
                name: name.parse().expect("test card name is valid"),
                version: None,
                bump: None,
                space: Some(space.parse().expect("test space is valid")),
                uid: None,
                labels: Default::default(),
                annotations: Default::default(),
                spec_hash: None,
                artifact_hash: None,
                origin: None,
            },
            spec: json!({}),
            artifacts: Vec::new(),
        }
    }

    fn key(submission: &CardSubmission) -> (&str, &str, &str, &str) {
        (
            submission.kind.wire_name(),
            submission
                .metadata
                .space
                .as_ref()
                .map_or("", |space| space.as_str()),
            submission.metadata.name.as_str(),
            submission
                .metadata
                .resolved_pin()
                .map_or("", |version| version.as_str()),
        )
    }

    #[test]
    fn canonical_order_is_deterministic() {
        let submissions = vec![
            submission(CardKind::Service, "prod", "gateway"),
            submission(CardKind::Agent, "shared", "triage"),
            submission(CardKind::Agent, "prod", "triage"),
            submission(CardKind::Prompt, "prod", "system"),
        ];
        let expected = canonical_order(&submissions)
            .into_iter()
            .map(|index| key(&submissions[index]))
            .collect::<Vec<_>>();

        for _ in 0..32 {
            let actual = canonical_order(&submissions)
                .into_iter()
                .map(|index| key(&submissions[index]))
                .collect::<Vec<_>>();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn canonical_order_ignores_wire_order() {
        let entries = [
            submission(CardKind::Service, "prod", "gateway"),
            submission(CardKind::Agent, "shared", "triage"),
            submission(CardKind::Agent, "prod", "triage"),
            submission(CardKind::Prompt, "prod", "system"),
        ];
        let permutations = [vec![0, 1, 2, 3], vec![3, 2, 1, 0], vec![1, 3, 0, 2]];
        let expected = vec![
            ("Agent", "prod", "triage", ""),
            ("Agent", "shared", "triage", ""),
            ("Prompt", "prod", "system", ""),
            ("Service", "prod", "gateway", ""),
        ];

        for permutation in permutations {
            let input = permutation
                .into_iter()
                .map(|index| entries[index].clone())
                .collect::<Vec<_>>();
            let actual = canonical_order(&input)
                .into_iter()
                .map(|index| key(&input[index]))
                .collect::<Vec<_>>();
            assert_eq!(actual, expected);
        }
    }
}
