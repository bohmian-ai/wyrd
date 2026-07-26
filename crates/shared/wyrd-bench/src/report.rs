//! Structured Bifrost benchmark reports and percentile math.

use serde::{Deserialize, Serialize};

use crate::lane::BifrostFaultProfile;

/// One compact Scribe SLO workload declaration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScribeCompactCase {
    /// Stable case identifier.
    pub id: String,
    /// Target frame size used to construct the Arrow IPC payload.
    pub frame_size_bytes: u64,
    /// Concurrent public SDK streams used by the case.
    pub producers: u32,
    /// Requested tenant count.
    pub tenants: u32,
    /// Requested pod count.
    pub pods: u32,
    /// Number of tables receiving frames in the case.
    pub tables: u32,
    /// Table distribution: all streams share one table or are dispersed.
    pub table_distribution: String,
    /// Explicit producer routing mode.
    pub routing_mode: String,
    /// Fsync mode declared by the case.
    pub fsync_mode: String,
    /// Concrete fault seam exercised by this case.
    pub fault_profile: BifrostFaultProfile,
    /// Injected fsync delay in milliseconds.
    pub fsync_delay_ms: u64,
    /// Minimum measured frames after warmup.
    pub minimum_samples: u64,
}

type CompactCaseDefinition = (
    &'static str,
    u64,
    u32,
    u32,
    u32,
    u32,
    &'static str,
    &'static str,
    u64,
    u64,
);

const COMPACT_CASES: [CompactCaseDefinition; 12] = [
    (
        "tiny-w1-t1-p1-tbl1-same",
        1,
        1,
        1,
        1,
        1,
        "same",
        "normal",
        0,
        1_000,
    ),
    (
        "tiny-w64-t1-p1-tbl1-same",
        1,
        64,
        1,
        1,
        1,
        "same",
        "normal",
        0,
        1_000,
    ),
    (
        "64k-w1-t1-p1-tbl1-same",
        64 * 1024,
        1,
        1,
        1,
        1,
        "same",
        "normal",
        0,
        1_000,
    ),
    (
        "64k-w64-t1-p1-tbl1-same",
        64 * 1024,
        64,
        1,
        1,
        1,
        "same",
        "normal",
        0,
        1_000,
    ),
    (
        "64k-w64-t1-p3-tbl1-same",
        64 * 1024,
        64,
        1,
        3,
        1,
        "same",
        "normal",
        0,
        1_000,
    ),
    (
        "64k-w64-t10-p3-tbl1-same",
        64 * 1024,
        64,
        10,
        3,
        1,
        "same",
        "normal",
        0,
        1_000,
    ),
    (
        "64k-w64-t10-p3-tbl8-dispersed",
        64 * 1024,
        64,
        10,
        3,
        8,
        "dispersed",
        "normal",
        0,
        1_000,
    ),
    (
        "64k-w64-t10-p3-tbl1-delayed",
        64 * 1024,
        64,
        10,
        3,
        1,
        "same",
        "delayed_test_only",
        60,
        1_000,
    ),
    (
        "1m-w32-t4-p3-tbl1-same",
        1024 * 1024,
        32,
        4,
        3,
        1,
        "same",
        "normal",
        0,
        256,
    ),
    (
        "8m-w8-t4-p3-tbl8-dispersed",
        8 * 1024 * 1024,
        8,
        4,
        3,
        8,
        "dispersed",
        "normal",
        0,
        64,
    ),
    (
        "32m-w1-t1-p1-tbl1-same",
        32 * 1024 * 1024,
        1,
        1,
        1,
        1,
        "same",
        "normal",
        0,
        16,
    ),
    (
        "32m-w4-t4-p3-tbl4-dispersed",
        32 * 1024 * 1024,
        4,
        4,
        3,
        4,
        "dispersed",
        "delayed_test_only",
        60,
        16,
    ),
];

/// Return the compact Scribe matrix plus explicit fault/stress cases.
#[must_use]
pub fn compact_scribe_matrix() -> Vec<ScribeCompactCase> {
    let mut cases = COMPACT_CASES
        .into_iter()
        .map(
            |(
                id,
                frame_size_bytes,
                producers,
                tenants,
                pods,
                tables,
                table_distribution,
                fsync_mode,
                fsync_delay_ms,
                minimum_samples,
            )| ScribeCompactCase {
                id: id.to_owned(),
                frame_size_bytes,
                producers,
                tenants,
                pods,
                tables,
                table_distribution: table_distribution.to_owned(),
                routing_mode: if table_distribution == "same" {
                    "same_logical_table".to_owned()
                } else {
                    "dispersed_tenant_table_pod".to_owned()
                },
                fsync_mode: fsync_mode.to_owned(),
                fault_profile: if fsync_mode == "delayed_test_only" {
                    BifrostFaultProfile::DelayedFsync
                } else {
                    BifrostFaultProfile::None
                },
                fsync_delay_ms,
                minimum_samples,
            },
        )
        .collect::<Vec<_>>();
    cases.extend([
        fault_case("retry", BifrostFaultProfile::Retry),
        fault_case("memory-pressure", BifrostFaultProfile::MemoryPressure),
        fault_case("wal-pressure", BifrostFaultProfile::WalPressure),
        fault_case("object-store-stall", BifrostFaultProfile::ObjectStoreStall),
        fault_case("postgres-stall", BifrostFaultProfile::PostgresStall),
        fault_case(
            "concurrent-role-memory",
            BifrostFaultProfile::ConcurrentRoleMemory,
        ),
    ]);
    cases
}

fn fault_case(id: &str, fault_profile: BifrostFaultProfile) -> ScribeCompactCase {
    ScribeCompactCase {
        id: id.to_owned(),
        frame_size_bytes: 64 * 1024,
        producers: 4,
        tenants: 1,
        pods: 1,
        tables: 1,
        table_distribution: "same".to_owned(),
        routing_mode: "same_logical_table".to_owned(),
        fsync_mode: "normal".to_owned(),
        fault_profile,
        fsync_delay_ms: 0,
        minimum_samples: 16,
    }
}

/// Select a stable subset of the compact Scribe matrix by comma-separated ID.
pub fn select_compact_scribe_cases(filter: Option<&str>) -> Result<Vec<ScribeCompactCase>, String> {
    let cases = compact_scribe_matrix();
    let Some(filter) = filter.filter(|value| !value.trim().is_empty()) else {
        return Ok(cases);
    };
    let requested = filter
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<std::collections::BTreeSet<_>>();
    if requested.is_empty() {
        return Err("WYRD_BIFROST_CASES must contain at least one case ID".to_owned());
    }
    let known = cases
        .iter()
        .map(|case| case.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let unknown = requested
        .difference(&known)
        .map(|case| (*case).to_owned())
        .collect::<Vec<_>>();
    if !unknown.is_empty() {
        return Err(format!("unknown Scribe benchmark case(s): {unknown:?}"));
    }
    Ok(cases
        .into_iter()
        .filter(|case| requested.contains(case.id.as_str()))
        .collect())
}

/// A distribution captured from a production metric series.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScribeDistribution {
    /// Number of observations.
    pub count: u64,
    /// 50th percentile in microseconds or metric units.
    pub p50: u64,
    /// 95th percentile in microseconds or metric units.
    pub p95: u64,
    /// 99th percentile when at least 100 observations exist.
    pub p99: Option<u64>,
    /// Maximum observed value.
    pub max: u64,
}

/// Component benchmark measurement from a real production seam.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScribeComponentReport {
    /// Stable component name.
    pub name: String,
    /// Production path used by the measurement.
    pub path: String,
    /// Explicit measurement provenance for the production seam.
    pub provenance: String,
    /// Warmup operations excluded from the measured sample.
    pub warmup_operations: u64,
    /// Fixture payload bytes represented by one operation.
    pub fixture_bytes: u64,
    /// Measured operations per second.
    pub operations_per_second: f64,
    /// Measured mebibytes per second.
    pub mib_per_second: f64,
    /// Number of measured operations.
    pub operations: u64,
    /// Processed bytes.
    pub bytes: u64,
    /// Elapsed wall-clock microseconds.
    pub elapsed_us: u64,
    /// Captured distribution.
    pub distribution: ScribeDistribution,
}

/// Exact production seams required by the component closeout report.
#[must_use]
pub fn required_scribe_components() -> [&'static str; 7] {
    [
        "wal_prepare_crc_no_io",
        "vectored_append_no_sync",
        "sync_alone",
        "one_frame_append_sync",
        "64_frame_append_one_sync",
        "gate_ack",
        "sdk_gate_scribe",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_matrix_retains_base_cases_and_explicit_fault_cases() {
        let matrix = compact_scribe_matrix();
        assert_eq!(matrix.len(), 18);
        for profile in [
            BifrostFaultProfile::DelayedFsync,
            BifrostFaultProfile::Retry,
            BifrostFaultProfile::MemoryPressure,
            BifrostFaultProfile::WalPressure,
            BifrostFaultProfile::ObjectStoreStall,
            BifrostFaultProfile::PostgresStall,
            BifrostFaultProfile::ConcurrentRoleMemory,
        ] {
            assert!(matrix.iter().any(|case| case.fault_profile == profile));
        }
    }

    #[test]
    fn case_filter_selects_only_requested_cases() {
        let selected = select_compact_scribe_cases(Some("memory-pressure,tiny-w1-t1-p1-tbl1-same"))
            .expect("requested cases");
        assert_eq!(
            selected
                .iter()
                .map(|case| case.id.as_str())
                .collect::<Vec<_>>(),
            vec!["tiny-w1-t1-p1-tbl1-same", "memory-pressure"]
        );
        assert!(select_compact_scribe_cases(Some("missing-case")).is_err());
    }
}
