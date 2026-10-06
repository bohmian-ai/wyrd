//! Record-admission sampling for continuous Eval.
//!
//! Sampling decides, before any trace wait or task work, whether one committed
//! record is evaluated at all. Every policy is a pure function of durable run
//! inputs, so a retried, reclaimed, or restarted run reaches the same decision.

use serde_json::Value;
use sha2::{Digest, Sha256};
use wyrd_spec::vala::eval::EvalSampling;

use crate::{EvalExecError, extract_required_jsonpath_from};

/// The durable facts one record's sampling decision is derived from.
#[derive(Debug, Clone, Copy)]
pub struct RecordSample<'a> {
    /// Exact producer-owned record identity; seeds the `ratio` draw.
    pub record_id: &'a str,
    /// The record's context; `deterministic_by_hash` reads its key here.
    pub context: &'a Value,
    /// One-based position of this record among its binding's observation runs.
    pub ordinal: u64,
}

impl RecordSample<'_> {
    /// Whether `sampling` selects this record; an absent policy selects every record.
    ///
    /// `ratio` compares a uniform draw derived from the record identity, so it
    /// is probabilistic across records yet stable for one record. `every_nth`
    /// selects ordinals `n, 2n, …`.
    ///
    /// # Errors
    /// Returns [`EvalExecError::JsonPathFailure`] or
    /// [`EvalExecError::ExtractPathMissing`] when a `deterministic_by_hash`
    /// key path fails or resolves to nothing.
    ///
    /// # Panics
    ///
    /// Panics if the `sampling` task id literal fails validation.
    pub fn selected(&self, sampling: Option<&EvalSampling>) -> Result<bool, EvalExecError> {
        Ok(match sampling {
            None => true,
            Some(EvalSampling::Ratio { ratio }) => {
                // Top 53 bits give an exact uniform f64 in [0, 1).
                let draw = (digest(self.record_id.as_bytes()) >> 11) as f64 / (1_u64 << 53) as f64;
                draw < *ratio
            }
            Some(EvalSampling::DeterministicByHash {
                key_path,
                modulus,
                bucket,
            }) => {
                let key = extract_required_jsonpath_from(
                    self.context,
                    key_path,
                    &wyrd_spec::vala::eval::TaskId::new("sampling")
                        .expect("`sampling` is a valid task id literal"),
                )?;
                digest(key.to_string().as_bytes()) % u64::from(*modulus) == u64::from(*bucket)
            }
            Some(EvalSampling::EveryNth { n }) => self.ordinal.is_multiple_of(u64::from(*n)),
        })
    }
}

/// First eight bytes of the SHA-256 of `bytes`, big-endian.
fn digest(bytes: &[u8]) -> u64 {
    let hash = Sha256::digest(bytes);
    u64::from_be_bytes(hash[..8].try_into().expect("a SHA-256 digest has 32 bytes"))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wyrd_spec::vala::eval::{EvalSampling, JsonPath};

    use super::RecordSample;

    /// Each policy is deterministic per record, and a missing hash key errors.
    #[test]
    fn sampling_policies_are_stable_per_record() {
        let context = json!({"user": "u-1"});
        let sample = |record_id, ordinal| RecordSample {
            record_id,
            context: &context,
            ordinal,
        };
        assert!(sample("r", 1).selected(None).expect("no policy"));
        let none = EvalSampling::Ratio { ratio: 0.0 };
        let all = EvalSampling::Ratio { ratio: 1.0 };
        assert!(!sample("r", 1).selected(Some(&none)).expect("ratio"));
        assert!(sample("r", 1).selected(Some(&all)).expect("ratio"));
        let half = EvalSampling::Ratio { ratio: 0.5 };
        let picked = (0..200)
            .filter(|i| {
                let record_id = format!("record-{i}");
                RecordSample {
                    record_id: &record_id,
                    context: &context,
                    ordinal: 1,
                }
                .selected(Some(&half))
                .expect("ratio")
            })
            .count();
        assert!((60..140).contains(&picked), "{picked} of 200 picked at 0.5");

        let every = EvalSampling::EveryNth { n: 3 };
        let chosen: Vec<u64> = (1..=7)
            .filter(|ordinal| sample("r", *ordinal).selected(Some(&every)).expect("nth"))
            .collect();
        assert_eq!(chosen, vec![3, 6]);

        let by_hash = |path: &str| EvalSampling::DeterministicByHash {
            key_path: JsonPath::new(path).expect("valid path"),
            modulus: 1,
            bucket: 0,
        };
        assert!(
            sample("r", 1)
                .selected(Some(&by_hash("$.user")))
                .expect("hash")
        );
        assert!(sample("r", 1).selected(Some(&by_hash("$.absent"))).is_err());
    }
}
