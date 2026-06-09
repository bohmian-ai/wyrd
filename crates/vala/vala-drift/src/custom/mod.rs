//! Custom drift scoring. Body lands in Commit 9.

use wyrd_spec::card::drift::CustomProfile;

use crate::error::DriftScoreError;
use crate::report::DriftReport;

pub fn score_custom(
    _target: &arrow::record_batch::RecordBatch,
    _profile: &CustomProfile,
) -> Result<DriftReport, DriftScoreError> {
    unimplemented!("Commit 9")
}
