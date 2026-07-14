//! Seal filename generation — PR#5.
//!
//! `seal_filename` produces unique Parquet filenames in the form `{pod_id}-{ulid}.parquet`
//! so multi-pod seals into the same partition never collide.

use wyrd_spec::ids::PodId;

/// Generate a unique seal filename: `{pod_id}-{ulid}.parquet`.
///
/// ULID monotonicity guarantees no collision within one pod; `pod_id` prefix
/// guarantees no collision across pods.
pub fn seal_filename(pod_id: &PodId) -> String {
    let ulid = ulid::Ulid::new();
    format!("{}-{}.parquet", pod_id.as_str(), ulid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn filename_never_collides_same_pod() {
        // 10k calls to seal_filename(pod_A) produce 10k distinct filenames
        let pod_a = PodId::new("pod-a".to_string()).unwrap();
        let mut seen = HashSet::new();

        for _ in 0..10_000 {
            let filename = seal_filename(&pod_a);
            assert!(
                seen.insert(filename.clone()),
                "filename collision detected: {filename}"
            );
        }

        assert_eq!(seen.len(), 10_000);
    }

    #[test]
    fn filename_never_collides_across_pods() {
        // 3 different pod_ids calling seal_filename within the same wall-clock millisecond
        // produce 3 distinct filenames
        let pod_a = PodId::new("pod-a".to_string()).unwrap();
        let pod_b = PodId::new("pod-b".to_string()).unwrap();
        let pod_c = PodId::new("pod-c".to_string()).unwrap();

        let mut filenames = vec![
            seal_filename(&pod_a),
            seal_filename(&pod_b),
            seal_filename(&pod_c),
        ];

        filenames.sort();
        filenames.dedup();

        assert_eq!(
            filenames.len(),
            3,
            "cross-pod collision: fewer than 3 distinct filenames"
        );
    }

    #[test]
    fn filename_format_matches_expected_pattern() {
        let pod = PodId::new("test-pod".to_string()).unwrap();
        let filename = seal_filename(&pod);

        // Expected format: {pod_id}-{ulid}.parquet
        assert!(filename.starts_with("test-pod-"));
        assert!(filename.ends_with(".parquet"));

        // ULID portion should be 26 characters (uppercase base32)
        let ulid_part = filename
            .strip_prefix("test-pod-")
            .unwrap()
            .strip_suffix(".parquet")
            .unwrap();
        assert_eq!(ulid_part.len(), 26);
    }
}
