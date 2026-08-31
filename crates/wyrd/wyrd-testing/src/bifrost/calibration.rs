//! Oracle calibration evidence and candidate-profile completeness contracts.

use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

/// Serialize a report exactly as it is persisted for content-addressed linkage.
///
/// # Errors
///
/// Returns an error when the report cannot be encoded as JSON.
pub(crate) fn calibration_report_bytes(
    report: &OracleCalibrationReport,
) -> Result<Vec<u8>, serde_json::Error> {
    let mut bytes = serde_json::to_vec_pretty(report)?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Compute the stable non-secret digest used to cross-link calibration artifacts.
#[must_use]
pub(crate) fn calibration_content_digest(bytes: &[u8]) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    format!("sip64:{:016x}", hasher.finish())
}

/// Exact numeric proposal paths required before a candidate can be reviewed.
pub const REQUIRED_PROPOSALS: &[&str] = &[
    "slot.cpu_cores_per_slot",
    "slot.headroom_factor",
    "class.interactive.share",
    "class.interactive.minimum_slots",
    "class.analytical.share",
    "class.analytical.minimum_slots",
    "tenant.single_tenant_limit",
    "tenant.multi_tenant_default_limit",
    "classification.assumed_scan_bytes_per_second",
    "classification.analytical_threshold_millis",
    "placement.max_attempts",
    "placement.deadline_millis",
    "placement.jitter_min_millis",
    "placement.jitter_max_millis",
    "lease.cluster_ttl_seconds",
    "lease.renew_interval_seconds",
    "reservation.pending_ttl_seconds",
    "membership.expiration_seconds",
    "tail.fence_ttl_seconds",
    "tail.page_rows",
    "tail.page_encoded_bytes",
    "distribution.max_workers_per_query",
    "distribution.fragment_target_rows",
    "distribution.fragment_target_bytes",
    "distribution.max_fragment_bytes",
    "distribution.max_frame_bytes",
    "distribution.max_in_flight_fragments",
    "distribution.max_worker_concurrency",
    "performance.p95_query_millis",
    "performance.p99_query_millis",
    "performance.p95_ttfb_millis",
    "performance.p99_ttfb_millis",
    "performance.minimum_rows_per_second",
    "performance.last_stable_concurrency",
    "performance.maximum_tail_page_millis",
    "performance.maximum_object_store_throttle_rate",
];

/// Reproducible environment attached to one calibration report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleCalibrationEnvironment {
    /// Source revision under measurement.
    pub source_revision: String,
    /// Operating-system and architecture description.
    pub platform: String,
    /// Available logical CPU count.
    pub logical_cpus: u32,
    /// Rust runtime/compiler identity supplied by the runner.
    pub runtime: String,
}

/// Machine-readable measurements emitted by the calibration runner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleCalibrationReport {
    /// Report schema version.
    pub schema_version: u32,
    /// Reproducible host and revision metadata.
    pub environment: OracleCalibrationEnvironment,
    /// Stable workload content hashes.
    pub workload_hashes: BTreeMap<String, String>,
    /// Deterministic random seeds used by the matrix.
    pub seeds: Vec<u64>,
    /// Warmup window in completed queries.
    pub warmup_queries: u32,
    /// Measurement window in completed queries.
    pub measurement_queries: u32,
    /// Every measured topology/workload case.
    pub cases: Vec<OracleCalibrationCase>,
}

/// One measured calibration matrix case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleCalibrationCase {
    /// Stable case identifier used by candidate evidence links.
    pub case_id: String,
    /// Pod count (1, 3, or 6).
    pub pods: u8,
    /// `published_only` or `fused`.
    pub visibility: String,
    /// `interactive` or `analytical`.
    pub query_class: String,
    /// Number of isolated tenants active in the workload.
    pub tenants: u32,
    /// Completed untimed public-client warmup queries.
    pub warmup_queries: u32,
    /// Completed measured public-client queries.
    pub measurement_queries: u32,
    /// Whether at least one fragment executed remotely.
    pub distributed: bool,
    /// Query concurrency used by this case.
    pub concurrency: u32,
    /// Deterministic input row count.
    pub input_rows: u64,
    /// Production source-byte counter delta for the measured case.
    pub input_bytes: u64,
    /// Exact result row count/digest verification.
    pub correctness_verified: bool,
    /// Negative-flow set completed successfully.
    pub negative_flows_verified: bool,
    /// A validated terminal was observed for every successful query.
    pub terminal_verified: bool,
    /// Durable read-decision audits advanced for every measured query.
    pub audit_verified: bool,
    /// p50 query latency in milliseconds.
    pub p50_ms: f64,
    /// p95 query latency in milliseconds.
    pub p95_ms: f64,
    /// p99 query latency in milliseconds.
    pub p99_ms: f64,
    /// p95 time-to-first-batch in milliseconds.
    pub p95_ttfb_ms: f64,
    /// p99 time-to-first-batch in milliseconds.
    pub p99_ttfb_ms: f64,
    /// Rows completed per second.
    pub rows_per_second: f64,
    /// Peak Oracle-owned memory observed in bytes.
    pub peak_memory_bytes: u64,
    /// Spill bytes observed in the production recorder.
    pub spill_bytes: u64,
    /// Process CPU seconds consumed during the case.
    pub cpu_seconds: f64,
    /// Object-store read time in milliseconds.
    pub object_store_ms: f64,
    /// Remote/local tail page time in milliseconds.
    pub tail_ms: f64,
    /// Maximum concurrent slots observed.
    pub peak_slots: u32,
    /// Rejected-query fraction.
    pub rejection_rate: f64,
    /// Retried-attempt fraction.
    pub retry_rate: f64,
    /// Cancellation flow completed and cleaned up.
    pub cancellation_verified: bool,
}

/// One explicit candidate value linked to measured evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleProposalEvidence {
    /// Proposed numeric value; never silently defaulted by validation.
    pub value: f64,
    /// Calibration case that supports this value.
    pub evidence_case_id: String,
}

/// Candidate profile deliberately remains unapproved until maintainer review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleCalibrationProfile {
    /// Profile schema version.
    pub schema_version: u32,
    /// Must remain `candidate` for generated artifacts.
    pub status: String,
    /// Digest of the exact source report bytes.
    pub generated_from: String,
    /// Source revision copied from the report environment.
    pub source_revision: String,
    /// Explicit measured proposals keyed by architecture field path.
    pub proposal: BTreeMap<String, OracleProposalEvidence>,
}

impl OracleCalibrationReport {
    /// Reject incomplete, unsafe, or non-reproducible report matrices.
    ///
    /// # Errors
    ///
    /// Returns a stable explanation for missing metadata, dimensions, safety
    /// bounds, correctness, negative flows, terminals, or measurements.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1
            || self.environment.source_revision.is_empty()
            || self.environment.platform.is_empty()
            || self.environment.runtime.is_empty()
            || self.environment.logical_cpus == 0
            || self.workload_hashes.is_empty()
            || self.seeds.is_empty()
            || self.measurement_queries == 0
        {
            return Err("calibration reproducibility metadata is incomplete".to_owned());
        }
        let ids = self
            .cases
            .iter()
            .map(|case| case.case_id.as_str())
            .collect::<BTreeSet<_>>();
        if ids.len() != self.cases.len() {
            return Err("calibration case IDs must be unique".to_owned());
        }
        for pods in [1_u8, 3, 6] {
            for visibility in ["published_only", "fused"] {
                for query_class in ["interactive", "analytical"] {
                    if !self.cases.iter().any(|case| {
                        case.pods == pods
                            && case.visibility == visibility
                            && case.query_class == query_class
                    }) {
                        return Err(format!(
                            "calibration matrix missing pods={pods} visibility={visibility} class={query_class}"
                        ));
                    }
                }
            }
        }
        for case in &self.cases {
            if !matches!(case.pods, 1 | 3 | 6)
                || case.tenants == 0
                || case.warmup_queries == 0
                || case.measurement_queries < 20
                || case.concurrency == 0
                || case.input_rows == 0
                || case.input_bytes == 0
                || !case.correctness_verified
                || !case.negative_flows_verified
                || !case.terminal_verified
                || !case.audit_verified
                || !case.cancellation_verified
                || (case.pods > 1 && !case.distributed)
                || !(case.p50_ms.is_finite()
                    && case.p95_ms.is_finite()
                    && case.p99_ms.is_finite()
                    && case.p95_ttfb_ms.is_finite()
                    && case.p99_ttfb_ms.is_finite()
                    && case.rows_per_second.is_finite()
                    && case.cpu_seconds.is_finite()
                    && case.object_store_ms.is_finite()
                    && case.tail_ms.is_finite())
                || case.rows_per_second <= 0.0
                || case.peak_memory_bytes == 0
                || case.peak_slots == 0
                || case.cpu_seconds < 0.0
                || case.p50_ms > case.p95_ms
                || case.p95_ms > case.p99_ms
                || case.p95_ttfb_ms > case.p99_ttfb_ms
                || !(0.0..=1.0).contains(&case.rejection_rate)
                || !(0.0..=1.0).contains(&case.retry_rate)
            {
                return Err(format!("calibration case {} is incomplete", case.case_id));
            }
        }
        Ok(())
    }
}

impl OracleCalibrationProfile {
    /// Derives the deterministic scheduling/performance candidate from measured evidence.
    ///
    /// Resource-allocation proposals are intentionally absent because the
    /// Bifrost resource governor derives capacity from the running process.
    ///
    /// # Errors
    ///
    /// Returns an error when the report is invalid or contains no cases.
    pub fn from_report(report: &OracleCalibrationReport) -> Result<Self, String> {
        report.validate()?;
        let report_bytes = calibration_report_bytes(report).map_err(|error| error.to_string())?;
        let evidence = report
            .cases
            .iter()
            .max_by(|left, right| left.p99_ms.total_cmp(&right.p99_ms))
            .ok_or_else(|| "calibration matrix is empty".to_owned())?;
        let proposal = REQUIRED_PROPOSALS
            .iter()
            .map(|key| {
                (
                    (*key).to_owned(),
                    OracleProposalEvidence {
                        value: proposal_value(key, evidence),
                        evidence_case_id: evidence.case_id.clone(),
                    },
                )
            })
            .collect();
        let profile = Self {
            schema_version: 1,
            status: "candidate".to_owned(),
            generated_from: calibration_content_digest(&report_bytes),
            source_revision: report.environment.source_revision.clone(),
            proposal,
        };
        profile.validate(report)?;
        Ok(profile)
    }

    /// Renders the canonical checked-in TOML representation.
    #[must_use]
    pub fn render_toml(&self) -> String {
        let mut output = format!(
            "schema_version = {}\nstatus = {:?}\ngenerated_from = {:?}\nsource_revision = {:?}\n",
            self.schema_version, self.status, self.generated_from, self.source_revision
        );
        for (key, value) in &self.proposal {
            output.push_str(&format!(
                "\n[proposal.{key:?}]\nvalue = {}\nevidence_case_id = {:?}\n",
                value.value, value.evidence_case_id
            ));
        }
        output
    }

    /// Validate candidate status and every evidence-linked proposal value.
    ///
    /// # Errors
    ///
    /// Returns an error when activation status, digest/revision, a required
    /// proposal, numeric value, or evidence case is absent or unsafe.
    pub fn validate(&self, report: &OracleCalibrationReport) -> Result<(), String> {
        report.validate()?;
        let report_bytes = calibration_report_bytes(report).map_err(|error| error.to_string())?;
        if self.schema_version != 1
            || self.status != "candidate"
            || self.generated_from != calibration_content_digest(&report_bytes)
            || self.source_revision != report.environment.source_revision
        {
            return Err("profile must be schema 1 candidate linked to its report".to_owned());
        }
        let case_ids = report
            .cases
            .iter()
            .map(|case| case.case_id.as_str())
            .collect::<BTreeSet<_>>();
        for key in REQUIRED_PROPOSALS {
            let evidence = self
                .proposal
                .get(*key)
                .ok_or_else(|| format!("candidate proposal missing {key}"))?;
            if !evidence.value.is_finite()
                || evidence.value < 0.0
                || !case_ids.contains(evidence.evidence_case_id.as_str())
            {
                return Err(format!("candidate proposal {key} has invalid evidence"));
            }
        }
        if self.proposal["distribution.max_workers_per_query"].value > 63.0 {
            return Err("distribution.max_workers_per_query exceeds 63".to_owned());
        }
        Ok(())
    }
}

/// Returns one measurement-derived scheduling or performance proposal.
fn proposal_value(key: &str, evidence: &OracleCalibrationCase) -> f64 {
    let measured_queries = f64::from(evidence.measurement_queries);
    let peak_slots = f64::from(evidence.peak_slots);
    let concurrency = f64::from(evidence.concurrency);
    let per_query_bytes = evidence.input_bytes as f64 / measured_queries;
    let bytes_per_row = per_query_bytes / evidence.input_rows as f64;
    let workers = f64::from(evidence.pods.saturating_sub(1));
    let observed_query_seconds = evidence.p99_ms * measured_queries / 1_000.0;
    match key {
        "slot.cpu_cores_per_slot" => evidence.cpu_seconds / observed_query_seconds / peak_slots,
        "slot.headroom_factor" | "class.interactive.share" | "class.analytical.share" => {
            peak_slots / concurrency
        }
        "class.interactive.minimum_slots"
        | "class.analytical.minimum_slots"
        | "tenant.single_tenant_limit"
        | "tenant.multi_tenant_default_limit"
        | "distribution.max_in_flight_fragments"
        | "distribution.max_worker_concurrency" => peak_slots,
        "classification.assumed_scan_bytes_per_second" => evidence.rows_per_second * bytes_per_row,
        "classification.analytical_threshold_millis" | "performance.p95_query_millis" => {
            evidence.p95_ms
        }
        "placement.max_attempts" => {
            evidence.retry_rate * measured_queries + evidence.rejection_rate
        }
        "placement.deadline_millis" | "performance.p99_query_millis" => evidence.p99_ms,
        "placement.jitter_min_millis" => evidence.p95_ms - evidence.p50_ms,
        "placement.jitter_max_millis" => evidence.p99_ms - evidence.p50_ms,
        "lease.cluster_ttl_seconds"
        | "membership.expiration_seconds"
        | "tail.fence_ttl_seconds" => evidence.p99_ms / 1_000.0,
        "lease.renew_interval_seconds" | "reservation.pending_ttl_seconds" => {
            evidence.p95_ms / 1_000.0
        }
        "tail.page_rows" => evidence.input_rows as f64,
        "tail.page_encoded_bytes" | "distribution.max_frame_bytes" => per_query_bytes,
        "distribution.max_workers_per_query" => workers,
        "distribution.fragment_target_rows" => evidence.input_rows as f64 / evidence.pods as f64,
        "distribution.fragment_target_bytes" | "distribution.max_fragment_bytes" => {
            per_query_bytes / f64::from(evidence.pods)
        }
        "performance.p95_ttfb_millis" => evidence.p95_ttfb_ms,
        "performance.p99_ttfb_millis" => evidence.p99_ttfb_ms,
        "performance.minimum_rows_per_second" => evidence.rows_per_second,
        "performance.last_stable_concurrency" => peak_slots,
        "performance.maximum_tail_page_millis" => evidence.tail_ms,
        "performance.maximum_object_store_throttle_rate" => evidence.retry_rate,
        _ => peak_slots,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one complete, valid calibration matrix from fixed evidence.
    ///
    /// Every dimension `OracleCalibrationReport::validate` requires is present:
    /// the three pod counts crossed with both visibilities and both query
    /// classes, each case carrying the verified correctness, negative-flow,
    /// terminal, audit, and cancellation flags plus ordered latency
    /// percentiles. The numbers are fixed rather than measured because this
    /// fixture proves how a report is *turned into* a candidate, not what the
    /// hardware did; the measured matrix is produced by the Oracle
    /// qualification bench and is not a checked-in source artifact.
    fn complete_calibration_report() -> OracleCalibrationReport {
        let mut cases = Vec::new();
        for pods in [1_u8, 3, 6] {
            for visibility in ["published_only", "fused"] {
                for query_class in ["interactive", "analytical"] {
                    cases.push(OracleCalibrationCase {
                        case_id: format!("{pods}-{visibility}-{query_class}"),
                        pods,
                        visibility: visibility.to_owned(),
                        query_class: query_class.to_owned(),
                        tenants: 2,
                        warmup_queries: 5,
                        measurement_queries: 20,
                        distributed: pods > 1,
                        concurrency: 4,
                        input_rows: 1_000,
                        input_bytes: 64 * 1024,
                        correctness_verified: true,
                        negative_flows_verified: true,
                        terminal_verified: true,
                        audit_verified: true,
                        p50_ms: 10.0,
                        p95_ms: 20.0,
                        p99_ms: f64::from(pods) * 30.0,
                        p95_ttfb_ms: 5.0,
                        p99_ttfb_ms: 9.0,
                        rows_per_second: 5_000.0,
                        peak_memory_bytes: 8 * 1024 * 1024,
                        spill_bytes: 0,
                        cpu_seconds: 1.5,
                        object_store_ms: 12.0,
                        tail_ms: 3.0,
                        peak_slots: 4,
                        rejection_rate: 0.0,
                        retry_rate: 0.0,
                        cancellation_verified: true,
                    });
                }
            }
        }
        OracleCalibrationReport {
            schema_version: 1,
            environment: OracleCalibrationEnvironment {
                source_revision: "fixture-revision".to_owned(),
                platform: "fixture-platform".to_owned(),
                logical_cpus: 8,
                runtime: "fixture-runtime".to_owned(),
            },
            workload_hashes: BTreeMap::from([("rows".to_owned(), "digest".to_owned())]),
            seeds: vec![7],
            warmup_queries: 5,
            measurement_queries: 20,
            cases,
        }
    }

    /// Keeps a derived candidate free of allocator policy and linked to its evidence.
    ///
    /// Resource allocation is the running process's decision, so a calibration
    /// candidate must never propose slot memory, an Oracle memory limit, class
    /// limits, or a spill limit no matter what the matrix measured. The
    /// candidate must also stay traceable: every proposal it does make names
    /// the case it came from, and that case must be the worst-observed p99 in
    /// the matrix, which is the only case whose numbers are safe to generalize.
    #[test]
    fn oracle_candidate_excludes_allocator_proposals_and_preserves_observations() {
        let report = complete_calibration_report();
        report.validate().expect("fixture matrix is complete");
        let candidate = OracleCalibrationProfile::from_report(&report)
            .expect("measured report must derive a candidate");
        for removed in [
            "slot.memory_bytes_per_slot",
            "memory.oracle_limit_bytes",
            "memory.class_limits",
            "spill.limit_bytes",
        ] {
            assert!(
                !candidate.proposal.contains_key(removed),
                "allocator policy {removed} must never enter a calibration candidate"
            );
        }
        assert!(
            !candidate.proposal.is_empty(),
            "a valid matrix must yield at least one measured proposal"
        );
        let worst = report
            .cases
            .iter()
            .max_by(|left, right| left.p99_ms.total_cmp(&right.p99_ms))
            .expect("fixture matrix is non-empty");
        for (key, evidence) in &candidate.proposal {
            assert_eq!(
                evidence.evidence_case_id, worst.case_id,
                "proposal {key} must cite the worst-observed case"
            );
        }
        candidate
            .validate(&report)
            .expect("a derived candidate must validate against its own report");
        assert_eq!(candidate.status, "candidate");
        assert_eq!(
            candidate.source_revision,
            report.environment.source_revision
        );
    }

    /// Incomplete topology/workload matrices fail before profile generation.
    #[test]
    fn oracle_calibration_report_requires_complete_matrix() {
        let report = OracleCalibrationReport {
            schema_version: 1,
            environment: OracleCalibrationEnvironment {
                source_revision: "test".to_owned(),
                platform: "test".to_owned(),
                logical_cpus: 1,
                runtime: "test".to_owned(),
            },
            workload_hashes: BTreeMap::from([("rows".to_owned(), "digest".to_owned())]),
            seeds: vec![7],
            warmup_queries: 0,
            measurement_queries: 1,
            cases: Vec::new(),
        };
        assert!(report.validate().is_err());
    }

    /// Generated evidence cannot claim production activation.
    #[test]
    fn oracle_candidate_profile_cannot_activate_production() {
        let profile = OracleCalibrationProfile {
            schema_version: 1,
            status: "approved".to_owned(),
            generated_from: "digest".to_owned(),
            source_revision: "test".to_owned(),
            proposal: BTreeMap::new(),
        };
        let report = OracleCalibrationReport {
            schema_version: 1,
            environment: OracleCalibrationEnvironment {
                source_revision: "test".to_owned(),
                platform: "test".to_owned(),
                logical_cpus: 1,
                runtime: "test".to_owned(),
            },
            workload_hashes: BTreeMap::from([("rows".to_owned(), "digest".to_owned())]),
            seeds: vec![7],
            warmup_queries: 0,
            measurement_queries: 1,
            cases: Vec::new(),
        };
        assert!(profile.validate(&report).is_err());
    }
}
