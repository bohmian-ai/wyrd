//! Deterministic submission canonicalization.

use crate::registry::CardSubmission;

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
    use super::canonical_order;
    use crate::api_version::ApiVersion;
    use crate::envelope::{CardKind, Metadata};
    use crate::registry::CardSubmission;
    use serde_json::json;

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
