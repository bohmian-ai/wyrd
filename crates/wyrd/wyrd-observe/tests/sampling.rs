use std::num::NonZeroU32;

use wyrd_observe::{RunId, SamplingPolicy};

#[test]
fn one_of_constructor_rejects_zero() {
    assert!(SamplingPolicy::one_of(0).is_none());
    assert!(SamplingPolicy::one_of(1).is_some());
    assert!(SamplingPolicy::one_of(1_000).is_some());
}

#[test]
fn bucket_constructor_rejects_zero_buckets() {
    assert!(SamplingPolicy::bucket(0, 5).is_none());
    assert!(SamplingPolicy::bucket(100, 0).is_some());
    assert!(SamplingPolicy::bucket(100, 100).is_some());
}

#[test]
fn one_of_n_emits_every_nth_iteration() {
    let policy = SamplingPolicy::one_of(3).expect("3 is non-zero");
    let run_id = RunId::from_string("r1".to_owned());

    assert!(policy.should_sample_iteration(&run_id, 0));
    assert!(!policy.should_sample_iteration(&run_id, 1));
    assert!(!policy.should_sample_iteration(&run_id, 2));
    assert!(policy.should_sample_iteration(&run_id, 3));
    assert!(policy.should_sample_iteration(&run_id, 6));
}

#[test]
fn bucket_keep_zero_drops_all() {
    let policy = SamplingPolicy::bucket(100, 0).expect("100 is non-zero");
    let run_id = RunId::from_string("r1".to_owned());

    assert!(!policy.should_sample_iteration(&run_id, 1));
    assert!(!policy.should_sample_iteration(&run_id, 2));
    assert!(!policy.should_sample_iteration(&run_id, 999));
}

#[test]
fn bucket_keep_equal_buckets_emits_all() {
    let policy = SamplingPolicy::bucket(100, 100).expect("100 is non-zero");
    let run_id = RunId::from_string("r1".to_owned());

    assert!(policy.should_sample_iteration(&run_id, 1));
}

#[test]
fn one_of_one_emits_every_event() {
    let policy = SamplingPolicy::OneOf(NonZeroU32::new(1).expect("1 is non-zero"));
    let run_id = RunId::from_string("r1".to_owned());

    for iteration in 0..10 {
        assert!(policy.should_sample_iteration(&run_id, iteration));
    }
}

#[test]
fn critical_tool_always_samples_regardless_of_policy() {
    let policy = SamplingPolicy::one_of(1_000_000).expect("1_000_000 is non-zero");
    let run_id = RunId::from_string("r1".to_owned());

    assert!(policy.should_sample_tool_call(&run_id, "database_write"));
    assert!(policy.should_sample_tool_call(&run_id, "execute_code"));
}

#[test]
fn run_id_bucket_is_deterministic() {
    let id = RunId::from_string("r1".to_owned());

    assert_eq!(id.hash_bucket(100), id.hash_bucket(100));
}

#[test]
fn run_id_bucket_pins_sha256_value() {
    let id = RunId::from_string("r1".to_owned());

    assert_eq!(id.hash_bucket(100), 41);
}

#[test]
fn run_id_bucket_zero_buckets_returns_zero() {
    let id = RunId::from_string("r1".to_owned());

    assert_eq!(id.hash_bucket(0), 0);
}
