//! SPC parity: pins X-bar/S control-limit math to a hand-computed value.
//!
//! If `fit_control_limits` ever drifts away from the mean of chunk means plus
//! mean of chunk stddevs divided by c4, this test fails.

use std::sync::Arc;

use arrow::array::Float64Array;
use arrow::record_batch::RecordBatch;
use arrow_schema::{DataType, Field, Schema};
use vala_drift::fit_spc_baseline;
use wyrd_spec::card::drift::{SpcAlertThreshold, SpcProfile, SpcWecoRule};
use wyrd_spec::ids::FeatureName;

#[test]
fn x_bar_s_parity_with_hand_computation() {
    let values = vec![1.0, 3.0, 2.0, 4.0, 3.0, 5.0, 4.0, 6.0];
    let batch = RecordBatch::try_new(
        Arc::new(Schema::new(vec![Field::new("x", DataType::Float64, true)])),
        vec![Arc::new(Float64Array::from(values))],
    )
    .expect("record batch");

    let profile = SpcProfile {
        sample_size: 2,
        weco_rule: SpcWecoRule::default(),
        alert_threshold: SpcAlertThreshold::Zone4,
    };
    let fname = FeatureName::new("x").expect("valid feature name");

    let baseline = fit_spc_baseline(&batch, &profile, &[fname.clone()]).expect("baseline");

    let fitted = baseline.features.get(&fname).expect("feature");
    let expected_stddev_adj = (2.0_f64).sqrt() / 0.8;
    assert!((fitted.center - 3.5).abs() < 1e-12);
    assert!((fitted.one_ucl - (3.5 + expected_stddev_adj)).abs() < 1e-9);
    assert!((fitted.one_lcl - (3.5 - expected_stddev_adj)).abs() < 1e-9);
    assert!((fitted.three_ucl - (3.5 + 3.0 * expected_stddev_adj)).abs() < 1e-9);
    assert!((fitted.three_lcl - (3.5 - 3.0 * expected_stddev_adj)).abs() < 1e-9);
}
