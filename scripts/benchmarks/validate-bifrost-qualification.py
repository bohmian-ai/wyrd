#!/usr/bin/env python3
"""Validate a Bifrost qualification manifest and its linked capacity reports.

Two phases share one internally-consistent core. ``--phase pre-review`` accepts a
clean manifest whose review reference and ``evidence_commit`` are still absent;
``--phase sealed`` additionally requires the sealed review reference, its digest,
and the recorded ``evidence_commit``. Both phases reject: missing or changed
report digests; source, dataset, workload, or topology mismatch; a dirty source
tree; failed correctness or an unrecovered audit backlog; a non-``pod_local_v1``
accounting marker; any report or stage whose tier is not ``qualification``; any
stage missing or inconsistent ``stage_execution`` evidence; a ``recommended``
block present when no saturation boundary was reached; and budget breaches.

Exit code 0 means the manifest passed its phase; exit code 1 means at least one
check failed. Every failure prints ``FAIL <field>: <detail>`` to stderr and all
failures in a run are reported before exiting, so one invocation surfaces the
complete diagnostic set. Standard library only.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

MANIFEST_SCHEMA = "wyrd.bifrost.qualification/v1"
REPORT_SCHEMA = "wyrd.bifrost.capacity/v1"
ACCOUNTING_MARKER = "pod_local_v1"
QUALIFICATION_TIER = "qualification"
EXPECTED_REPORT_COUNT = 4


def _is_hex(value: object, length: int) -> bool:
    """Return whether ``value`` is a lowercase hex string of exactly ``length``."""
    return (
        isinstance(value, str)
        and len(value) == length
        and all(character in "0123456789abcdef" for character in value)
    )


class Validator:
    """Collects field-diagnostic failures while validating one manifest.

    The validator accumulates every failure in :attr:`failures` rather than
    stopping at the first, so a single run reports the complete diagnostic set.
    Each failure is a ``(field, detail)`` pair rendered as ``FAIL <field>:
    <detail>`` on stderr by :meth:`report`.
    """

    def __init__(self, manifest_path: Path, phase: str) -> None:
        """Bind the manifest path and requested phase for this validation."""
        self.manifest_path = manifest_path
        self.phase = phase
        self.root = manifest_path.parent
        self.failures: list[tuple[str, str]] = []

    def fail(self, field: str, detail: str) -> None:
        """Record one field-diagnostic failure."""
        self.failures.append((field, detail))

    def run(self) -> int:
        """Validate the manifest and return the process exit code (0 or 1)."""
        try:
            raw = self.manifest_path.read_bytes()
        except OSError as error:
            self.fail("manifest", f"cannot read manifest: {error}")
            return self.report()
        try:
            manifest = json.loads(raw)
        except json.JSONDecodeError as error:
            self.fail("manifest", f"invalid JSON: {error}")
            return self.report()
        if not isinstance(manifest, dict):
            self.fail("manifest", "manifest is not a JSON object")
            return self.report()

        self._check_manifest_identity(manifest)
        self._check_phase(manifest)
        self._check_budgets(manifest)
        self._check_verdicts(manifest)
        self._check_reports(manifest)
        return self.report()

    def _check_manifest_identity(self, manifest: dict) -> None:
        """Validate the manifest envelope, source binding, and accounting marker."""
        if manifest.get("schema_version") != MANIFEST_SCHEMA:
            self.fail(
                "schema_version",
                f"expected {MANIFEST_SCHEMA}, got {manifest.get('schema_version')!r}",
            )
        if not _is_hex(manifest.get("qualified_source_commit"), 40):
            self.fail(
                "qualified_source_commit",
                "must be 40 hexadecimal characters",
            )
        if manifest.get("source_tree_clean") is not True:
            self.fail("source_tree_clean", "qualified source tree was not clean")
        if manifest.get("oracle_query_accounting") != ACCOUNTING_MARKER:
            self.fail(
                "oracle_query_accounting",
                f"expected {ACCOUNTING_MARKER}, got "
                f"{manifest.get('oracle_query_accounting')!r}",
            )
        if not _is_hex(manifest.get("dataset_digest"), 64):
            self.fail("dataset_digest", "must be a 64-character SHA-256 digest")
        if not _is_hex(manifest.get("workload_digest"), 64):
            self.fail("workload_digest", "must be a 64-character SHA-256 digest")

    def _check_phase(self, manifest: dict) -> None:
        """Enforce the review/evidence presence contract for the active phase."""
        review = manifest.get("review")
        evidence = manifest.get("evidence_commit")
        if self.phase == "pre-review":
            if review is not None:
                self.fail("review", "review reference must be absent pre-review")
            if evidence is not None:
                self.fail(
                    "evidence_commit",
                    "evidence_commit must be absent pre-review",
                )
        else:
            if not isinstance(review, dict):
                self.fail("review", "sealed manifest requires a review reference")
            else:
                if not isinstance(review.get("path"), str) or not review["path"]:
                    self.fail("review.path", "review path must be a non-empty string")
                if not _is_hex(review.get("sha256"), 64):
                    self.fail(
                        "review.sha256",
                        "review digest must be a 64-character SHA-256 digest",
                    )
            if not _is_hex(evidence, 40):
                self.fail(
                    "evidence_commit",
                    "sealed manifest requires a 40-hex evidence_commit",
                )

    def _check_budgets(self, manifest: dict) -> None:
        """Reject actual durations or disk usage that breach recorded budgets."""
        budgets = manifest.get("budgets")
        actual = manifest.get("actual_durations")
        if not isinstance(budgets, dict) or not isinstance(actual, dict):
            self.fail("budgets", "budgets and actual_durations must be objects")
            return
        pairs = (
            ("setup", "setup_seconds", "setup"),
            ("total", "total_seconds", "total"),
            ("disk_bytes", "disk_bytes_used", "disk"),
        )
        for budget_key, actual_key, label in pairs:
            budget = budgets.get(budget_key)
            used = actual.get(actual_key)
            if not isinstance(budget, int) or not isinstance(used, int):
                self.fail(f"budget.{label}", "budget and actual must be integers")
                continue
            if used > budget:
                self.fail(
                    f"budget.{label}",
                    f"{label} budget breached: {used} > {budget}",
                )

    def _check_verdicts(self, manifest: dict) -> None:
        """Reject a run whose rolled-up correctness or recovery verdict failed."""
        verdicts = manifest.get("verdicts")
        if not isinstance(verdicts, dict):
            self.fail("verdicts", "verdicts must be an object")
            return
        if verdicts.get("correctness_passed") is not True:
            self.fail("verdicts.correctness_passed", "a family failed correctness")
        if verdicts.get("recovery_recovered") is not True:
            self.fail(
                "verdicts.recovery_recovered",
                "a recovery replay did not recover",
            )

    def _check_reports(self, manifest: dict) -> None:
        """Validate the linked report set, digests, and each report's contents."""
        reports = manifest.get("reports")
        if not isinstance(reports, list):
            self.fail("reports", "reports must be a list")
            return
        if len(reports) != EXPECTED_REPORT_COUNT:
            self.fail(
                "reports",
                f"expected {EXPECTED_REPORT_COUNT} linked reports, got {len(reports)}",
            )
        for entry in reports:
            self._check_report_entry(manifest, entry)

    def _check_report_entry(self, manifest: dict, entry: object) -> None:
        """Validate one linked report entry's digest and report contents."""
        if not isinstance(entry, dict):
            self.fail("reports", "report entry must be an object")
            return
        family = entry.get("family")
        label = family if isinstance(family, str) and family else "<unknown>"
        path = entry.get("path")
        if not isinstance(path, str) or not path:
            self.fail(f"report.{label}.path", "report path must be a non-empty string")
            return
        report_path = self.root / path
        try:
            report_bytes = report_path.read_bytes()
        except OSError as error:
            self.fail(f"report.{label}.path", f"cannot read report: {error}")
            return
        digest = hashlib.sha256(report_bytes).hexdigest()
        if digest != entry.get("digest"):
            self.fail(
                f"report.{label}.digest",
                f"digest mismatch: recomputed {digest}, recorded {entry.get('digest')!r}",
            )
        try:
            report = json.loads(report_bytes)
        except json.JSONDecodeError as error:
            self.fail(f"report.{label}", f"invalid JSON: {error}")
            return
        if not isinstance(report, dict):
            self.fail(f"report.{label}", "report is not a JSON object")
            return
        self._check_report_contents(manifest, label, report)

    def _check_report_contents(self, manifest: dict, label: str, report: dict) -> None:
        """Cross-check one report's identity, tier, and every measured stage."""
        if report.get("schema_version") != REPORT_SCHEMA:
            self.fail(f"report.{label}.schema_version", "unexpected report schema")
        if report.get("tier") != QUALIFICATION_TIER:
            self.fail(
                f"tier.{label}",
                f"report tier must be {QUALIFICATION_TIER}, got {report.get('tier')!r}",
            )
        if report.get("source_commit") != manifest.get("qualified_source_commit"):
            self.fail(f"source_commit.{label}", "report source commit differs from manifest")
        if report.get("tree_clean") is not True:
            self.fail(f"tree_clean.{label}", "report source tree was not clean")
        if report.get("dataset_digest") != manifest.get("dataset_digest"):
            self.fail(f"dataset_digest.{label}", "report dataset digest differs from manifest")
        if report.get("workload_digest") != manifest.get("workload_digest"):
            self.fail(f"workload_digest.{label}", "report workload digest differs from manifest")
        if report.get("topology") != manifest.get("topology"):
            self.fail(f"topology.{label}", "report topology differs from manifest")

        stages = report.get("stages")
        if not isinstance(stages, list) or not stages:
            self.fail(f"stages.{label}", "report has no measured stages")
        else:
            for index, stage in enumerate(stages):
                self._check_stage(label, index, stage)

        body = report.get("body")
        if not isinstance(body, dict):
            self.fail(f"body.{label}", "report body must be an object")
            return
        saturation_reached = body.get("saturation_reached")
        recommended = body.get("recommended")
        if recommended is not None and saturation_reached is not True:
            self.fail(
                f"recommended.{label}",
                "recommended block present without a saturation boundary",
            )

    def _check_stage(self, label: str, index: int, stage: object) -> None:
        """Validate one stage's execution binding, correctness, and audit backlog."""
        tag = f"{label}#{index}"
        if not isinstance(stage, dict):
            self.fail(f"stage.{tag}", "stage must be an object")
            return
        ordinal = stage.get("ordinal")
        execution = stage.get("stage_execution")
        if not isinstance(execution, dict):
            self.fail(f"stage_execution.{tag}", "stage is missing stage_execution")
        else:
            if not isinstance(execution.get("run_id"), str) or not execution["run_id"]:
                self.fail(f"stage_execution.{tag}", "stage_execution run_id is empty")
            if execution.get("ordinal") != ordinal:
                self.fail(
                    f"stage_execution.{tag}",
                    "stage_execution ordinal disagrees with stage ordinal",
                )
            window = execution.get("capture_window")
            if not isinstance(window, dict):
                self.fail(f"stage_execution.{tag}", "capture_window is missing")
            else:
                started = window.get("started_at_micros")
                ended = window.get("ended_at_micros")
                if not isinstance(window.get("window_id"), str) or not window["window_id"]:
                    self.fail(f"stage_execution.{tag}", "capture_window window_id is empty")
                if (
                    not isinstance(started, int)
                    or not isinstance(ended, int)
                    or ended <= started
                ):
                    self.fail(f"stage_execution.{tag}", "capture_window is not a positive range")
            digests = execution.get("observation_digests")
            if (
                not isinstance(digests, list)
                or not digests
                or not all(_is_hex(digest, 64) for digest in digests)
            ):
                self.fail(
                    f"stage_execution.{tag}",
                    "observation_digests must be a non-empty list of SHA-256 digests",
                )

        if stage.get("correctness") != "passed":
            self.fail(f"correctness.{tag}", "stage correctness did not pass")

        audit = stage.get("audit")
        if not isinstance(audit, dict):
            self.fail(f"audit.{tag}", "stage audit sample is missing")
        elif audit.get("backlog_records") or audit.get("backlog_bytes"):
            self.fail(f"audit.{tag}", "stage ended with an unrecovered audit backlog")

        replay = stage.get("recovery_replay")
        if replay is not None and replay != "passed":
            self.fail(f"recovery.{tag}", "recovery replay did not pass")

    def report(self) -> int:
        """Print all recorded failures to stderr and return the exit code."""
        for field, detail in self.failures:
            print(f"FAIL {field}: {detail}", file=sys.stderr)
        return 1 if self.failures else 0


def main(argv: list[str] | None = None) -> int:
    """Parse arguments and validate the named manifest for the requested phase.

    Returns the process exit code: 0 when the manifest passes its phase, 1 when
    any check fails.
    """
    parser = argparse.ArgumentParser(description="Validate a Bifrost qualification manifest.")
    parser.add_argument("manifest", type=Path, help="path to qualification-manifest.json")
    parser.add_argument(
        "--phase",
        required=True,
        choices=("pre-review", "sealed"),
        help="validation phase",
    )
    args = parser.parse_args(argv)
    return Validator(args.manifest, args.phase).run()


if __name__ == "__main__":
    raise SystemExit(main())
