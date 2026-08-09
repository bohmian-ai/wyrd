"""Behavioral tests for ``validate-bifrost-qualification.py``.

Each test assembles a manifest in a temporary directory, copies one or more of
the checked-in report fixtures beside it, records their recomputed SHA-256
digests, and runs the validator as a subprocess. The suite covers a passing
pre-review manifest, a passing sealed manifest, and every rejection the
validator is required to surface, asserting on the ``FAIL <field>`` diagnostics.

The module exposes only top-level ``def test_*`` functions per repository test
rules; a module-level ``load_tests`` wraps each into a ``unittest`` case so the
stdlib runner collects them without a ``TestCase`` subclass.
"""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
VALIDATOR = HERE.parent / "validate-bifrost-qualification.py"
FIXTURES = HERE / "fixtures" / "qualification"
VALID_REPORT = FIXTURES / "reports" / "valid-report.json"

COMMIT = "0123456789abcdef0123456789abcdef01234567"
DATASET = "1" * 64
WORKLOAD = "2" * 64
REVIEW_SHA = "4" * 64
EVIDENCE = "89abcdef" * 5
TOPOLOGY = {"topology_id": "topo-1", "oracle_pods": 1}


def _write_manifest(tmpdir, report_sources, *, sealed=False, mutate=None):
    """Assemble a manifest in ``tmpdir`` linking copies of ``report_sources``.

    Copies each source report beside the manifest, records its recomputed
    digest, fills a passing manifest skeleton, optionally seals it, applies an
    optional ``mutate`` callback to inject a defect, and returns the written
    manifest path.
    """
    root = Path(tmpdir)
    families = ["family-a", "family-b", "family-c", "family-d"]
    reports = []
    for family, source in zip(families, report_sources):
        data = Path(source).read_bytes()
        destination = root / f"{family}.json"
        destination.write_bytes(data)
        report_topology = json.loads(data).get("topology")
        reports.append(
            {
                "family": family,
                "path": destination.name,
                "digest": hashlib.sha256(data).hexdigest(),
                "topology": report_topology,
            }
        )
    manifest = {
        "schema_version": "wyrd.bifrost.qualification/v1",
        "qualified_source_commit": COMMIT,
        "source_tree_clean": True,
        "oracle_query_accounting": "pod_local_v1",
        "dataset_digest": DATASET,
        "workload_digest": WORKLOAD,
        "topology": TOPOLOGY,
        "budgets": {"setup": 1800, "total": 10800, "disk_bytes": 34359738368},
        "actual_durations": {
            "setup_seconds": 100,
            "total_seconds": 200,
            "disk_bytes_used": 1000,
        },
        "verdicts": {
            "correctness_passed": True,
            "saturation_reached": False,
            "recovery_recovered": True,
        },
        "reports": reports,
    }
    if sealed:
        (root / "review.md").write_text("review")
        manifest["review"] = {"path": "review.md", "sha256": REVIEW_SHA}
        manifest["evidence_commit"] = EVIDENCE
    if mutate is not None:
        mutate(manifest)
    manifest_path = root / "qualification-manifest.json"
    manifest_path.write_text(json.dumps(manifest))
    return manifest_path


def _run(manifest_path, phase):
    """Run the validator as a subprocess and return ``(returncode, stderr)``."""
    result = subprocess.run(
        [sys.executable, str(VALIDATOR), str(manifest_path), "--phase", phase],
        capture_output=True,
        text=True,
    )
    return result.returncode, result.stderr


def _valid_sources():
    """Return four valid report sources for a passing manifest."""
    return [VALID_REPORT] * 4


def test_pre_review_manifest_passes():
    """A clean pre-review manifest with four valid reports exits 0."""
    with tempfile.TemporaryDirectory() as tmp:
        manifest = _write_manifest(tmp, _valid_sources())
        code, stderr = _run(manifest, "pre-review")
        assert code == 0, stderr


def test_sealed_manifest_passes():
    """A sealed manifest with a review reference and evidence commit exits 0."""
    with tempfile.TemporaryDirectory() as tmp:
        manifest = _write_manifest(tmp, _valid_sources(), sealed=True)
        code, stderr = _run(manifest, "sealed")
        assert code == 0, stderr


def test_pre_review_rejects_present_review():
    """A review reference present in pre-review is rejected."""
    def mutate(manifest):
        manifest["review"] = {"path": "review.md", "sha256": REVIEW_SHA}

    with tempfile.TemporaryDirectory() as tmp:
        manifest = _write_manifest(tmp, _valid_sources(), mutate=mutate)
        code, stderr = _run(manifest, "pre-review")
        assert code == 1
        assert "FAIL review:" in stderr


def test_pre_review_rejects_present_evidence_commit():
    """An evidence commit present in pre-review is rejected."""
    def mutate(manifest):
        manifest["evidence_commit"] = EVIDENCE

    with tempfile.TemporaryDirectory() as tmp:
        manifest = _write_manifest(tmp, _valid_sources(), mutate=mutate)
        code, stderr = _run(manifest, "pre-review")
        assert code == 1
        assert "FAIL evidence_commit:" in stderr


def test_sealed_rejects_missing_review():
    """A sealed manifest without a review reference is rejected."""
    with tempfile.TemporaryDirectory() as tmp:
        manifest = _write_manifest(tmp, _valid_sources())
        code, stderr = _run(manifest, "sealed")
        assert code == 1
        assert "FAIL review:" in stderr
        assert "FAIL evidence_commit:" in stderr


def test_sealed_rejects_bad_review_digest():
    """A sealed manifest whose review digest is not 64-hex is rejected."""
    def mutate(manifest):
        manifest["review"]["sha256"] = "deadbeef"

    with tempfile.TemporaryDirectory() as tmp:
        manifest = _write_manifest(tmp, _valid_sources(), sealed=True, mutate=mutate)
        code, stderr = _run(manifest, "sealed")
        assert code == 1
        assert "FAIL review.sha256:" in stderr


def test_rejects_bad_schema_version():
    """A manifest with the wrong schema version is rejected."""
    def mutate(manifest):
        manifest["schema_version"] = "wyrd.bifrost.capacity/v1"

    with tempfile.TemporaryDirectory() as tmp:
        manifest = _write_manifest(tmp, _valid_sources(), mutate=mutate)
        code, stderr = _run(manifest, "pre-review")
        assert code == 1
        assert "FAIL schema_version:" in stderr


def test_rejects_short_source_commit():
    """A source commit that is not 40-hex is rejected."""
    def mutate(manifest):
        manifest["qualified_source_commit"] = "abc123"

    with tempfile.TemporaryDirectory() as tmp:
        manifest = _write_manifest(tmp, _valid_sources(), mutate=mutate)
        code, stderr = _run(manifest, "pre-review")
        assert code == 1
        assert "FAIL qualified_source_commit:" in stderr


def test_rejects_dirty_source_tree():
    """A manifest recording an unclean source tree is rejected."""
    def mutate(manifest):
        manifest["source_tree_clean"] = False

    with tempfile.TemporaryDirectory() as tmp:
        manifest = _write_manifest(tmp, _valid_sources(), mutate=mutate)
        code, stderr = _run(manifest, "pre-review")
        assert code == 1
        assert "FAIL source_tree_clean:" in stderr


def test_rejects_wrong_accounting_marker():
    """A manifest whose accounting marker is not pod_local_v1 is rejected."""
    def mutate(manifest):
        manifest["oracle_query_accounting"] = "global_v1"

    with tempfile.TemporaryDirectory() as tmp:
        manifest = _write_manifest(tmp, _valid_sources(), mutate=mutate)
        code, stderr = _run(manifest, "pre-review")
        assert code == 1
        assert "FAIL oracle_query_accounting:" in stderr


def test_rejects_setup_budget_breach():
    """Actual setup duration above the setup budget is rejected."""
    def mutate(manifest):
        manifest["actual_durations"]["setup_seconds"] = 999999

    with tempfile.TemporaryDirectory() as tmp:
        manifest = _write_manifest(tmp, _valid_sources(), mutate=mutate)
        code, stderr = _run(manifest, "pre-review")
        assert code == 1
        assert "FAIL budget.setup:" in stderr


def test_rejects_disk_budget_breach():
    """Actual disk usage above the disk budget is rejected."""
    def mutate(manifest):
        manifest["actual_durations"]["disk_bytes_used"] = 99999999999999

    with tempfile.TemporaryDirectory() as tmp:
        manifest = _write_manifest(tmp, _valid_sources(), mutate=mutate)
        code, stderr = _run(manifest, "pre-review")
        assert code == 1
        assert "FAIL budget.disk:" in stderr


def test_rejects_failed_correctness_verdict():
    """A rolled-up correctness verdict that is not True is rejected."""
    def mutate(manifest):
        manifest["verdicts"]["correctness_passed"] = False

    with tempfile.TemporaryDirectory() as tmp:
        manifest = _write_manifest(tmp, _valid_sources(), mutate=mutate)
        code, stderr = _run(manifest, "pre-review")
        assert code == 1
        assert "FAIL verdicts.correctness_passed:" in stderr


def test_rejects_unrecovered_recovery_verdict():
    """A rolled-up recovery verdict that is not True is rejected."""
    def mutate(manifest):
        manifest["verdicts"]["recovery_recovered"] = False

    with tempfile.TemporaryDirectory() as tmp:
        manifest = _write_manifest(tmp, _valid_sources(), mutate=mutate)
        code, stderr = _run(manifest, "pre-review")
        assert code == 1
        assert "FAIL verdicts.recovery_recovered:" in stderr


def test_rejects_wrong_report_count():
    """A manifest linking fewer than four reports is rejected."""
    def mutate(manifest):
        manifest["reports"] = manifest["reports"][:3]

    with tempfile.TemporaryDirectory() as tmp:
        manifest = _write_manifest(tmp, _valid_sources(), mutate=mutate)
        code, stderr = _run(manifest, "pre-review")
        assert code == 1
        assert "FAIL reports:" in stderr


def test_rejects_digest_mismatch():
    """A recorded report digest that no longer matches its file is rejected."""
    def mutate(manifest):
        manifest["reports"][0]["digest"] = "0" * 64

    with tempfile.TemporaryDirectory() as tmp:
        manifest = _write_manifest(tmp, _valid_sources(), mutate=mutate)
        code, stderr = _run(manifest, "pre-review")
        assert code == 1
        assert "FAIL report.family-a.digest:" in stderr


def test_rejects_per_entry_topology_mismatch():
    """A report entry whose topology differs from its report file is rejected."""
    def mutate(manifest):
        manifest["reports"][0]["topology"] = {"topology_id": "six-pod", "oracle_pods": 6}

    with tempfile.TemporaryDirectory() as tmp:
        manifest = _write_manifest(tmp, _valid_sources(), mutate=mutate)
        code, stderr = _run(manifest, "pre-review")
        assert code == 1
        assert "FAIL topology.family-a:" in stderr


def test_rejects_non_qualification_tier_report():
    """A linked report whose tier is not qualification is rejected."""
    smoke = FIXTURES / "invalid" / "smoke-tier.json"
    with tempfile.TemporaryDirectory() as tmp:
        manifest = _write_manifest(tmp, [smoke, VALID_REPORT, VALID_REPORT, VALID_REPORT])
        code, stderr = _run(manifest, "pre-review")
        assert code == 1
        assert "FAIL tier.family-a:" in stderr


def test_rejects_missing_stage_execution():
    """A report stage missing stage_execution evidence is rejected."""
    missing = FIXTURES / "invalid" / "missing-stage-execution.json"
    with tempfile.TemporaryDirectory() as tmp:
        manifest = _write_manifest(tmp, [missing, VALID_REPORT, VALID_REPORT, VALID_REPORT])
        code, stderr = _run(manifest, "pre-review")
        assert code == 1
        assert "FAIL stage_execution.family-a#0:" in stderr


def test_rejects_recommended_without_saturation():
    """A report body with a recommended block but no saturation is rejected."""
    recommended = FIXTURES / "invalid" / "recommended-without-saturation.json"
    with tempfile.TemporaryDirectory() as tmp:
        manifest = _write_manifest(tmp, [recommended, VALID_REPORT, VALID_REPORT, VALID_REPORT])
        code, stderr = _run(manifest, "pre-review")
        assert code == 1
        assert "FAIL recommended.family-a:" in stderr


def test_rejects_failed_stage_correctness():
    """A report stage whose correctness did not pass is rejected."""
    failed = FIXTURES / "invalid" / "failed-correctness.json"
    with tempfile.TemporaryDirectory() as tmp:
        manifest = _write_manifest(tmp, [failed, VALID_REPORT, VALID_REPORT, VALID_REPORT])
        code, stderr = _run(manifest, "pre-review")
        assert code == 1
        assert "FAIL correctness.family-a#0:" in stderr


def load_tests(loader, standard_tests, pattern):
    """Wrap each top-level ``test_*`` function into a ``unittest`` suite.

    Repository rules forbid ``TestCase`` subclasses, so the stdlib runner cannot
    auto-collect the module-level functions. This hook wraps each into a
    ``FunctionTestCase`` so ``python -m unittest`` discovers and runs them.
    """
    suite = unittest.TestSuite()
    for name, value in sorted(globals().items()):
        if name.startswith("test_") and callable(value):
            suite.addTest(unittest.FunctionTestCase(value))
    return suite
